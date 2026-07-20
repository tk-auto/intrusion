//! Sound: emission and propagation (§9.1–9.2).
//!
//! **Guards were deaf — this is the substrate that ends that** (§9). The design
//! calls sound "the single largest missing system": it is how the player steers
//! guard attention, and how haste is punished. This module is the *data* half —
//! it does **not** wire guards, which have no reactions yet (`state.rs`, the
//! guard-AI tickets own that). It gives the world two things:
//!
//! - **Emission** — the [`Loudness`] vocabulary and the [`Sound`] an action makes.
//!   Which action makes which noise lives in the turn loop ([`crate::State::step`]);
//!   the *magnitudes* live here, on [`Loudness::intensity`], because §9.2 is the
//!   game's primary tuning surface and this is where a tuner reaches.
//! - **Propagation** — [`audible_field`], which spreads a sound **cell to cell
//!   through sound-passable space, cardinally, losing intensity per step**
//!   (§9.1 **[SETTLED]**). The key property is that **sound flows around walls,
//!   not through them**: a sound's reach is its *path* distance, not its
//!   straight-line distance. Closed doors [`Attenuate`](SoundBlocking::Attenuates),
//!   which is what gives "close the door behind you" a point.
//!
//! Both the sound presentation (the renderer showing the player a noise) and the
//! future guard-hearing check (`intensity at my cell > threshold → Investigating`,
//! §9.1) read the same [`Sound`]s and the same [`audible_field`] — one model, two
//! consumers.

use std::collections::BinaryHeap;

use crate::cell::Cell;
use crate::facility::{Facility, SoundBlocking};

/// How loud an action is (§9.2). The qualitative rungs the design names; the
/// turn loop maps each world-changing action to one of these, and the *number*
/// each carries ([`intensity`](Self::intensity)) is the tuning surface.
///
/// `Silent` is a real rung, not the absence of one: waiting, crouching and — once
/// they exist — camouflaged/dephased actions make **no** sound at all (§9.2), and
/// saying so explicitly keeps the "which actions are silent" decision in the type
/// rather than in an omission.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Loudness {
    /// No sound emitted — waiting, crouching, hiding still (§9.2 "None").
    Silent,
    /// Audible only very close — moving normally (§9.2 "Low").
    Low,
    /// A door opening or closing, a takedown, a dropped body (§9.2 "Medium").
    Medium,
    /// Running — carries down a corridor (§9.2 "High").
    High,
}

impl Loudness {
    /// The intensity a sound of this loudness is emitted at, in **cells of reach
    /// on open floor** (§9.2 **[START]**): a sound of intensity `N` is still
    /// (barely) audible `N` cardinal steps away through open space and inaudible
    /// beyond. These numbers are *the* primary tuning surface for the whole game's
    /// tension — pinned by tests so any later change is visible.
    pub fn intensity(self) -> u32 {
        match self {
            Loudness::Silent => 0,
            Loudness::Low => 3,
            Loudness::Medium => 6,
            Loudness::High => 12,
        }
    }
}

/// The extra intensity a **closed door** strips from sound crossing it (§9.1
/// **[START]**), on top of the 1-per-step falloff every cell already costs. A shut
/// door is therefore worth several cells of distance: with the [`Loudness`]
/// numbers above a closed door swallows a whole [`Low`](Loudness::Low) sound and
/// heavily muffles the rest — the point of "close the door behind you".
pub const DOOR_ATTENUATION: u32 = 4;

/// One noise made this turn (§9.2): where it came from and how loud it started.
///
/// `intensity` is the emitted value at the `source` cell before any falloff —
/// [`audible_field`] spreads it outward. A [`Loudness::Silent`] action makes no
/// `Sound` at all, so an emitted `Sound` always has `intensity > 0`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sound {
    /// The cell the noise originates at — the actor's own cell (§9.1).
    pub source: Cell,
    /// The emitted intensity at the source, in cells-of-reach (see [`Loudness`]).
    pub intensity: u32,
}

/// The intensity a [`Sound`] reaches every cell at (§9.1): a dense field over the
/// facility grid, `0` where the sound is inaudible or cannot reach. Produced by
/// [`audible_field`]; read by the sound presentation and by guard hearing.
#[derive(Clone, Debug)]
pub struct AudibleField {
    width: u32,
    height: u32,
    /// Row-major intensity per cell, `0` = inaudible. Same indexing as [`Facility`].
    intensity: Vec<u32>,
}

impl AudibleField {
    /// The intensity the sound reaches `cell` at — `0` if it is inaudible there,
    /// unreachable (behind a wall with no path), or off the grid.
    pub fn intensity_at(&self, cell: Cell) -> u32 {
        if cell.x < self.width && cell.y < self.height {
            self.intensity[(cell.y * self.width + cell.x) as usize]
        } else {
            0
        }
    }

    /// Whether the sound is audible at `cell` at all (intensity strictly above 0).
    pub fn is_audible_at(&self, cell: Cell) -> bool {
        self.intensity_at(cell) > 0
    }
}

/// Propagate `sound` through `facility`, returning the intensity it reaches every
/// cell at (§9.1).
///
/// Sound spreads to cardinal neighbours only (§4.1 — there is no diagonal), losing
/// **1** intensity per step and an extra [`DOOR_ATTENUATION`] whenever it crosses
/// into a closed door ([`SoundBlocking::Attenuates`]). A wall or hinge
/// ([`SoundBlocking::Blocks`]) stops it dead, so it must *flow around* — the field
/// at a cell is its least-attenuated (i.e. shortest, door-weighted) path from the
/// source, **not** its straight-line distance. This is a Dijkstra relaxation that
/// maximises remaining intensity; because every crossing costs at least 1, the
/// first time a cell is popped its value is final, and the resulting field is
/// unique — so the same facility and sound always yield the same field (§12.4).
///
/// Note the sound graph is **not** the movement graph: a console or the exit are
/// solid to a walker yet transparent to sound (§10.3), so a sound crosses them
/// freely. Propagation reads only [`Terrain::sound`](crate::Terrain::sound).
pub fn audible_field(facility: &Facility, sound: Sound) -> AudibleField {
    let width = facility.width();
    let height = facility.height();
    let mut intensity = vec![0u32; (width * height) as usize];
    let idx = |c: Cell| (c.y * width + c.x) as usize;

    // A max-heap keyed on remaining intensity: always settle the loudest frontier
    // cell next, so its value is final when popped (Dijkstra). Ties break on the
    // (x, y) packed into the tuple — deterministic, though the field is unique
    // regardless of order.
    let mut frontier = BinaryHeap::new();
    if sound.intensity > 0 && facility.in_bounds(sound.source) {
        intensity[idx(sound.source)] = sound.intensity;
        frontier.push((sound.intensity, sound.source.x, sound.source.y));
    }

    while let Some((here, x, y)) = frontier.pop() {
        let cell = Cell::new(x, y);
        // A stale heap entry left behind by a later, louder relaxation of `cell`.
        if here < intensity[idx(cell)] {
            continue;
        }
        for n in facility.neighbors(cell) {
            // The cost to cross *into* n depends on what n is made of (§9.1).
            let step = match facility.terrain(n).map(|t| t.sound()) {
                Some(SoundBlocking::Passes) => 1,
                Some(SoundBlocking::Attenuates) => 1 + DOOR_ATTENUATION,
                // A wall/hinge, or off-grid: sound does not cross at all.
                Some(SoundBlocking::Blocks) | None => continue,
            };
            let reached = here.saturating_sub(step);
            if reached > intensity[idx(n)] {
                intensity[idx(n)] = reached;
                if reached > 0 {
                    frontier.push((reached, n.x, n.y));
                }
            }
        }
    }

    AudibleField {
        width,
        height,
        intensity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facility::Terrain;

    /// Build an interior sound source and its field over a hand-stamped box. The
    /// box starts as all-interior floor; `walls` are stamped solid.
    fn field_over(
        w: u32,
        h: u32,
        walls: &[(u32, u32)],
        source: Cell,
        loudness: Loudness,
    ) -> AudibleField {
        let mut facility = Facility::walled_box(w, h);
        for &(x, y) in walls {
            facility.set_terrain(x, y, Terrain::Wall);
        }
        audible_field(
            &facility,
            Sound {
                source,
                intensity: loudness.intensity(),
            },
        )
    }

    /// §9.1 **[SETTLED]** — the headline property: **sound flows around a wall, not
    /// through it.** With a source and target two cells apart in a straight line but
    /// separated by a wall stub, the sound reaches the target at its *path* distance
    /// (6 steps here), not its straight-line distance (2) — and the wall cell itself
    /// stays silent.
    #[test]
    fn sound_flows_around_a_wall_not_through_it() {
        // 7×5 box, interior x∈1..=5, y∈1..=3. A two-cell wall stub at (2,1),(2,2)
        // forces the only path from S=(1,1) to T=(3,1) down and around through (2,3).
        let source = Cell::new(1, 1);
        let target = Cell::new(3, 1);
        let field = field_over(7, 5, &[(2, 1), (2, 2)], source, Loudness::High); // I0 = 12

        // The only path S→T is 6 steps: (1,1)(1,2)(1,3)(2,3)(3,3)(3,2)(3,1).
        assert_eq!(
            field.intensity_at(target),
            12 - 6,
            "reaches the target at its path distance, not straight-line"
        );
        // Straight-line would be 2 steps → intensity 10; it is emphatically not that.
        assert_ne!(field.intensity_at(target), 12 - 2);
        // The wall between them carries no sound.
        assert_eq!(field.intensity_at(Cell::new(2, 1)), 0, "the wall is silent");
    }

    /// §9.1 — intensity falls by exactly 1 per cardinal step across open floor, so a
    /// straight corridor reads off the path distance directly and is monotonic.
    #[test]
    fn intensity_decays_by_one_per_step_on_open_floor() {
        // A single-row corridor, interior x∈1..=8 at y=1; source at the west end.
        // Walk east to the last floor cell (x=8, d=7) — x=9 is the border wall.
        let source = Cell::new(1, 1);
        let field = field_over(10, 3, &[], source, Loudness::High); // I0 = 12
        for d in 0..=7 {
            let cell = Cell::new(1 + d, 1);
            assert_eq!(
                field.intensity_at(cell),
                12u32.saturating_sub(d),
                "intensity at path-distance {d}"
            );
        }
    }

    /// §9.1 — a **closed** door attenuates: crossing one costs an extra
    /// [`DOOR_ATTENUATION`] over an open door on the same path. Same geometry, one
    /// door cell, open vs closed → the intensity beyond it differs by exactly that.
    #[test]
    fn a_closed_door_attenuates_more_than_an_open_one() {
        let source = Cell::new(1, 1);
        let target = Cell::new(5, 1); // 4 steps east along a single row
        let door = Cell::new(3, 1);

        let with = |panel: Terrain| {
            let mut facility = Facility::walled_box(7, 3);
            facility.set_terrain(door.x, door.y, panel);
            audible_field(
                &facility,
                Sound {
                    source,
                    intensity: Loudness::High.intensity(), // 12
                },
            )
            .intensity_at(target)
        };

        let open = with(Terrain::DoorPanelOpen);
        let closed = with(Terrain::DoorPanelClosed);
        assert_eq!(open, 12 - 4, "open door: pure path falloff");
        assert_eq!(closed, 12 - 4 - DOOR_ATTENUATION, "closed door: extra loss");
        assert_eq!(
            open - closed,
            DOOR_ATTENUATION,
            "the closed door is worth exactly its attenuation"
        );
    }

    /// A wall does not merely attenuate — it stops sound. A source sealed behind a
    /// full wall line reaches nothing on the far side.
    #[test]
    fn a_wall_stops_sound_completely() {
        // 5×5 box; wall the whole x=2 interior column, sealing x=1 from x=3.
        let field = field_over(
            5,
            5,
            &[(2, 1), (2, 2), (2, 3)],
            Cell::new(1, 2),
            Loudness::High,
        );
        for y in 1..=3 {
            assert_eq!(
                field.intensity_at(Cell::new(3, y)),
                0,
                "nothing crosses the sealed wall at (3,{y})"
            );
        }
    }

    /// A silent action makes no audible field anywhere — `Silent` is truly zero,
    /// not a faint sound.
    #[test]
    fn a_silent_source_is_audible_nowhere() {
        let field = field_over(6, 6, &[], Cell::new(2, 2), Loudness::Silent);
        assert_eq!(field.intensity_at(Cell::new(2, 2)), 0);
        assert!(!field.is_audible_at(Cell::new(3, 2)));
    }

    /// The §9.2 **[START]** loudness ladder, pinned: a change to any rung is a
    /// deliberate, visible edit.
    #[test]
    fn the_loudness_ladder_matches_9_2() {
        assert_eq!(Loudness::Silent.intensity(), 0);
        assert_eq!(Loudness::Low.intensity(), 3);
        assert_eq!(Loudness::Medium.intensity(), 6);
        assert_eq!(Loudness::High.intensity(), 12);
        // Monotonic by construction — louder rungs reach strictly further.
        assert!(Loudness::Low.intensity() < Loudness::Medium.intensity());
        assert!(Loudness::Medium.intensity() < Loudness::High.intensity());
    }

    /// Determinism (§12.4): the same facility and sound always yield an identical
    /// field, whatever internal ordering the propagation used.
    #[test]
    fn propagation_is_deterministic() {
        let build = || {
            field_over(
                9,
                7,
                &[(3, 1), (3, 2), (5, 4)],
                Cell::new(2, 3),
                Loudness::High,
            )
        };
        assert_eq!(build().intensity, build().intensity);
    }
}
