//! Ability-resolution glue for [`State`](super::State) (§8.3).
//!
//! A second `impl State` holding the per-ability helpers the turn loop calls —
//! decoy spawn/stomp, Run's extra step, Drag's haul, Dephase's rematerialisation
//! check and the eject-and-stun it costs, and the door bump's mutation half — kept
//! out of `state.rs` so that file
//! reads as the phase machinery alone. These are `pub(super)`: visible to the
//! parent turn loop, no wider.

use super::*;

impl State {
    /// Whether the player's current cell can admit them as a **solid** body
    /// again (§8.3): terrain that accepts an actor's fill, and no guard on it.
    /// This is Dephase's rematerialization question — `false` at expiry throws the
    /// player clear and stuns them ([`eject_from_solid`](Self::eject_from_solid)),
    /// and `false` refuses an early toggle-off (there is nowhere to solidify). A body
    /// is non-solid (§7.2), so a cell holding one is still a legal place to stand —
    /// you rematerialise on top of it, and nothing throws you anywhere.
    pub(super) fn can_rematerialize(&self) -> bool {
        // A duct cell is a legal place to be (§10.7): the player already stands there
        // as a solid body, so Dephase expiring inside a duct is *not* the lethal
        // in-wall case — solidifying back into a crawlspace is fine, and the crawl
        // resumes. Terrain `can_enter` rejects it (a duct cell is solid), so it is
        // admitted here explicitly.
        (self.layout.facility().can_enter(self.player, ACTOR_FILL) || self.in_duct())
            && self.guard_at(self.player).is_none()
    }

    /// Throw the player clear of the solid they were about to rematerialize inside,
    /// and leave them stunned (§8.3/#329) — what Dephase's duration now costs instead
    /// of the run.
    ///
    /// The landing cell is drawn **at random from the nearest legal ones**: the
    /// smallest §6.1 box radius around the player that holds any cell a solid body can
    /// stand on, then a uniform pick among the ties from the run's threaded [`Rng`]
    /// (§12.4, so a seed reproduces the landing). Random rather than deterministic on
    /// purpose — a predictable eject would make phasing into a wall a reliable way
    /// *through* it, which is precisely the consequence-free version §8.3 warns about.
    /// You may well be dropped back on the side you came from.
    ///
    /// **The stun is as long as the throw** ([`phase_eject_stun`]): that same radius,
    /// plus a flat base. Clipping the corner of a table is the cheapest eject there
    /// is; burying yourself deep in a wall block is the dearest, because the search
    /// had to reach further to find you anywhere to stand.
    ///
    /// A dragged body does not come along (§8.3): it is released where it lies, since
    /// hauling it through the wall with you would be a free teleport for the one thing
    /// the drag makes expensive.
    ///
    /// [`Event::Entombed`] survives as the degenerate fallback only — a facility with
    /// no standable cell at all, which no generated level can be (§10.6 guarantees the
    /// player started on one). It is kept so that impossible case is a truthful loss
    /// rather than a silently impossible state.
    pub(super) fn eject_from_solid(&mut self, events: &mut Vec<Event>) {
        // The solid they were stranded in, read before anything moves them: one half of
        // the throw the event reports and the mark draws (§11.5/#339).
        let from = self.player;
        let Some(to) = self.eject_target() else {
            self.outcome = Outcome::Lost;
            events.push(Event::Entombed { at: from });
            return;
        };
        // Teleported, not walked: the pose cannot survive a cell it is not adjacent to
        // (§10.3), and the body in your hands stays where it lies.
        self.crouched_behind = None;
        if let Some(i) = self.dragging.take() {
            events.push(Event::BodyReleased {
                at: self.bodies[i].cell(),
            });
        }
        // The stun is priced off the throw itself (§8.3): the §6.1 box distance from
        // the solid to the landing — the very radius the search stopped at — plus the
        // flat base. Measured here, from the two cells, so the price can never drift
        // from the distance actually travelled.
        let stunned = phase_eject_stun(from.sight_distance(to));
        self.player = to;
        self.stunned = stunned;
        events.push(Event::Ejected { from, to, stunned });
        // Anything arriving on the decoy tramples it (§8.3) — arriving by wall
        // included.
        self.stomp_decoy(to, events);
        // The eject lands after this turn's sight phase, so the player's FOV is still
        // cast from inside the wall. Recast it from where they actually are, or the
        // frame would show the `@` in one place and its sight in another (§11.5).
        self.recompute_sight();
    }

    /// The cell [`eject_from_solid`](Self::eject_from_solid) throws the player onto:
    /// a uniform random pick among the cells at the smallest §6.1 box radius that
    /// holds any which can admit a solid body. `None` only when the facility holds no
    /// such cell anywhere.
    ///
    /// The predicate is deliberately *narrower* than
    /// [`can_rematerialize`](Self::can_rematerialize): a duct cell is a legal place to
    /// *be* (§10.7) but never a legal place to be **thrown** — you are spat into a
    /// room, not into a crawlspace you never climbed into. A loose body is non-solid
    /// (§7.2), so its cell is a fine place to land.
    fn eject_target(&mut self) -> Option<Cell> {
        let (width, height) = {
            let facility = self.layout.facility();
            (facility.width(), facility.height())
        };
        // Every in-bounds cell is inside one of these rings, so the search either
        // returns on the first ring holding a candidate or runs out having proved
        // there is none.
        for radius in 1..=width.max(height) {
            let candidates: Vec<Cell> = self
                .ring(radius, width, height)
                .filter(|&cell| {
                    self.layout.facility().can_enter(cell, ACTOR_FILL)
                        && self.guard_at(cell).is_none()
                })
                .collect();
            if candidates.is_empty() {
                continue;
            }
            // The draw happens only on the ring that decides the landing, so the
            // stream advances exactly once per eject (§12.4).
            return Some(candidates[self.rng.below(candidates.len() as u32) as usize]);
        }
        None
    }

    /// The in-bounds cells exactly `radius` away from the player under the §6.1 box
    /// metric — one square ring, walked in a fixed order so the draw off it is
    /// reproducible (§12.4).
    fn ring(&self, radius: u32, width: u32, height: u32) -> impl Iterator<Item = Cell> + '_ {
        let centre = self.player;
        let ys =
            centre.y.saturating_sub(radius)..=(centre.y + radius).min(height.saturating_sub(1));
        let xs = centre.x.saturating_sub(radius)..=(centre.x + radius).min(width.saturating_sub(1));
        ys.flat_map(move |y| xs.clone().map(move |x| Cell::new(x, y)))
            .filter(move |&cell| centre.sight_distance(cell) == radius)
    }

    /// Where a decoy activated right now would spawn (§8.3): the faced cell —
    /// [`TargetingMode::Direction`](crate::TargetingMode) resolved against §5's
    /// facing — provided it could hold an intruder: terrain that admits an
    /// actor's fill and no actor already on it. `None` refuses the activation
    /// (a fake standing in a wall or inside a guard would fool no one).
    pub(super) fn decoy_spawn_cell(&self) -> Option<Cell> {
        let target = self.player.step(self.facing)?;
        (self.layout.facility().can_enter(target, ACTOR_FILL) && !self.occupied(target))
            .then_some(target)
    }

    /// The decoy dies when anything steps onto its cell (§8.3) — called after
    /// every actor arrival, the player's own steps included. Its ability ends
    /// into the **full** cooldown, exactly as an early toggle-off would (§8.2:
    /// refunds nothing), and the death is reported (§11.7).
    pub(super) fn stomp_decoy(&mut self, at: Cell, events: &mut Vec<Event>) {
        if self.decoy == Some(at) {
            self.decoy = None;
            for id in AbilityId::ALL
                .into_iter()
                .filter(|&id| declares(id, Effect::SpawnDecoy))
            {
                self.abilities.deactivate(id);
            }
            events.push(Event::DecoyDied { at });
        }
    }

    /// Run's effect (§8.3, [`Effect::ExtraStep`]): while it is active, a
    /// successful step carries the player one **more** cell the same way in the
    /// same turn — "stepping N times covers 2N cells". The convention chosen
    /// here (the §8.3 row reads "one free move per turn"): the extra move is
    /// **automatic and straight ahead**, and it happens only into a cell that
    /// admits a plain move — a wall, a door, a cupboard, a guard stops the sprint
    /// at one cell rather than auto-bumping (no door flung open, no takedown, no
    /// climb — a sprint never triggers an interaction the player didn't aim, the
    /// §8.4 no-auto-target spirit). A loose body is non-solid (§7.2), so the
    /// sprint runs straight over it — and never picks it up, since taking hold is
    /// the deliberate step *off* a body's cell, which the extra step is not. It
    /// sets facing like any move
    /// (trivially: the same direction) and the whole two-cell step is one spent
    /// turn, so guards still get exactly one turn — the only speed asymmetry in
    /// the game (§7.1: guards never accelerate; §8.3: watch this pair).
    ///
    /// **Dragging suppresses it** (§8.3/#103): Run and Drag must not stack into
    /// fast body-hauling — while dragging, movement caps at the drag's half
    /// speed and the extra step simply never fires.
    pub(super) fn run_extra_step(&mut self, dir: Direction, events: &mut Vec<Event>) {
        if self.dragging.is_some() || !self.abilities.effect_active(Effect::ExtraStep) {
            return;
        }
        let Some(target) = self.player.step(dir) else {
            return;
        };
        if !matches!(self.bump_kind(target), BumpKind::Move) {
            return;
        }
        self.player = target;
        events.push(Event::Moved { to: target });
        self.stomp_decoy(target, events);
    }

    /// Haul the dragged body — if any — into `vacated`, the cell the player is
    /// stepping out of (§8.3). Called by every arm that moves the player, so the
    /// body follows wherever they go: onto floor and through a doorway. The vacated
    /// cell just held the player, so it admits the body; no occupancy re-check is
    /// needed (a body is non-solid anyway, §7.2). Leaves a haul debt: the next spent
    /// turn pays for the weight (the half-speed convention on
    /// [`drag_debt`](Self::drag_debt)). Stowing the body *inside* a cupboard is a
    /// separate, deliberate deposit (§7.2, [`BumpKind::DepositBody`]) — not this
    /// follow-along haul.
    pub(super) fn haul_body_to(&mut self, vacated: Cell) {
        if let Some(i) = self.dragging {
            self.bodies[i].move_to(vacated);
            self.drag_debt = true;
        }
    }

    /// Apply the door operation a bump triggers at `target` — the mutation half of a
    /// [`BumpKind::Door`] classification (the read-only verdict came from
    /// [`bump_kind`](Self::bump_kind)). Fields are captured so the occupancy predicate
    /// can borrow them while `layout` is borrowed `&mut`.
    pub(super) fn operate_door(&mut self, target: Cell) {
        let player = self.player;
        let guards = &self.guards;
        let bodies = &self.bodies;
        self.layout
            .bump_door(target, |c| actor_occupies(player, guards, bodies, c));
    }
}
