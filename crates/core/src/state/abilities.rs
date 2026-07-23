//! Ability-resolution glue for [`State`](super::State) (§8.3).
//!
//! A second `impl State` holding the per-ability helpers the turn loop calls —
//! decoy spawn/stomp, Run's extra step, Drag's haul, Dephase's rematerialisation
//! check, and the door bump's mutation half — kept out of `state.rs` so that file
//! reads as the phase machinery alone. These are `pub(super)`: visible to the
//! parent turn loop, no wider.

use super::*;

impl State {
    /// Whether the player's current cell can admit them as a **solid** body
    /// again (§8.3): terrain that accepts an actor's fill, and no guard or body
    /// on it. This is Dephase's rematerialization question — `false` at expiry
    /// is lethal ([`Event::Entombed`]), and `false` refuses an early toggle-off
    /// (there is nowhere to solidify).
    pub(super) fn can_rematerialize(&self) -> bool {
        self.layout.facility().can_enter(self.player, ACTOR_FILL)
            && self.guard_at(self.player).is_none()
            && self.body_at(self.player).is_none()
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
    /// admits a plain move — a wall, a door, a cupboard, a guard, a body stops
    /// the sprint at one cell rather than auto-bumping (no door flung open, no
    /// takedown, no climb — a sprint never triggers an interaction the player
    /// didn't aim, the §8.4 no-auto-target spirit). It sets facing like any move
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
    /// body follows wherever they go: onto floor, through a doorway, and — the
    /// §7.2 hide flow — *into a cupboard*, by walking through it and out the
    /// other side. The vacated cell just admitted the player (fill 1.0), so it
    /// admits the body; no occupancy re-check is needed. Leaves a haul debt: the
    /// next spent turn pays for the weight (the half-speed convention on
    /// [`drag_debt`](Self::drag_debt)).
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
