//! Cover (§8.3/§10.3/#562): put a partial-cover table down, push it ahead of you, and
//! hand the floor back when the window ends.
//!
//! # It is a table, and that is the whole of what it is
//!
//! The deploy writes [`Terrain::PartialCover`] into the faced cell — the *same* terrain
//! kind the §10.1a sightline pass stamps, in the one spatial model (§10.5). There is no
//! second notion of "a piece of cover" that only the player's systems know about: the
//! glyph is `π`, guards route around it, sight passes over it, a drone flies over it, and
//! the crouch's geometry ([`crouch`](crate::crouch)) gathers it into whatever run it
//! touches. Cover placed against a bench *is* part of that bench, arms and all, because
//! the flood fill has no way to tell the two apart — which is the point rather than a
//! coincidence, and `deployed_cover_is_a_table_in_every_model` pins it.
//!
//! What is stored on the state is one cell ([`State::deployed_cover`]) and nothing else.
//! It is not a second model of the world; it is the *bookmark* that says which table is
//! the one this ability owns — so the push knows what it may shove, and the release knows
//! which cell to hand back.
//!
//! # The push is the ability
//!
//! Bumping a generated table crouches you behind it (§10.3). Bumping **this** table
//! shoves it one cell directly away, steps you into the cell it vacated, and leaves you
//! crouched — one turn, one verb ([`State::push_cover`]). Repeat and the cover walks
//! across the room ahead of you at full speed, concealing you from everything on its far
//! side the whole way, which is the crossing the ability is bought for.
//!
//! **Crouch always; push when it can.** A shove with nowhere to go — a wall, other
//! furniture, a guard, the board's edge — is not a refusal: it falls back to the plain
//! §10.3 crouch from where you stand. So the bump has one meaning the player can rely on
//! (*you end up behind it*) and one variable consequence (*and maybe a cell further on*),
//! rather than a press that sometimes does nothing at all.
//!
//! # Why there is no severance check, and why that is safe
//!
//! §10.6's guarantee is about **generation**: a facility must never be *born* unsolvable.
//! Nothing here is generation. A solid the player placed, that expires on its own clock,
//! that can be pushed, and that can be dismissed for free at any time cannot produce an
//! unsolvable facility — so plugging a corridor and making a patrol take the long way
//! round is a *tactic*, and it is precisely the one Lockdown already sells with a door
//! (§8.3, and the design accepted eight turns of it). Restoring a severance check here
//! would delete the tactic and leave the ability a crouch with an activation bolted on.
//!
//! The two structural guarantees that make that true are the same pair Repel rests on,
//! and neither is a matter of care:
//!
//! - **The window is the ability's own duration.** The cover carries no clock — it stands
//!   for exactly as long as the §8.2 slot runs — so there is no second timer to outlive
//!   the first.
//! - **The release is total.** [`release_cover`](State::release_cover) hands back one
//!   cell, from the one teardown list every window end walks
//!   ([`unwind_effect`](State::unwind_effect)), so *"nothing is left behind"* is a
//!   property of the shape and not of a sweep that could miss something.
//!
//! And the owner is never boxed in: the toggle-off is free (§4.4), and a bump pushes
//! rather than refusing. That is Lockdown's *"you are never refused"* answered a different
//! way to the same end (§2.2).
//!
//! # Nothing is ever inside it
//!
//! The deploy refuses any cell that is not plain, empty floor, and so does the push, so a
//! piece of cover is never written over an actor, a body or a decoy — there is nothing to
//! eject when it goes, and the Dephase safety-eject problem is not reinvented here. A
//! *phased* player walks through it exactly as they walk through any solid (§8.3), and if
//! the window ends around them the cell simply becomes floor again, which is the outcome
//! the eject exists to reach anyway.
//!
//! # What the region graph is not told, and why
//!
//! The deploy does **not** move the cell out of its §10.5 region, and the release does not
//! put it back. Generation drops a stamped table from its region because the building has
//! changed shape; a table somebody puts down for twelve turns has not changed the
//! building. The machinery for exactly this already exists and predates the ability: a
//! guard's patrol sweep draws its candidates through its own `walkable_ground` flood
//! precisely because *"the region graph does not know about the
//! solid usables stamped into the building afterwards"* (#477), so a cell that
//! has stopped being routable is filtered out of a beat wherever it came from.
//!
//! That is also what keeps the §7.5 partition **uncut**: beats are recut when the guard
//! set changes (§7.3/#374) and at no other time, and neither deploying nor releasing a
//! table touches the guard set.
//!
//! # Guards do not notice it
//!
//! Nothing here touches the alert ladder, the radio or the sense. Guards detect on vision
//! (§9) and nothing in §7 notices that a room changed shape, so a piece of cover appearing
//! in the middle of a corridor is routed around without comment and its disappearance is
//! not evidence either. That is the honest §2.3 call rather than a gap: *"guards notice new
//! furniture"* is a new system and its own ticket, and half-building it here would be the
//! facade the design warns about.

use super::*;

/// What a bump into the deployed cover would leave behind (§8.3/#562) — where the cover
/// ends up and where the player ends up, resolved **before** anything moves.
///
/// It exists so the shove has exactly one description, read by the three surfaces that
/// need it: the bump ladder classifies with it, the executor performs it, and the §13.2
/// bot plans against it ([`State::cover_push`]). A push predicted one way and performed
/// another would be the usable line's own failure mode (§11.4) moved into the geometry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CoverPush {
    /// The cell the cover is shoved into — one step directly away from the player, and
    /// the crouch's new anchor.
    pub cover: Cell,
    /// The cell the player advances into: the one the cover just left.
    pub player: Cell,
}

impl State {
    /// The cell the run's deployed **Cover** is standing in (§8.3/#562), or `None` when no
    /// window is running.
    ///
    /// One piece at a time, and the economy already guarantees it: the ability is
    /// activated (§8.2) with a duration well inside its lockout, so a second deploy cannot
    /// overlap the first.
    ///
    /// It is a **bookmark, not a model** — the cover itself is ordinary
    /// [`Terrain::PartialCover`] in the one spatial model (§10.5), and every §10.3
    /// consumer reads it from the grid like any other table. What this answers is only
    /// *which* table the ability owns, which is the one question the grid cannot.
    pub fn deployed_cover(&self) -> Option<Cell> {
        self.cover
    }

    /// Whether `cell` can take a piece of cover (§8.3/#562) — **plain, empty floor**, and
    /// the one predicate both the deploy and the push read so *"where may it go"* has a
    /// single answer.
    ///
    /// Plain floor is [`Terrain::Floor`] and nothing else, which refuses the whole of the
    /// ticket's list in one clause rather than in eight: a wall, a doorway (a panel open
    /// or shut is its own terrain, §10.4), a hideout, a duct mouth, a console, a crate,
    /// the comms terminal and the exit are each a different kind. Empty is *nothing
    /// standing in it* — no actor (§4.3), no body, no decoy — which is what makes
    /// "nothing can ever be inside a piece of cover" true at the moment it is written
    /// rather than something the release has to cope with.
    ///
    /// Pure, so the ability bar may ask it every frame (§11.4/#345) and get the answer the
    /// press itself would.
    pub fn cover_ground(&self, cell: Cell) -> bool {
        self.layout.facility().terrain(cell) == Some(Terrain::Floor)
            && !self.occupied(cell)
            && self.body_at(cell).is_none()
            && self.decoy != Some(cell)
    }

    /// The cell a deploy would put the cover in (§8.3/#562): the **cell the player faces**,
    /// if it can take one — §8.4's second way to aim, the Decoy's, and no target list
    /// anywhere (appendix 1).
    ///
    /// `None` is the free refusal (§4.4): the press changes nothing, costs nothing, and
    /// greys the bar entry (§11.4). It says so in **silence**, on the Decoy's own grounds —
    /// the faced cell is drawn on the board, so *why* there is nowhere to put a table is
    /// already on screen and a near line would repeat it (§11.7).
    pub fn cover_deploy_cell(&self) -> Option<Cell> {
        // **Not from inside a crawlspace** (§10.7), on the drone's *needs you on your
        // feet* precedent: a player folded into a duct is not in a position to put
        // furniture down in the room outside, and the one cell it would otherwise reach
        // — the mouth an entry faces — is the cell they climb out through. Most duct
        // cells refuse themselves anyway (they are walled in), which is exactly why the
        // rule is stated rather than left to the geometry to enforce by accident.
        if self.in_duct() {
            return None;
        }
        let ahead = self.player.step(self.facing)?;
        self.cover_ground(ahead).then_some(ahead)
    }

    /// What a bump one step `dir` would do to the deployed cover (§8.3/#562), or `None`
    /// when that bump is not a push at all — nothing deployed, a different table, or a
    /// shove with nowhere to go.
    ///
    /// A `None` is never a refusal. It is the fall-back to the plain §10.3 crouch, which
    /// is what *"crouch always; push when it can"* means at the seam: the bump keeps one
    /// meaning, and only the extra cell is conditional.
    ///
    /// Public because the §13.2 bot plans its duck against the same answer: on a piece of
    /// cover a bump moves *both* the player and the furniture, so a bot predicting
    /// concealment for a stationary crouch would be planning against geometry the press
    /// does not produce (§13.4).
    pub fn cover_push(&self, dir: Direction) -> Option<CoverPush> {
        let cover = self.cover?;
        if self.player.step(dir) != Some(cover) {
            return None;
        }
        let shoved = cover.step(dir)?;
        self.cover_ground(shoved).then_some(CoverPush {
            cover: shoved,
            player: cover,
        })
    }

    /// Put a piece of cover in `cell` (§8.3/#562) — the whole world change the activation
    /// makes, applied once the deck has switched the ability on.
    ///
    /// It writes terrain and nothing else. In particular it does **not** duck the player
    /// behind what it just put down: the deploy is a turn spent standing in the open with
    /// a table in front of you, and getting behind it is the bump on the turn after. That
    /// exposed turn is the ability's entry price (§2.3) and the reason twelve turns of
    /// window is worth its lockout.
    ///
    /// **It raises no event of its own** (§11.7). A `π` appearing in the cell the player
    /// is facing is the report, and the activation already speaks one line
    /// ([`Event::AbilityActivated`]); a second saying *a table is there* would spend the
    /// near line's one row restating the board.
    pub(super) fn deploy_cover(&mut self, cell: Cell) {
        self.layout.place(cell, Terrain::PartialCover);
        self.cover = Some(cell);
    }

    /// Perform the shove (§8.3/#562): the cover moves to `push.cover`, the player steps
    /// into `push.player`, and the crouch is anchored on the cover's new cell.
    ///
    /// The terrain moves **before** the player walks, which is not incidental: the walk is
    /// the ordinary [`walk_into`](Self::walk_into), so a dragged body is hauled, facing
    /// follows the step (§5), a decoy underfoot is stomped and a Run sprint gets its
    /// second cell offered exactly as it would on open floor. Offered and declined — the
    /// cell ahead is the table the player has just pushed there, and a sprint cannot
    /// double-shove.
    ///
    /// The anchor is reset rather than kept, which is what stops the crouch-walk rule
    /// standing the player up on their own push: the pose changed, deliberately, to the
    /// cover's new cell.
    pub(super) fn push_cover(&mut self, push: CoverPush, dir: Direction, events: &mut Vec<Event>) {
        self.layout.place(push.player, Terrain::Floor);
        self.layout.place(push.cover, Terrain::PartialCover);
        self.cover = Some(push.cover);
        self.walk_into(dir, push.player, events);
        self.crouched_behind = Some(push.cover);
        // The **crouch's own report**, not a shove's (§11.7): what the player needs told
        // is the pose, and *"you duck behind the table"* is exactly as true here as after
        // a plain §10.3 bump. That the table also moved a cell is on the board, and the
        // usable line said it would before the press.
        events.push(Event::Crouched { behind: push.cover });
    }

    /// Take the cover away (§8.3/#562) — the end of the window, however it ended: the
    /// duration running out (§8.2) or the free toggle-off (§4.4). Idempotent, and safe to
    /// call when nothing is out.
    ///
    /// One cell, handed back whole, so *"nothing is left behind in any model"* is a
    /// property of the shape. The cell goes back to plain [`Terrain::Floor`] because plain
    /// floor is the only thing [`cover_ground`](Self::cover_ground) ever let it be written
    /// over, so there is no prior terrain to remember.
    ///
    /// **The pose goes with it.** A player crouched behind the piece is simply standing:
    /// the anchor named a table that no longer exists, and a crouch anchored on nothing
    /// conceals nothing anyway ([`cover_run`](crate::crouch::cover_run) yields an empty run) — so
    /// clearing it is the difference between *standing up* and *believing yourself hidden*.
    /// A pose anchored on a bench the cover merely joined is untouched: that furniture is
    /// still there, and it is still the run it always was.
    /// It raises no event either, for the deploy's reason: the window ending already
    /// speaks ([`Event::AbilityExpired`], or the toggle-off's own line), and the table
    /// vanishing from the cell the player is standing next to is the rest of the report.
    pub(super) fn release_cover(&mut self) {
        let Some(cell) = self.cover.take() else {
            return;
        };
        self.layout.place(cell, Terrain::Floor);
        if self.crouched_behind == Some(cell) {
            self.crouched_behind = None;
        }
    }
}
