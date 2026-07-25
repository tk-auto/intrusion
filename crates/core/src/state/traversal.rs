//! Auto lateral-shift past an obstacle (#57) — the traversal experiment.
//!
//! When a player's step is blocked by a **dead-bump obstacle** — a wall, a
//! pillar, an occupied cupboard, a table already crouched behind: the bumps that
//! today just no-op (§4.4) — and **exactly one** of the two perpendicular cells
//! is plain, safe, enterable floor, the player slides one cell that way instead
//! of catching on the corner. It smooths the "brush past a pillar while fleeing
//! and have to re-input the turn" friction (§10.1a/§7.6) without adding a diagonal
//! move (§4.1 [SETTLED]): the slide is one orthogonal step, chosen only when it is
//! *unambiguous*.
//!
//! This is an **experiment** (see the ticket): shipped behind the smallest seam —
//! `resolve_step`'s final dead-bump arm calls [`State::try_lateral_shift`] before
//! the bump no-ops — and meant to be judged in play and dropped if it fights §4.4
//! or §5. The three open questions the ticket parks are answered here, in code, so
//! the decision lives next to what it governs:
//!
//! 1. **Does the shift cost the turn? Yes.** A shift is world-changing movement, so
//!    §4.4's "every action that changes the world costs the turn" governs — a *free*
//!    slide would be free movement, a far worse hole in the economy than the typo
//!    §4.4 protects. The typo cost is contained instead by the *unambiguous* gate:
//!    a genuine mis-aim in a corridor (wall ahead, both sides open) is **ambiguous**
//!    and still free-bumps; only a bump with exactly one open side — where the open
//!    side is almost always the way you meant to go — spends the turn and moves.
//! 2. **Does the shift change facing? Yes — to the shift direction.** §5 is
//!    [SETTLED]: "facing is the direction of the last successful step." The slide is
//!    a successful step sideways, so facing follows it, and the sight phase recasts
//!    the ~180° cone from the new pose — the danger overlay stays truthful because
//!    the player never lands anywhere a guard's cone already covers (the fairness
//!    rule below). Preserving the *forward* facing was the tempting alternative, but
//!    it would make a successful move that does not set facing — a §5 violation — so
//!    it is left as a thing to try only if the playtest dislikes the swing.
//! 3. **What is "the obstacle" and what is a "free lateral"?** The obstacle is any
//!    **dead bump** — a [`BumpKind::Solid`] (wall, pillar, duct wall), an occupied/
//!    locked cupboard ([`BumpKind::HideoutBlocked`]), or a held crouch-table
//!    ([`BumpKind::CrouchHeld`]): exactly the bumps that otherwise no-op. Every
//!    *interactive* bump keeps its head-on meaning and never triggers a slide — a
//!    door is bumped open, a table is bumped to crouch, a cupboard is climbed into,
//!    a guard is taken down (§4.3). The lateral must be **plain enterable floor**: a
//!    [`BumpKind::Move`] cell holding no loose body and no decoy, so the slide never
//!    grabs, stomps, or triggers any interaction the player did not aim (the §8.4
//!    no-auto-target spirit, mirroring Run's straight-ahead extra step).
//!
//! **Fairness is the hard constraint (§2.2/§4.5).** Capture is contact, and
//! permadeath demands every capture trace to a decision the player made — so an
//! auto-move must never drop the player where a guard captures them, nor silently
//! into a guard's cone. [`State::shift_is_safe`] refuses the slide unless the
//! destination is clear of *both*: no guard's cone covers it (you never land seen),
//! and no guard stands orthogonally adjacent to it (nothing can step in and capture
//! next guard phase, §7.1's one-cell orthogonal move). When in doubt the slide is
//! refused and the bump falls back to the free §4.4 no-op it has always been —
//! losing nothing, only declining to help. (One noted wrinkle for the playtest: a
//! slide refused by an *unseen* guard's cone is faintly informative, since it would
//! otherwise have fired; the safety of never auto-walking into detection is judged
//! to outweigh it.)

use super::{BumpKind, Event, State};
use crate::cell::Direction;

impl State {
    /// Try to slide one cell past a dead-bump obstacle (#57), the player having
    /// aimed `dir` into it. Returns whether a slide happened — `true` spends the
    /// turn as movement (the caller reports it spent), `false` leaves the bump to
    /// no-op free. Called from `resolve_step`'s dead-bump arm only, so `dir` is
    /// always a bump that would otherwise change nothing.
    pub(super) fn try_lateral_shift(&mut self, dir: Direction, events: &mut Vec<Event>) -> bool {
        // Never while dragging — hands are full and movement is capped at the drag's
        // half speed (§8.3); an auto-slide must not haul a body sideways. Never
        // inside a duct — the crawlspace confinement owns movement there (§10.7), and
        // a lateral out of a mouth would be an unaimed climb-out. (Phasing never
        // reaches here: while phased there is no bump, every cell is a plain Move.)
        if self.dragging.is_some() || self.occupied_duct().is_some() {
            return false;
        }

        // Exactly one lateral must qualify — the *unambiguous* gate. Two open sides
        // (a real mis-aim) or none both leave the bump free, unchanged (§4.4).
        let mut only = None;
        for lateral in dir.perpendicular() {
            if self.lateral_shift_target(lateral) {
                if only.is_some() {
                    return false; // both sides qualify — ambiguous, no slide
                }
                only = Some(lateral);
            }
        }
        let Some(lateral) = only else {
            return false; // neither side qualifies
        };
        let target = self
            .player
            .step(lateral)
            .expect("a qualifying lateral is an in-bounds move cell");

        // The slide: one orthogonal cell, facing follows the successful step (§5),
        // the turn spent (§4.4). Emitted as a plain [`Event::Moved`] — it *is* a
        // move, so the near line, the crouch-walk check, and `moved_this_turn` all
        // read it exactly as a stepped move, no special case. No Run extra step (a
        // slide is a recovery, not a committed sprint) and no haul/stomp (dragging is
        // refused above, and the destination holds no body or decoy).
        self.player = target;
        self.facing = lateral;
        events.push(Event::Moved { to: target });
        true
    }

    /// Whether the cell one step `lateral` from the player is a valid slide
    /// destination: plain enterable floor (a [`BumpKind::Move`]) that holds no loose
    /// body and no decoy — so the slide triggers no interaction the player did not
    /// aim — and is fairness-safe ([`shift_is_safe`](Self::shift_is_safe)).
    fn lateral_shift_target(&self, lateral: Direction) -> bool {
        let Some(cell) = self.player.step(lateral) else {
            return false; // off the north/west edge — no cell there
        };
        matches!(self.bump_kind(cell), BumpKind::Move)
            && self.body_at(cell).is_none()
            && self.decoy != Some(cell)
            && self.shift_is_safe(cell)
    }

    /// The fairness gate (§2.2/§4.5): a cell is safe to be auto-slid into only if no
    /// guard's cone covers it — the player never lands seen — and no guard stands
    /// orthogonally adjacent to it — nothing can step in and capture next guard
    /// phase (§7.1: guards move one orthogonal cell a turn). A downed guard is a
    /// [`Body`](crate::body::Body), not in `guards`, and never moves or captures, so
    /// scanning the live guards alone is the whole threat set.
    fn shift_is_safe(&self, cell: crate::cell::Cell) -> bool {
        !self
            .guards
            .iter()
            .any(|g| g.fov().contains(cell) || g.pos().manhattan_distance(cell) == 1)
    }
}
