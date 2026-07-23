//! Operating a door: open, close, and auto-close (§10.4).
//!
//! Generation (§10.1.4) cuts the doorways and records each as a hinged, panelled
//! [`Door`](crate::Door) edge in the region graph. This module is the *runtime*
//! half — the bump interactions that open and close them. A door touches two things
//! at once: the graph's open/closed state and the panels' terrain on the grid. Both
//! must move together (an open door whose panels still read `+` would lie to vision,
//! sound, and the renderer), so the operations live on [`Layout`], the one type that
//! owns both.
//!
//! **Bump a panel to open, bump a hinge to close** (§10.4) — and, since #148, a
//! *closed* hinge opens the door too, a second way in from beside the frame. The
//! hinge is the handle, which is why hinges stay solid forever. A door refuses to
//! close while anything occupies a panel — doors never crush anyone.
//!
//! Doors also close on their own, two ways (§10.4). A **manual** hinged door is shut
//! by a Calm guard passing through it ([`close_behind`](Layout::close_behind), #146).
//! An **automatic** door ([`DoorKind::Automatic`]) has no hinges — the whole span is
//! panels, so there is no handle — and shuts *itself* a few turns after the doorway is
//! last vacated ([`tick_auto_doors`](Layout::tick_auto_doors), #147). Both are what
//! stop an opened door decaying the level into an open plan, and both make an open
//! door read as evidence someone came this way.
//!
//! Occupancy is supplied by the caller: "is a panel occupied?" is a predicate the
//! turn loop passes in, built from the live actors (the player, guards, bodies).

use crate::cell::Cell;
use crate::facility::Terrain;
use crate::region::{DoorCell, DoorId, DoorKind};
use crate::Layout;

/// What operating a door did (§10.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DoorAction {
    /// A closed door was opened — its panels are now walk-through.
    Opened,
    /// An open door was closed — its panels are solid again.
    Closed,
    /// A close was refused because an actor occupies a panel; the door stays open.
    Obstructed,
}

impl Layout {
    /// Bump `cell` as the interaction verb (§4.3): if it is a door cell, operate the
    /// door and report what happened.
    ///
    /// - Bumping a **closed panel** opens the door.
    /// - Bumping a **closed hinge** *also* opens it (#148): the frame is a second
    ///   way in — you crack the door from beside it without stepping into the new
    ///   sightline. (The player then auto-faces the opening for a peek; that facing
    ///   turn is the caller's, §5-exception, not the door's.)
    /// - Bumping an **open hinge** closes it — unless a panel is occupied, which
    ///   refuses the close ([`DoorAction::Obstructed`]).
    /// - Every other case is *not* a door action and returns `None`: an **open
    ///   panel** you simply walk through, or a cell that is no door at all.
    ///
    /// `occupied(cell)` reports whether an actor stands on `cell`.
    pub fn bump_door(&mut self, cell: Cell, occupied: impl Fn(Cell) -> bool) -> Option<DoorAction> {
        let id = self.regions().door_at(cell)?;
        match self.preview_door_bump(cell, &occupied)? {
            DoorAction::Opened => {
                self.set_door_open(id, true);
                Some(DoorAction::Opened)
            }
            // The close verdict (Closed vs. the refused Obstructed) is re-derived by
            // `close_door`, which owns the crush-safety check and the mutation.
            DoorAction::Closed | DoorAction::Obstructed => Some(self.close_door(id, occupied)),
        }
    }

    /// What bumping `cell` *would* do to a door, touching nothing — the read-only twin
    /// of [`bump_door`], so the usable line (§11.4) can predict the exact outcome the
    /// bump will produce. Returns `None` for the non-door-action cases `bump_door`
    /// also declines: an open panel (walked through) or a cell that is no door at all.
    pub fn preview_door_bump(
        &self,
        cell: Cell,
        occupied: impl Fn(Cell) -> bool,
    ) -> Option<DoorAction> {
        let id = self.regions().door_at(cell)?;
        let door = self.regions().door(id);
        match (door.role(cell)?, door.is_open()) {
            // A closed door opens from either handle (#148): a panel (walk in) or a
            // hinge (crack the frame). All panels swing as one unit regardless.
            (DoorCell::Panel | DoorCell::Hinge, false) => Some(DoorAction::Opened),
            (DoorCell::Hinge, true) => Some(if door.panels().iter().any(|&c| occupied(c)) {
                DoorAction::Obstructed
            } else {
                DoorAction::Closed
            }),
            // An open panel is walked through, not operated.
            (DoorCell::Panel, true) => None,
        }
    }

    /// Advance every open **automatic** door's close timer one turn and shut those
    /// whose countdown reaches zero (§10.4/#147) — the once-per-turn tick the world
    /// phase runs after everyone has moved. An open automatic door whose doorway is
    /// **occupied** rearms its timer to the door's `delay` (an actor on a panel holds
    /// it open — automatic doors never crush); a **vacant** one counts down, and on
    /// the turn it would reach zero it closes, its panels restamped solid exactly as a
    /// manual close does. Returns the doors that shut this tick, so the caller can
    /// surface them as events. Manual doors and closed doors are untouched.
    pub(crate) fn tick_auto_doors(&mut self, occupied: impl Fn(Cell) -> bool) -> Vec<DoorId> {
        let ids: Vec<DoorId> = self.regions().doors().map(|(id, _)| id).collect();
        let mut closed = Vec::new();
        for id in ids {
            // Read what the tick needs, then drop the borrow before mutating.
            let (delay, occupied_now, timer) = {
                let door = self.regions().door(id);
                let DoorKind::Automatic { delay } = door.kind() else {
                    continue;
                };
                if !door.is_open() {
                    continue;
                }
                let occupied_now = door.panels().iter().any(|&c| occupied(c));
                (delay, occupied_now, door.auto_timer())
            };
            if occupied_now {
                // An actor in the doorway holds it open — rearm the full delay.
                self.parts_mut().1.door_mut(id).set_auto_timer(delay);
            } else if timer <= 1 {
                // The countdown is spent and the doorway is clear: shut it.
                self.set_door_open(id, false);
                closed.push(id);
            } else {
                self.parts_mut().1.door_mut(id).set_auto_timer(timer - 1);
            }
        }
        closed
    }

    /// Close `door` behind a guard that has just walked through it (§10.4, #146):
    /// a **Calm** guard restoring the level's structure, so an open door stays
    /// meaningful as evidence someone passed. Closes only a door that is currently
    /// open, and still refuses on an occupied panel — the crush rule holds for a
    /// guard-close exactly as for a bump ([`DoorAction::Obstructed`]). Returns
    /// `None` for a door that is already closed; the caller (the turn loop) owns the
    /// Calm-only gate and the seeded probability (§7.6/§12.4), so this is just the
    /// mutation. Automatic doors are never guard-closed — they have no handle and shut
    /// themselves ([`tick_auto_doors`](Self::tick_auto_doors)) — so the caller only
    /// hands manual doors here.
    pub(crate) fn close_behind(
        &mut self,
        door: DoorId,
        occupied: impl Fn(Cell) -> bool,
    ) -> Option<DoorAction> {
        if !self.regions().door(door).is_open() {
            return None;
        }
        Some(self.close_door(door, occupied))
    }

    /// Open `door` as an **initial generation state** (#145, §10.4): move the graph
    /// flag and its panels' terrain to open together, exactly as a bump would. No
    /// occupancy check — generation opens doors before any actor is placed, so
    /// there is never anything to crush — and no auto-close side effect: this sets a
    /// starting *pose*, not a runtime interaction. Door open/closed is live state
    /// (§11.3), which is why it is layered on here rather than baked into terrain by
    /// the carve.
    pub(crate) fn open_door_initial(&mut self, door: DoorId) {
        self.set_door_open(door, true);
    }

    /// Close `door` unless a panel is occupied, restamping the panels solid. Refuses
    /// (leaving the door open) when an actor stands on a panel — doors never crush.
    fn close_door(&mut self, door: DoorId, occupied: impl Fn(Cell) -> bool) -> DoorAction {
        let panels: Vec<Cell> = self.regions().door(door).panels().to_vec();
        if panels.iter().any(|&c| occupied(c)) {
            return DoorAction::Obstructed;
        }
        self.set_door_open(door, false);
        DoorAction::Closed
    }

    /// Move a door's open state and its panels' terrain in one step, so the grid the
    /// game reads always agrees with the graph. Opening an **automatic** door arms its
    /// close timer to the door's `delay` (§10.4/#147), so the countdown starts the
    /// moment it opens however that happened — a bump, a guard walking in, or the
    /// generator's initial pose.
    fn set_door_open(&mut self, door: DoorId, open: bool) {
        let panels: Vec<Cell> = self.regions().door(door).panels().to_vec();
        let arm = match (open, self.regions().door(door).kind()) {
            (true, DoorKind::Automatic { delay }) => Some(delay),
            _ => None,
        };
        let terrain = if open {
            Terrain::DoorPanelOpen
        } else {
            Terrain::DoorPanelClosed
        };
        let (facility, regions) = self.parts_mut();
        regions.door_mut(door).set_open(open);
        if let Some(delay) = arm {
            regions.door_mut(door).set_auto_timer(delay);
        }
        for panel in panels {
            facility.set_terrain(panel.x, panel.y, terrain);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::facility::Facility;
    use crate::region::{DoorId, DoorKind, RegionGraph, RegionKind};
    use crate::test_support::seed_sweep;
    use crate::{generate, Cell, DoorAction, Layout, Rng, Terrain};

    /// Nothing is ever occupied — the common case for the door-mechanics tests.
    fn vacant(_: Cell) -> bool {
        false
    }

    /// A generated 40×40 facility always has doors, each a closed span stamped into
    /// the grid — and its structure follows its kind (§10.1.4, §10.4/#147): a
    /// **manual** door is two hinges around 1–4 panels, an **automatic** door is 3–6
    /// panels and *no* hinges (the frameless span).
    #[test]
    fn generation_places_closed_doors_by_kind() {
        let layout = generate(40, 40, &mut Rng::new(7)).unwrap();
        let regions = layout.regions();
        assert!(regions.door_count() > 0, "a facility must have doorways");
        let facility = layout.facility();

        for (_, door) in regions.doors() {
            assert!(!door.is_open(), "doors start closed");
            match door.kind() {
                DoorKind::Manual => {
                    assert_eq!(door.hinges().len(), 2, "a manual door: a hinge at each end");
                    assert!(
                        (1..=4).contains(&door.panels().len()),
                        "1–4 panels, got {}",
                        door.panels().len()
                    );
                    for &h in door.hinges() {
                        assert_eq!(facility.terrain_at(h.x, h.y), Some(Terrain::DoorHinge));
                    }
                }
                DoorKind::Automatic { .. } => {
                    assert!(door.hinges().is_empty(), "an automatic door has no hinges");
                    assert!(
                        (3..=6).contains(&door.panels().len()),
                        "3–6 panels spanning the doorway, got {}",
                        door.panels().len()
                    );
                }
            }
            for &p in door.panels() {
                assert_eq!(
                    facility.terrain_at(p.x, p.y),
                    Some(Terrain::DoorPanelClosed)
                );
            }
        }
    }

    /// Every door joins a room to a corridor, never a room to a room (§10.1.4).
    #[test]
    fn every_door_joins_a_room_to_a_corridor() {
        for seed in seed_sweep(64) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let regions = layout.regions();
            for (_, door) in regions.doors() {
                let [a, b] = door.regions();
                let kinds = [regions.kind(a), regions.kind(b)];
                assert!(
                    kinds.contains(&RegionKind::Room) && kinds.contains(&RegionKind::Corridor),
                    "seed {seed}: door joins {kinds:?}, not a room to a corridor"
                );
            }
        }
    }

    /// A generated facility and the handle of its first **manual** door, for the
    /// hinge/panel mechanics tests. Skips any automatic (frameless) door, which has
    /// no hinge to bump (§10.4/#147); a generated level always has manual doors too.
    fn one_door() -> (Layout, DoorId) {
        let layout = generate(40, 40, &mut Rng::new(7)).unwrap();
        let door = layout
            .regions()
            .doors()
            .find(|(_, d)| !d.is_automatic())
            .expect("a facility has manual doors")
            .0;
        (layout, door)
    }

    /// A tiny two-room layout joined by one **automatic** door with close `delay`:
    /// no hinges, a 3-cell panel span down wall column 4. The fixture for the
    /// auto-close timer tests (§10.4/#147).
    fn one_auto_door(delay: u32) -> (Layout, DoorId) {
        let cells = |xs: std::ops::Range<u32>| {
            xs.flat_map(|x| (1..4).map(move |y| Cell::new(x, y)))
                .collect::<Vec<_>>()
        };
        let mut f = Facility::walled_box(9, 5);
        let mut g = RegionGraph::new(9, 5);
        let left = g.add_region(RegionKind::Room, cells(1..4));
        let right = g.add_region(RegionKind::Room, cells(5..8));
        let panels: Vec<Cell> = (1..4).map(|y| Cell::new(4, y)).collect();
        for &p in &panels {
            f.set_terrain(p.x, p.y, Terrain::DoorPanelClosed);
        }
        let door = g.add_door(left, right, [], panels, DoorKind::Automatic { delay });
        (Layout::from_parts(f, g), door)
    }

    #[test]
    fn bumping_a_panel_opens_and_bumping_a_hinge_closes() {
        let (mut layout, door) = one_door();
        let panel = layout.regions().door(door).panels()[0];
        let hinge = layout.regions().door(door).hinges()[0];

        // Bump a panel: the door opens and every panel becomes walk-through.
        assert_eq!(layout.bump_door(panel, vacant), Some(DoorAction::Opened));
        assert!(layout.regions().door(door).is_open());
        for &p in layout.regions().door(door).panels() {
            assert_eq!(
                layout.facility().terrain_at(p.x, p.y),
                Some(Terrain::DoorPanelOpen)
            );
        }

        // Bump a hinge: the door closes and the panels are solid again.
        assert_eq!(layout.bump_door(hinge, vacant), Some(DoorAction::Closed));
        assert!(!layout.regions().door(door).is_open());
        for &p in layout.regions().door(door).panels() {
            assert_eq!(
                layout.facility().terrain_at(p.x, p.y),
                Some(Terrain::DoorPanelClosed)
            );
        }
    }

    /// #148: a *closed* hinge opens the door too — the frame is a second way in. All
    /// panels swing as one unit, exactly as a panel bump does, and the preview
    /// predicts it so the usable line can offer `door: open` (§11.4).
    #[test]
    fn bumping_a_closed_hinge_opens_the_door() {
        let (mut layout, door) = one_door();
        let hinge = layout.regions().door(door).hinges()[0];

        // The read-only preview agrees with what the bump will do (§11.4).
        assert_eq!(
            layout.preview_door_bump(hinge, vacant),
            Some(DoorAction::Opened),
            "a closed hinge previews as an open"
        );

        assert_eq!(layout.bump_door(hinge, vacant), Some(DoorAction::Opened));
        assert!(layout.regions().door(door).is_open());
        for &p in layout.regions().door(door).panels() {
            assert_eq!(
                layout.facility().terrain_at(p.x, p.y),
                Some(Terrain::DoorPanelOpen),
                "every panel swings open as one unit"
            );
        }

        // The now-open hinge closes again (the unchanged behaviour) — so the frame is
        // a toggle: closed→open on this bump, open→closed on the next.
        assert_eq!(layout.bump_door(hinge, vacant), Some(DoorAction::Closed));
        assert!(!layout.regions().door(door).is_open());
    }

    #[test]
    fn one_bump_moves_every_panel_as_a_unit() {
        let (mut layout, door) = one_door();
        // Bump exactly one panel; assert *all* of them open together.
        let panel = layout.regions().door(door).panels()[0];
        layout.bump_door(panel, vacant);
        let all_open = layout
            .regions()
            .door(door)
            .panels()
            .iter()
            .all(|p| layout.facility().terrain_at(p.x, p.y) == Some(Terrain::DoorPanelOpen));
        assert!(all_open, "panels open as one unit");
    }

    #[test]
    fn a_door_refuses_to_close_on_an_occupant() {
        let (mut layout, door) = one_door();
        let panel = layout.regions().door(door).panels()[0];
        let hinge = layout.regions().door(door).hinges()[0];
        layout.bump_door(panel, vacant); // open it

        // Someone stands on a panel; bumping the hinge refuses to close.
        assert_eq!(
            layout.bump_door(hinge, |c| c == panel),
            Some(DoorAction::Obstructed)
        );
        assert!(
            layout.regions().door(door).is_open(),
            "stays open on an occupant"
        );
        assert_eq!(
            layout.facility().terrain_at(panel.x, panel.y),
            Some(Terrain::DoorPanelOpen),
            "the occupant is never crushed shut"
        );

        // Step off, and it closes.
        assert_eq!(layout.bump_door(hinge, vacant), Some(DoorAction::Closed));
        assert!(!layout.regions().door(door).is_open());
    }

    /// A closed panel is transparent to pathfinding (§10.4): a pathfinder that
    /// consults terrain routes through it exactly as it does an open one.
    #[test]
    fn closed_panels_do_not_block_pathfinding() {
        let (layout, door) = one_door();
        for &p in layout.regions().door(door).panels() {
            let terrain = layout.facility().terrain_at(p.x, p.y).unwrap();
            assert_eq!(terrain, Terrain::DoorPanelClosed);
            assert!(
                !terrain.blocks_pathing(),
                "a pathfinder routes through a closed door"
            );
        }
    }

    #[test]
    fn walking_through_an_open_door_is_not_a_door_action() {
        let (mut layout, door) = one_door();
        let panel = layout.regions().door(door).panels()[0];

        // Open it via the panel. An open panel is then walk-through; bumping it is
        // movement, not a door op.
        layout.bump_door(panel, vacant);
        assert_eq!(
            layout.bump_door(panel, vacant),
            None,
            "open panel walks through"
        );
    }

    /// §10.4/#147: an automatic door opened by a bump arms its close timer and,
    /// once the doorway is clear, shuts itself after exactly `delay` vacant ticks —
    /// its panels restamped solid, exactly as a manual close leaves them.
    #[test]
    fn an_automatic_door_shuts_itself_after_the_delay() {
        let (mut layout, door) = one_auto_door(3);
        let panel = layout.regions().door(door).panels()[0];

        assert_eq!(layout.bump_door(panel, vacant), Some(DoorAction::Opened));
        assert!(layout.regions().door(door).is_open());

        // Two vacant ticks keep it open; the third (delay = 3) shuts it.
        assert!(
            layout.tick_auto_doors(vacant).is_empty(),
            "tick 1: still open"
        );
        assert!(
            layout.tick_auto_doors(vacant).is_empty(),
            "tick 2: still open"
        );
        assert!(layout.regions().door(door).is_open());
        assert_eq!(
            layout.tick_auto_doors(vacant),
            vec![door],
            "tick 3: the door times out",
        );
        assert!(!layout.regions().door(door).is_open());
        for &p in layout.regions().door(door).panels() {
            assert_eq!(
                layout.facility().terrain_at(p.x, p.y),
                Some(Terrain::DoorPanelClosed),
                "a timed-out door restamps its panels solid",
            );
        }
        // A closed automatic door is inert to further ticks.
        assert!(layout.tick_auto_doors(vacant).is_empty());
    }

    /// §10.4/#146: the guard close-behind shuts an open door and, like every close,
    /// refuses on an occupied panel — the crush rule holds whoever the occupant is.
    #[test]
    fn close_behind_shuts_an_open_door_but_never_crushes() {
        let (mut layout, door) = one_door();
        let panel = layout.regions().door(door).panels()[0];

        // A closed door offers nothing to close.
        assert_eq!(layout.close_behind(door, vacant), None, "already closed");

        layout.bump_door(panel, vacant); // a guard opened it walking through
        assert!(layout.regions().door(door).is_open());

        // An occupant in the throat refuses the close — the door never crushes.
        assert_eq!(
            layout.close_behind(door, |c| c == panel),
            Some(DoorAction::Obstructed),
        );
        assert!(
            layout.regions().door(door).is_open(),
            "stays open on an occupant"
        );

        // The throat clear, the guard's close-behind shuts it.
        assert_eq!(layout.close_behind(door, vacant), Some(DoorAction::Closed));
        assert!(!layout.regions().door(door).is_open());
    }

    /// §10.4/#147: an automatic door never shuts on an occupant — an actor on a
    /// panel rearms the timer every tick, so it holds open indefinitely and only
    /// times out once the doorway is finally clear.
    #[test]
    fn an_automatic_door_never_shuts_on_an_occupant() {
        let (mut layout, door) = one_auto_door(2);
        let panel = layout.regions().door(door).panels()[0];
        layout.bump_door(panel, vacant); // open and arm

        // Held in the doorway for far longer than the delay: it never closes.
        for _ in 0..10 {
            assert!(layout.tick_auto_doors(|c| c == panel).is_empty());
            assert!(
                layout.regions().door(door).is_open(),
                "an occupant holds it open",
            );
        }

        // Step clear and it times out after the delay (2 ticks).
        assert!(layout.tick_auto_doors(vacant).is_empty(), "tick 1 of 2");
        assert_eq!(layout.tick_auto_doors(vacant), vec![door], "times out");
        assert!(!layout.regions().door(door).is_open());
    }
}
