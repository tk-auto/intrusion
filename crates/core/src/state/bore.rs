//! Pierce Wall (§8.3/§8.4, #303): bore through the one wall you are standing
//! against, permanently.
//!
//! The whole ability is a **precondition** and a one-cell terrain write. Everything
//! interesting is in the precondition, so it lives here rather than in the turn loop:
//! a pure, derived verdict ([`State::bore_target`]) that the bar reads to colour the
//! entry, the turn loop reads to decide whether the press does anything, and the near
//! line reads to say *why* when it does not — one function, three surfaces, no way
//! for them to disagree (§11.4's derived-function rule).
//!
//! # Why "exactly one adjacent wall" is the design and not a limitation
//!
//! §8.4 [SETTLED] warns that auto-target-nearest-visible "was the path of least
//! resistance" and the direct cause of the old free neutralise. This ability targets
//! without a cursor, which sounds like exactly that mistake, and is not: **the target
//! is unique by precondition, never chosen from candidates.** Where more than one
//! wall touches the player the ability is simply unusable — it never picks one, not
//! by facing, not by nearest, not by anything.
//!
//! What that buys is the balance. A corridor has two side walls and a corner has two
//! walls, so the panic-bore mid-chase is ruled out **by construction** rather than by
//! a cooldown (§7.6): you can only cut a new route from open floor, standing square
//! against a single wall face, which is a deliberate and unhurried act. The ability
//! that survives is a tool for re-routing the facility on your own time, which is a
//! better ability than the escape hatch it would otherwise have been.
//!
//! # What is behind the wall is the player's business
//!
//! The ticket left "what is behind the wall?" open, and the answer is: the ability
//! does not ask. [`thicken_walls`](crate::generate) deliberately fattens about a
//! third of the interior wall runs to two cells (§10.1.5), so boring one of those
//! opens a one-cell **pocket** rather than a route — and a pocket is a *use* of this
//! tool, not a waste of it. A dead-end alcove off the room, out of the through-routes
//! a patrol sweeps, is somewhere to sit a sweep out. It conceals nothing — it is not
//! a cupboard (§10.3) — so whether it is shelter or a trap is the player's judgement,
//! which is exactly the kind of decision worth handing them. Refusing it would have
//! bought a "no" in place of a choice.
//!
//! The facility's **outer shell** is the one thing refused (§1/§4.5): the intruder
//! enters and leaves by their own tunnel and there is no other exit, so boring out
//! through the shell must never become a route. Refusing it does not make a boundary
//! wall stop *counting*, either — standing beside one boundary wall and one interior
//! wall is two adjacent walls, and is refused twice over.
//!
//! # The hole is real, and it cuts both ways
//!
//! The bored cell becomes ordinary [`Terrain::Floor`] in the **one** spatial model
//! (§10.5) — there is no second notion of "a hole" that only the player's systems
//! know about. Guards route through it (their pathing predicates are terrain, so this
//! is automatic and cannot be forgotten), their cones see through it, the danger
//! overlay redraws around the new sightlines, and it is there for the rest of the
//! level. The route you cut is a route they get.
//!
//! §10.5's other half is the region graph, and the bored cell joins the region of the
//! space it opens onto — exactly as a recessed cupboard does
//! ([`RegionGraph::add_cell`](crate::region::RegionGraph::add_cell)) — so the "every
//! walkable cell belongs to exactly one region" invariant survives a bore.
//!
//! # §10.1a and the sightline rule
//!
//! A hole punched into a corridor's long wall from the room side can create exactly
//! the uncovered straight run §10.1a forbids. That is not a bug and nothing here
//! prevents it: **§10.1a constrains the generator, not the player.** The rule exists
//! so a level is never *born* with an unsurvivable sightline; a player who cuts one
//! themselves has made a choice, and the danger overlay (§11.5) draws the new cone
//! the moment a guard's line reaches down it — so the consequence reads as their own
//! doing. §10.6 is untouched in the other direction: boring only ever *adds*
//! connectivity, so every reachability guarantee that held before a bore holds after.

use super::*;

/// Why a bore is refused right now (§8.4/#303) — the [`Err`] half of
/// [`State::bore_target`].
///
/// Each case is a *different thing to do about it*, which is why they are not one
/// "unusable": walk to a wall, step away from the corner, find an interior wall,
/// find a thinner one. The near line speaks them (§11.7) so a player learns the rule
/// by being told it once rather than by being refused repeatedly in silence.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoreRefusal {
    /// No wall touches the player at all — standing in the open, there is nothing to
    /// bore.
    NothingToBore,
    /// Two or more walls touch the player — a corridor, a corner, an alcove. The
    /// target would be ambiguous and this ability never disambiguates (§8.4).
    TooManyWalls,
    /// The one adjacent wall is the facility's **outer shell** (§1/§4.5). The tunnel
    /// is the only way in and the exit the only way out; the shell is not a wall,
    /// it is the edge of the world.
    TheOuterShell,
    /// The run does not hold Pierce Wall, or its per-level budget is spent
    /// (§8.2/#302). Not a fact about where the player stands, so it is checked last
    /// and reported only when the geometry would otherwise have allowed the bore.
    NoUsesLeft,
}

impl BoreRefusal {
    /// What the near line says about this refusal (§11.7). Short enough for the row
    /// (the bound is pinned in the hud tests) and phrased as the *rule*, not as a
    /// complaint: the player is being taught the precondition.
    pub fn message(self) -> &'static str {
        match self {
            BoreRefusal::NothingToBore => "no wall to bore",
            BoreRefusal::TooManyWalls => "too many walls to choose one",
            BoreRefusal::TheOuterShell => "that is the outer shell",
            BoreRefusal::NoUsesLeft => "the borer is spent",
        }
    }
}

impl State {
    /// The cell Pierce Wall would bore right now, or why it will not (§8.3/#303).
    ///
    /// A **pure derived function of state** (§11.4), recomputed wherever it is
    /// needed rather than plumbed: the ability bar colours its entry from it, the
    /// turn loop gates the activation on it, and the near line speaks its refusal.
    /// Because all three read this one function they cannot disagree about whether a
    /// bore is on — which is the whole reason the bar flickering between usable and
    /// unusable as the player walks is a *teaching* signal rather than a lie.
    ///
    /// The order of the checks is the order of the rules: count the walls first
    /// (§8.4 — the target must be unique), then judge the one wall found — only the
    /// shell is off limits — then the budget. The budget is last on purpose, so a
    /// player standing somewhere they could never bore is told about the geometry
    /// rather than about their supply.
    ///
    /// **What is behind the wall is not asked.** The thickness of the wall is the
    /// player's business, not the ability's: boring into a two-cell wall (§10.1.5)
    /// opens a one-cell **pocket** off the room, and that is a use of the tool rather
    /// than a waste of it — a dead-end alcove out of the through-routes to sit out a
    /// sweep in. It is not a hideout (§10.3 conceals; this does not), so it is shelter
    /// you have to judge, which is the right kind of decision to hand a player.
    pub fn bore_target(&self) -> Result<Cell, BoreRefusal> {
        let facility = self.layout.facility();
        let walls: Vec<Cell> = Direction::ALL
            .into_iter()
            .filter_map(|dir| self.player.step(dir))
            .filter(|&cell| facility.terrain(cell) == Some(Terrain::Wall))
            .collect();

        let wall = match walls.as_slice() {
            [] => return Err(BoreRefusal::NothingToBore),
            [one] => *one,
            _ => return Err(BoreRefusal::TooManyWalls),
        };

        // The shell is the edge of the world, not a wall you may open (§1/§4.5).
        if is_outer_shell(facility, wall) {
            return Err(BoreRefusal::TheOuterShell);
        }

        // The **economy** state, read from the deck rather than through
        // [`ability_state`](Self::ability_state): that surface now layers this very
        // verdict over the economy to produce the bar's contextual `Unusable`
        // (§11.4/#345), so asking it here would ask this function about itself. The
        // deck is the half this check actually wants — is there a use left? — and the
        // contextual half is precisely what is being decided.
        if !matches!(
            self.abilities.state(AbilityId::PierceWall),
            AbilityState::Ready | AbilityState::Limited { .. }
        ) {
            return Err(BoreRefusal::NoUsesLeft);
        }
        Ok(wall)
    }

    /// Turn `wall` into ordinary floor, permanently (§10.3/§10.5) — the whole world
    /// change a bore makes, applied after [`bore_target`](Self::bore_target) has
    /// approved it.
    ///
    /// The cell joins the region of the space it opens onto, which is the player's
    /// own — the same claim a recessed cupboard makes (§10.1.6), and what keeps
    /// §10.5's "every walkable cell has exactly one region" true through a bore. A
    /// fixture layout with no regions at all (the test worlds) simply has none to
    /// join, so the claim is skipped rather than forced.
    pub(super) fn bore_wall(&mut self, wall: Cell, events: &mut Vec<Event>) {
        self.layout.place(wall, Terrain::Floor);
        if let Some(region) = self.region_opening_onto(wall) {
            self.layout.claim_cell(region, wall);
        }
        events.push(Event::WallBored { at: wall });
    }

    /// Which region the newly bored cell belongs to: the player's own if they stand
    /// in one, else any cardinal neighbour's, in [`Direction::ALL`] order so the
    /// answer is deterministic (§12.4). `None` on a layout with no regions.
    fn region_opening_onto(&self, wall: Cell) -> Option<RegionId> {
        let regions = self.layout.regions();
        regions.region_at(self.player).or_else(|| {
            Direction::ALL
                .into_iter()
                .filter_map(|dir| wall.step(dir))
                .find_map(|cell| regions.region_at(cell))
        })
    }
}

/// Whether `cell` is on the facility's outer ring — the shell §4.1/§10.6 guarantee
/// encloses every level, and §1/§4.5 forbid opening (the tunnel is the only way in).
fn is_outer_shell(facility: &Facility, cell: Cell) -> bool {
    cell.x == 0 || cell.y == 0 || cell.x + 1 >= facility.width() || cell.y + 1 >= facility.height()
}
