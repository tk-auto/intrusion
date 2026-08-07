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
//! # What the guard does about it, and the one rule that resolves the rest
//!
//! **The field refuses a crossing of its boundary inward, and nothing else.** Say it that
//! way and every case the ticket asks for falls out of the one sentence rather than being
//! a list of exceptions:
//!
//! - A guard **outside** may not step in — Calm, investigating, searching or chasing, since
//!   the rule never asks what state it is in ([`repels`](State::repels)).
//! - A guard **inside** when the disc lands is not moved and not held: it may step within
//!   the field and out of it, because neither of those is a crossing inward.
//! - A guard that has **left** cannot come back, with nothing remembered — it is outside
//!   now, and the first clause is all it takes.
//!
//! Two halves, as with the seal. Its **route** treats the field as solid, so it plans the
//! long way round instead of walking into ground that will refuse it
//! ([`repel_route_blocks`](State::repel_route_blocks)); and should it arrive at the edge
//! anyway — a destination inside the disc, a field stamped under its feet — its step is
//! simply declined and it holds that turn.
//!
//! **A guard with no route at all holds where it stands**, which is the ticket's open
//! question answered the way Lockdown already answers it: a sealed door can cut a corridor
//! for eight turns and the design accepted that. The alternative — walk up and wait *at*
//! the boundary, facing in — reads better as a cordon, and it is the recorded fallback if
//! the hold plays as a stall (appendix 60); it is not what shipped, because a guard that
//! failed to route would have to be asked for a second route in the same turn, and
//! [`Guard::decide`](crate::Guard::decide) spends dwell, memory and facing on the asking.
//! What makes the hold safe either way is the clock: the window ends, and everything the
//! guard was going to do it does then.
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
    /// and the guard is not already in it. Every case the design asks for is this sentence
    /// read in a different direction (see the module header) — including the two that would
    /// otherwise need memory: a guard inside may walk about and out, and a guard that has
    /// left cannot return, because by then it is one of the guards this refuses.
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
    /// about the ground: a guard already inside is bound by nothing, so it is handed no
    /// blocks at all and routes through its own neighbourhood normally. That is the one
    /// place this differs from a lock, and it differs because a locked door is a fact about
    /// the door while this is a fact about the boundary.
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
