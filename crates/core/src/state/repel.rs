//! Repel (§7.6/§8.3, #554): stamp a disc no guard will walk into, then hand every cell
//! of it back.
//!
//! It is [`lockdown`](super::lockdown)'s ability with the brush widened from a doorway to
//! open ground, and it is built out of the same two words — a **snapshot** and a
//! **release**. What the two abilities do *not* share is where they work, and that is the
//! whole reason both exist (§2.3): a lockdown is refused where there is no door and is the
//! tool for a built-up wing, and this one works precisely where a lockdown is inert —
//! a hub room, a stretch of open floor, a corridor with nothing to shut.
//!
//! # A wall, and why it can only ever be a temporary one
//!
//! What the field buys is what a seal buys: a **detour**. §7.6/§10.4's *"a guard cannot get
//! it open, so its route goes the long way round"* is handed to the router here as plain
//! blocked ground, so a pursuit loses the turns the long way costs. It is not a hiding
//! place and not invincibility — the wall is made of time, and §2.2/§7.2 forbid the class
//! of ability that could sever pathing for good (#170/#182). Two structural guarantees,
//! neither of them a matter of care:
//!
//! - **The window is the ability's own duration.** The field carries no clock of its own —
//!   it is one [`EffectArea`] held in [`State::repel`] for exactly as long as the §8.2 slot
//!   is running — so there is no second timer to outlive the first.
//! - **The release is total.** [`release_repel`](State::release_repel) drops the whole
//!   field in one move, from the one teardown list every window end walks
//!   ([`unwind_effect`](State::unwind_effect)), so there is no per-cell bookkeeping to fall
//!   out of step with and no cell that can be missed.
//!
//! # It is your field, so it does not refuse you
//!
//! Nothing in the player's own movement consults it — the field is handed to the *guards'*
//! router as blocked cells and is never stamped into terrain — so the player walks their
//! own disc exactly as they walked the floor a moment before (§4.4). That is the same
//! asymmetry a lockdown is made of, and it settles the same way: an ability that could box
//! its own owner in is one that ends runs by geometry rather than by decision (§2.2).
//!
//! # What the guard does about it: ground it will not stand in
//!
//! One sentence, and everything else here is that sentence meeting a particular
//! geometry: **no guard stands in the field.** Two halves follow from it, and the second
//! is what makes the first read as a wall rather than as a glitch.
//!
//! **Nobody gets in.** The boundary refuses a crossing *inward*
//! ([`repels`](State::repels)) — Calm, investigating, searching or chasing alike, since
//! the rule never asks what mood a guard is in. A route plans around the disc
//! ([`repel_route_blocks`](State::repel_route_blocks)) so a pursuit takes the long way
//! (§7.6/§10.4) and loses the turns that costs; and a guard the field has cut off
//! **entirely** closes on the boundary and waits there, facing in
//! ([`repel_approach_step`](State::repel_approach_step)) — a cordon, not a stall. That is
//! the ticket's open question settled the *second* way it offered (appendix 62): the two
//! answers are the same hold mechanically, and only one of them looks like the hunt is
//! still happening.
//!
//! **Anybody inside walks out**, by the shortest way
//! ([`repel_exit_step`](State::repel_exit_step)), the turn after the disc lands around
//! them and every turn until they are clear. Nothing is moved by the stamp itself — a
//! guard spends its own turn leaving, keeps its mood and its lead, and picks its errand
//! back up outside — and once out it is one of the guards the boundary refuses, so it
//! cannot come back. That last part needs nothing remembered.
//!
//! The pair is what the ability is *called*: the field does not hide you and does not
//! freeze anybody, it makes a patch of floor something guards will not occupy. What
//! gathers around it is the cost (below).
//!
//! **It never deadlocks, and the guarantee is the clock rather than care**: the window
//! ends, the field is released whole, and everything a cordoned guard was going to do it
//! does then — one turn later, from a cell much closer to the player than the one it was
//! standing in when the wall went up.
//!
//! # It conceals nothing
//!
//! Nothing in this module touches sight, the sense, the alert ladder or the radio. A guard
//! with a line into the field sees the player standing in it, steps §7.3's ladder on the
//! sighting and calls it in (§7.7) exactly as it would over open floor — and what gathers
//! outside a wall nobody may cross is a ring of guards waiting for the window to close.
//! That is the ability's price rather than a gap in it (§2.3).

use super::effects::area_radius;
use super::*;
use crate::guard::routable;
use crate::path;

impl State {
    /// The field **a Repel fired from where the player stands would stamp** (§8.3/#554):
    /// the §6.1 box of [`REPEL_RADIUS`], read from the one [`area_radius`] table so the
    /// ability's reach and the table cannot drift apart.
    ///
    /// The firing seam, in [`lockdown_area`](Self::lockdown_area)'s and
    /// [`confusion_blast`](Self::confusion_blast)'s shape and for the same reason: one
    /// object carries the geometry to the rule the guards are held by, to the event, and
    /// to the mark the player reads — so what is painted is what is enforced, never a
    /// redrawing of it.
    ///
    /// **Unclamped**, which is [`lockdown_area`](Self::lockdown_area)'s answer rather than
    /// the blast's. Confusion narrows to what the player can *perceive* because a guard
    /// frozen out of sense range is an effect with no readout; a patch of ground is a fact
    /// about the building, and ground a guard will not walk into is still ground a guard
    /// will not walk into when nobody is looking at it. At [`REPEL_RADIUS`] the point is
    /// moot in every case the game can produce — the whole disc fits inside even a duct's
    /// shortened sense (pinned beside the constant) — and stating the rule this way is
    /// what keeps a later widening from silently acquiring a clamp it was never given.
    ///
    /// Pure, so the ability bar may ask it every frame (§11.4/#345) and the answer is the
    /// one the press itself would use.
    pub fn repel_area(&self) -> EffectArea {
        EffectArea {
            centre: self.player,
            radius: area_radius(Effect::Repel).expect("Repel is an area effect"),
        }
    }

    /// The field a Repel is **currently holding** (§8.3/#554), or `None` when none is
    /// running — the live half of [`repel_area`](Self::repel_area)'s derived one.
    ///
    /// This is the snapshot: the area is stored as it was measured at the firing and never
    /// re-derived from the player, so walking away narrows nothing, widens nothing and
    /// moves nothing. §4.5's capture-is-contact is **[SETTLED]**, and a disc that followed
    /// its owner is a disc no guard could ever reach him in — the one shape this ability
    /// must not have.
    pub fn repel_field(&self) -> Option<EffectArea> {
        self.repel
    }

    /// Whether `cell` is inside the live Repel field (§8.3/#554) — the one question the
    /// rule, the router and the mark all ask, so none of them can answer it differently.
    /// `false` on every turn no field is up, which is nearly all of them.
    pub fn repelled(&self, cell: Cell) -> bool {
        self.repel.is_some_and(|field| field.contains(cell))
    }

    /// Whether the field refuses a guard standing at `from` the step onto `to`
    /// (§7.6/§8.3/#554) — **the whole rule**, in one predicate both the router and the
    /// movement pass read.
    ///
    /// A crossing of the boundary **inward**, and nothing else: the target is in the field
    /// and the guard is not already in it. It is the "nobody gets in" half of the rule; the
    /// other half is [`repel_exit_step`](Self::repel_exit_step), and between them a guard
    /// that has left cannot return with nothing remembered, because by then it is simply
    /// one of the guards this refuses.
    ///
    /// It never asks what state the guard is in. That is deliberate rather than an
    /// omission: a rule that enumerated moods is a rule a later mood can be quietly left
    /// out of, and `no_guard_enters_the_field_in_any_state` pins the four the game has.
    pub(super) fn repels(&self, from: Cell, to: Cell) -> bool {
        self.repelled(to) && !self.repelled(from)
    }

    /// The cells a **guard's route** must treat as solid because the field holds them
    /// (§7.6/§8.3/#554), for the guard standing at `from`.
    ///
    /// This is what turns the field into a wall rather than a wait, and it is
    /// [`sealed_route_blocks`](Self::sealed_route_blocks)'s twin: without it a guard would
    /// keep planning straight through the disc and then have its step declined at the edge,
    /// every turn until the window closed — technically not a deadlock, but a guard walking
    /// into an invisible wall over and over is not *"routing the long way round"*, which is
    /// the whole thing the ability buys.
    ///
    /// **Per guard, unlike the seal's**, because the rule is about a crossing rather than
    /// about the ground: a guard already inside is handed no blocks at all — it is not
    /// routing anywhere, it is [leaving](Self::repel_exit_step). That is the one place this
    /// differs from a lock, and it differs because a locked door is a fact about the door
    /// while this is a fact about the boundary.
    ///
    /// Handed to the router as blocked cells rather than stamped into terrain, for the
    /// seal's reason exactly: the field is live state on the ability (§11.3) and the
    /// *player's* routing is deliberately unchanged by it. Empty on every turn no field is
    /// running, and empty for a guard standing in one.
    pub(super) fn repel_route_blocks(&self, from: Cell) -> Vec<Cell> {
        match self.repel {
            Some(field) if !field.contains(from) => field.cells(self.layout.facility()),
            _ => Vec::new(),
        }
    }

    /// The step a guard **standing inside the field** takes to get out of it
    /// (§7.6/§8.3/#554): the first step of the shortest walk to the nearest cell that is
    /// not in the disc, or `None` when the guard is not inside one — or is, and is walled
    /// in with no way out at all.
    ///
    /// **A guard caught inside leaves, and leaving is all it does that turn.** The field
    /// is ground guards will not stand in, so the rule has two halves and this is the
    /// second: the boundary refuses everyone outside ([`repels`](Self::repels)), and
    /// anybody the disc lands *around* walks out by the shortest way rather than carrying
    /// on with its errand in the middle of it. It is a step, never a shove: nothing is
    /// teleported, the guard spends its own turn, and it keeps its mood, its lead and its
    /// destination — which is waiting for it the moment it is out.
    ///
    /// **It routes over ordinary ground and takes the ordinary consequences.** Other
    /// guards are solid to it (§7.8); the *player* is not, so a guard whose shortest way
    /// out runs over the cell the player is standing on takes that step and captures them
    /// (§4.5 **[SETTLED]** — contact is contact, and a wall the guard is already inside
    /// stops it from nothing). The way out points away from the disc's middle, which is
    /// where the player usually is, so this is rare rather than routine — but it is the
    /// rule, and it is why firing this with a guard on top of you is not a way to be rid
    /// of it.
    ///
    /// The blocked set is passed in rather than read here because the movement pass
    /// already has it: it is the same colleagues-and-seals set the guard's own routing
    /// uses that turn, so a guard leaves *around* its colleagues rather than through them.
    pub(super) fn repel_exit_step(&self, from: Cell, blocked: &[Cell]) -> Option<Direction> {
        let field = self.repel?;
        if !field.contains(from) {
            return None;
        }
        let facility = self.layout.facility();
        path::first_step_to_nearest(
            from,
            |cell| routable(facility, cell) && !blocked.contains(&cell),
            |cell| !field.contains(cell),
        )
    }

    /// The step a guard whose route the field has **cut off entirely** takes toward it
    /// (§7.6/§8.3/#554): the first step of the shortest walk to its destination *as if the
    /// field were not there*, which the boundary rule then stops at the edge.
    ///
    /// # Why a guard walks up to a wall it cannot cross
    ///
    /// This is the ticket's open question, and it is settled the second way it offered
    /// (appendix 62): **a guard with no route waits at the boundary, facing in**, rather
    /// than holding wherever it happened to be standing. Both are the same hold —
    /// mechanically nothing crosses the line either way — but a hunt that stops dead in a
    /// corridor two rooms away reads as the game giving up on you, where a hunt that walks
    /// up to the edge of the wall and stands there reads as what it is: a cordon, waiting
    /// for the window to close, in exactly the cells you have to come out through.
    ///
    /// It is asked **only when the ordinary route has already failed**, so a guard that
    /// can go the long way round still does — that detour is what the ability buys
    /// (§7.6/§10.4) and nothing here is allowed to shortcut it. What is left when the
    /// strict route fails is a destination the field is standing in front of, and the
    /// honest thing to do about it is to close.
    ///
    /// The guard walks the loose route one step per turn and is refused at the edge by the
    /// boundary rule itself, so there is no separate stopping condition to keep in step
    /// with it: the cordon forms because the route runs out of legal steps, not because
    /// anything counted the distance. Its facing follows its walk, so a guard that arrives
    /// at the edge is looking into the field — which is where the player is.
    pub(super) fn repel_approach_step(
        &self,
        from: Cell,
        to: Cell,
        blocked: &[Cell],
    ) -> Option<Direction> {
        self.repel?;
        let facility = self.layout.facility();
        path::first_step_toward(from, to, |cell| {
            routable(facility, cell) && !blocked.contains(&cell)
        })
    }

    /// What the field does to guard `index`'s step this turn (§7.6/§8.3/#554) — the one
    /// seam the movement pass calls, so the two overrides are read in one place and in a
    /// fixed order, and `chose` passes straight through on every turn no field is up.
    ///
    /// Two cases, and neither is a plan: both leave the guard's mood, lead and destination
    /// exactly as [`decide`](crate::Guard::decide) left them, and both are spent on the
    /// guard's own turn.
    ///
    /// 1. **Inside the disc** → it leaves, by the shortest way out
    ///    ([`repel_exit_step`](Self::repel_exit_step)), whatever it had been about to do.
    ///    Its errand is still there when it is out.
    /// 2. **Outside, and cut off** → it closes to the boundary
    ///    ([`repel_approach_step`](Self::repel_approach_step)) instead of standing wherever
    ///    the failed route left it. Asked only when the guard found no step at all *and*
    ///    has somewhere to be, so a guard that is dwelling (§7.5), rotating in place, or
    ///    simply arrived is never marched anywhere by this.
    ///
    /// The step it hands back is walked by the pass without going through
    /// [`commit_step`](crate::Guard), so neither override pays §7.5's turn-in-place cost.
    /// That is deliberate in both cases and for the same reason: each fires only where the
    /// guard would otherwise spend the turn achieving nothing, and a rotation tax on
    /// getting out of the way — or on closing the last cell of a cordon — would read as
    /// the guard being slow rather than as the wall being firm.
    pub(super) fn repel_step(
        &self,
        index: usize,
        chose: Option<Direction>,
        blocked: &[Cell],
    ) -> Option<Direction> {
        if self.repel.is_none() {
            return chose;
        }
        let guard = &self.guards[index];
        if self.repelled(guard.pos()) {
            // The way out, or — walled in with nowhere to go — whatever it had planned.
            return self.repel_exit_step(guard.pos(), blocked).or(chose);
        }
        if chose.is_some() {
            return chose;
        }
        let destination = guard.destination()?;
        self.repel_approach_step(guard.pos(), destination, blocked)
    }

    /// Stamp the field `area` (§8.3/#554) — the whole world change the activation makes,
    /// applied once the deck has actually switched the ability on.
    ///
    /// **Nothing is moved and nothing is caught.** A guard standing where the disc lands is
    /// left exactly where it is, with its state, lead and destination untouched: the field
    /// is ground, and ground does not push. That is also why there is no count to report —
    /// see [`Event::RepelFired`].
    pub(super) fn fire_repel(&mut self, area: EffectArea, events: &mut Vec<Event>) {
        self.repel = Some(area);
        events.push(Event::RepelFired { field: area });
    }

    /// Release the field (§8.3/#554) — the end of the window, however it ended: the
    /// duration running out (§8.2) or the free toggle-off (§4.4). Idempotent, and safe to
    /// call when nothing is up.
    ///
    /// One field, dropped whole, so *"no cell keeps the flag"* is a property of the shape
    /// rather than of a loop that has to visit them all. The wall was time, and the time is
    /// up: the very next guard phase routes as though it had never been there.
    pub(super) fn release_repel(&mut self) {
        self.repel = None;
    }

    /// Every cell of the live field (§11.5/#554) — the persistent **mark** the renderer
    /// paints in [`Category::Effect`](crate::Category), and the ground the rule above is
    /// enforced over, read from the one [`EffectArea`] so the picture and the wall are one
    /// object.
    ///
    /// Drawn through the fog because it is the player's own gadget: how far your own
    /// device reached is not something the building can keep from you (§11.5a, the same
    /// reasoning that lets a blast's footprint and a lockdown's box reach over ground you
    /// have never seen). It leaks no geometry with it — a background wash says *this is
    /// where my field is*, and says nothing whatever about what the cells under it hold.
    pub fn repel_field_cells(&self) -> Vec<Cell> {
        match self.repel {
            Some(field) => field.cells(self.layout.facility()),
            None => Vec::new(),
        }
    }
}
