//! The **effect layer**: how an ability's effect is shown on the board (§8.3/§11.5,
//! #308/#324/#338/#340).
//!
//! # One vocabulary
//!
//! **An ability effect always colourises the background.** The glyph keeps its own
//! meaning — a guard's §11.2 threat ladder, `Owned` for a thing of yours — and the
//! effect is the wash underneath it. That is the standing rule this module owns, so
//! that every effect the game grows has one place to go instead of inventing a channel
//! apiece.
//!
//! What varies is not the channel but two things:
//!
//! - **Where the mark lands** ([`MarkPlace`]) — a fixed **cell set**, decided when the
//!   mark is lit and never re-derived (a blast's footprint, a bored cell, the pair a
//!   safety eject threw you between), or the **thing** in a cell, which carries the mark
//!   wherever it goes and for exactly as long as it exists (a guard a blast froze, a
//!   decoy still standing, the player while concealment holds).
//! - **How long it lives** ([`MarkLife`]) — **momentary** where the effect *is* a
//!   moment (a bore, a blast's reach: [`EFFECT_FLASH_TURNS`], or as long as the moment's
//!   consequence runs — an eject is lit on just the frames its stun holds the player
//!   down), or **standing** where the effect is a state (a guard still frozen, a live
//!   decoy, concealment in force). One decay schedule serves both
//!   ([`decay_effect_marks`](State::decay_effect_marks)); there is no second timer.
//!
//! A mark is keyed by the **ability** it came from, not by which [`Effect`] is running:
//! Pierce Wall is the one `Behaviour::Coded` ability (§8.1), so it declares no `Effect`
//! at all and a channel keyed on that enum could never reach it. Marks are likewise
//! **latched from the turn's events** ([`record_effect_marks`](State::record_effect_marks))
//! rather than from "is this effect active" — the same reason, and the reason a new
//! effect joins the layer by adding one arm there.
//!
//! # The two readings, and why they sit in different places
//!
//! The renderer reads the layer through two queries, one per placement, because the
//! two make different claims and the §11.5 precedence treats them differently:
//!
//! - [`effect_cell_marks`](State::effect_cell_marks) — the **wash**. The weakest
//!   background there is: a door cue, a sensed guard and a danger cone all paint over
//!   it, because an advisory layer must never hide the detection set §11.5
//!   **[SETTLED]** calls the board's one non-negotiable claim.
//! - [`effect_thing_marks`](State::effect_thing_marks) — a **recolour of a cue the
//!   thing already draws**, never a new mark. It refines the sense channel rather than
//!   competing with it ("a guard is exactly here, *and* it is frozen"; "that `@` is
//!   not a thing you left, it is the ability running"), so it sits above `Sensed` and
//!   still below `Danger`.
//!
//! Net precedence, unchanged from #324: **Danger > a mark on a thing > Sensed / door
//! cues > the wash.**
//!
//! # Fog (§11.5a)
//!
//! A mark on a **thing** is gated on perceiving that thing
//! ([`guard_under_effect`](State::guard_under_effect)), so it can only ever recolour
//! something the player is already shown and can never draw one the fog is hiding. A
//! mark on **cells** needs no such gate and takes none: how far your own gadget reached
//! is your own knowledge, through walls and over ground you have never seen, and it
//! says nothing about the facility's contents.
//!
//! The decoy is a thing whose perception gate is *already* "always" (§11.5a's second
//! exception, #321/#326): it is the player's own placed object, drawn in the FOV and
//! out of it, so its mark follows the glyph it sits under and needs no gate of its own.
//! That is the rule holding, not an exemption from it — the mark is shown exactly when
//! the thing is. The player is the same case, trivially: they are always drawn.
//!
//! # A mark that blinks (§8.3/#341/#416)
//!
//! Camouflage conceals you on any turn you do not move, so the ability being **on** and
//! concealment actually **holding** are two facts, and the §11.4 bar can only report the
//! first. So [`MarkPlace::ConcealedPlayer`] is a standing mark whose drawing is gated on
//! the live rule ([`camouflage_holding`](State::camouflage_holding)): lit once at the
//! activation, dark on a turn the player moved, back on the next still turn. Its mark
//! and its bar entry can **disagree** — and that disagreement is the whole reason it
//! earns a mark at all. An effect that were simply on for its window (Run, Autodoors)
//! would say nothing the bar does not, which is why the rule is "a **conditional**
//! effect", not "an effect".
//!
//! [`MarkPlace::PhasedPlayer`] is the second one to qualify, and it qualifies on that
//! same rule rather than by being an ability. A running Dephase is unconditional and
//! earns nothing; what is conditional is *where you are standing while it runs*. The
//! safety eject fires only if the window ends somewhere a solid body cannot stand, so
//! the mark is gated on [`can_rematerialize`](State::can_rematerialize) — the very
//! predicate the eject rule consumes — and blinks off and on as the player steps out of
//! a wall and back into one, with the bar entry unchanged throughout. The bar says the
//! clock is running; the mark says you are inside something while it does.
//!
//! # Fired, not carried (#325)
//!
//! Confusion is **instant**: the blast is decided **once**, at the moment it is
//! pressed, from the cell the player is standing in
//! ([`confusion_blast`](State::confusion_blast)), and what it caught is carried from
//! there by the guards themselves — each one counting its own daze down (§8.3). There
//! is no ongoing area to travel with the player and no window to switch off; distance
//! stops mattering the instant the flash goes off. That is what makes the ability a
//! panic-buy of time rather than a mobile no-guard-may-act field, which is much closer
//! to the "no shield" the design argues for.
//!
//! So Confusion is exactly one firing wearing **both** placements: a momentary cell
//! mark over the box it went off in — the very [`EffectArea`] the daze was computed
//! from, so the picture cannot disagree with the rule — and a standing thing mark that
//! rides each guard it froze, reading off that guard's own counter and so staying
//! truthful for one that has since walked out of the box.

use serde::{Deserialize, Serialize};

use super::*;

/// The footprint one area effect **fired** with (§8.3/#325): the §6.1 **box** of its
/// radius around the cell it went off in. A box, not a disc — [`Cell::sight_distance`]
/// is the metric the effects themselves are measured in, so a round footprint would be
/// a picture that disagreed with the rule.
///
/// Decided once, at the firing seam ([`State::confusion_blast`]), and then fixed: the
/// player walking away narrows nothing and widens nothing, because the set of guards
/// it caught was settled the moment it went off.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EffectArea {
    pub(super) centre: Cell,
    pub(super) radius: u32,
}

impl EffectArea {
    /// A footprint built **outside a firing** — for a test that needs to hand an event a
    /// geometry without standing a whole [`State`] up to fire one (§11.7's near-line fit
    /// check is the caller, #554).
    ///
    /// Crate-internal, and it is not a second firing seam: every area the *game* acts on
    /// is measured at its own seam ([`confusion_blast`](State::confusion_blast),
    /// [`lockdown_area`](State::lockdown_area), [`false_call_area`](State::false_call_area),
    /// [`repel_area`](State::repel_area)) and carried by value from there, which is the
    /// discipline that keeps the picture and the rule one object.
    #[cfg(test)]
    pub(crate) const fn at(centre: Cell, radius: u32) -> Self {
        Self { centre, radius }
    }

    /// Whether `cell` is inside the footprint — the §6.1 box test, through walls.
    pub fn contains(&self, cell: Cell) -> bool {
        self.centre.sight_distance(cell) <= self.radius
    }

    /// The cell the blast was measured from — where the player stood when it fired.
    pub fn centre(&self) -> Cell {
        self.centre
    }

    /// The box's reach in cells.
    pub fn radius(&self) -> u32 {
        self.radius
    }

    /// The in-bounds cells of the box, as the explicit set a [`MarkPlace::Cells`] mark
    /// is lit with (#338). Clipped to `facility` here, once, at the moment the mark is
    /// placed — the layer itself never re-derives geometry.
    pub fn cells(&self, facility: &Facility) -> Vec<Cell> {
        let (cx, cy) = (self.centre.x, self.centre.y);
        let r = self.radius;
        let ys = cy.saturating_sub(r)..=(cy + r).min(facility.height().saturating_sub(1));
        let xs = cx.saturating_sub(r)..=(cx + r).min(facility.width().saturating_sub(1));
        ys.flat_map(|y| xs.clone().map(move |x| Cell::new(x, y)))
            .collect()
    }
}

/// The radius of `effect` when it acts on an **area** around the player, or `None` when
/// it does not — the one table that says which effects have a footprint at all (§8.3).
///
/// A **cap**, read at the firing seam and clamped there by whatever that effect's own
/// rule says (Confusion's is the guard sense, [`confusion_blast`](State::confusion_blast)).
/// Adding Lockdown's radius (#242) here is all that ticket owes the render layer; its
/// own clamp, if it wants one, is its own — the door sense is not the guard sense, and
/// nothing in this table presumes otherwise.
///
/// This is the effect's **own reach**, not the layer's geometry: since #338 the drawing
/// side reads an explicit cell set and never this table, so a mark that is not a box at
/// all (a bored cell, an eject's landing) needs no row here.
pub(super) fn area_radius(effect: Effect) -> Option<u32> {
    match effect {
        Effect::Confuse => Some(CONFUSION_RADIUS),
        Effect::SealDoors => Some(LOCKDOWN_RADIUS),
        Effect::FakeCall => Some(FALSE_CALL_RADIUS),
        Effect::Repel => Some(REPEL_RADIUS),
        // Everything else acts on the player themselves, not on a region around them.
        // Reversal (#243) acts on the one cell the player is standing in — the guard
        // has to reach *them* — so it has no footprint to draw either.
        Effect::ExtraStep
        | Effect::ConcealWhileStill
        | Effect::SpawnDecoy
        | Effect::Phase
        | Effect::AutoDoors
        | Effect::EnhancedSight
        // The Guide's bearing (#505) acts on **one** cell, picked by direction rather
        // than measured out by a radius — so it has no footprint to declare here, and
        // its cell comes from [`guide_bearing`](State::guide_bearing) instead.
        | Effect::ObjectiveBearing
        | Effect::ReverseCapture => None,
    }
}

/// Where an effect mark lands (§11.5/#338) — one of the two shapes the layer speaks.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub(super) enum MarkPlace {
    /// An explicit set of cells, fixed when the mark was lit: a blast's footprint, the
    /// cell a bore opened, the pair an eject threw you between. Nothing about it is a
    /// live query, so it stays where it happened rather than following the player.
    Cells(Vec<Cell>),
    /// The guards an effect currently **holds** — the mark rides each one wherever it
    /// walks, for as long as it is held, and is gated on perception (§11.5a).
    HeldGuards,
    /// The **piece of Cover the run has deployed** (§8.3/§10.3/#562) — the mark sits on
    /// the table for exactly as long as the window holds it, and follows it as it is
    /// pushed.
    ///
    /// It joins on [`LiveDecoy`](Self::LiveDecoy)'s reasoning almost word for word, and
    /// for the same reason: a piece of Cover is a §10.3 table in every *rule* model —
    /// same terrain, same `π`, same blocking — so the board has nothing to tell it apart
    /// from the furniture the building came with, and *"that `π` is not a table the
    /// generator put there, it is the ability running"* is exactly the fact this channel
    /// exists to add. Without it the only place the difference is stated is the §11.7
    /// usable line, which speaks only when the player is already standing next to it.
    ///
    /// **Background, so it collides with nothing.** §11.5's standing rule is that an
    /// effect colourises the background and the glyph keeps its own meaning — which
    /// matters more here than anywhere, because the glyph channel on this very cell is
    /// already spoken for: §10.3 recolours the whole covering run to `Owned` while it
    /// conceals you (§11.3). So the two compose and each keeps its word: `Owned` ink
    /// says *this furniture is hiding me right now*, the cyan behind it says *and I put
    /// it there*. A colour that tried to say both would say neither.
    ///
    /// Read **live** from [`State::deployed_cover`] rather than latched as a cell, which
    /// is what makes the mark follow a push with nothing to relight it and stop dead the
    /// frame the window ends — the same shape, and the same guarantee,
    /// [`LiveDecoy`](Self::LiveDecoy) gets.
    DeployedCover,
    /// The **live decoy** an effect is running (§8.3/#340) — the mark sits on the fake
    /// for exactly as long as it exists, and on nothing at all once it does not.
    ///
    /// Read live from [`State::decoy`] rather than latched as a cell, which is what
    /// makes "no mark outlives the decoy" a property of the shape instead of a clear
    /// call to remember: a decoy stomped in the guard phase — after this turn's decay
    /// has already run — stops being drawn on the very frame it dies.
    LiveDecoy,
    /// The player, on the turns an effect's **condition** is actually met (§8.3/#341) —
    /// Camouflage, whose concealment holds only while you stand still.
    ///
    /// One of the two placements whose mark **blinks while its ability runs**, and that
    /// is the whole of its job: the §11.4 bar can say the window is open and nothing
    /// more, so a mark that were merely "the ability is on" would repeat it. This one
    /// goes dark on a turn the player moves and comes back the next still turn, which is
    /// the fact the bar has no room to carry. A later conditional effect joins for that
    /// reason — because its mark and its bar entry can **disagree** — and an
    /// unconditional one (Run, Autodoors) never does. [`PhasedPlayer`](Self::PhasedPlayer)
    /// is the second instance, admitted on exactly that ground.
    ConcealedPlayer,
    /// The player, on the turns a running **Dephase** has them somewhere a solid body
    /// cannot stand (§8.3/#416) — i.e. exactly while the safety eject would fire if the
    /// duration ran out now.
    ///
    /// The second conditional mark, and it joins on [`ConcealedPlayer`](Self::ConcealedPlayer)'s
    /// own rule rather than as an exception to it. The mark is **not** "Phase Out is
    /// running" — the bar says that, and a mark that only restated it would earn
    /// nothing. It is "you are inside something, and the clock is running", which the
    /// bar has no room to say and which is the fact that changes what the next turn is
    /// worth: step onto open floor and the risk is nil, step into a wall and the window
    /// closing costs a random throw plus a stun as long as it (§8.3/appendix 12). So the
    /// mark blinks off and on as the player walks wall → floor → wall while the bar
    /// entry never changes — the disagreement that is the price of admission here.
    ///
    /// Read live off [`can_rematerialize`](State::can_rematerialize), the very predicate
    /// the eject rule consumes, so the mark cannot claim a turn the rule would not.
    PhasedPlayer,
    /// The **remote the player is currently flying** (§8.1/#273) — the third conditional
    /// placement, and it joins on [`ConcealedPlayer`](Self::ConcealedPlayer)'s rule
    /// rather than as an exception to it.
    ///
    /// While a control transfer is in force there are two things of yours on the board
    /// and only one of them answers the arrow keys. That is the single thing a player
    /// can get wrong about this ability, and it is a fact the §11.4 bar cannot carry: it
    /// reads `Drone[23]` whether you are at the controls or standing in a room three
    /// corridors away watching a camera. So the mark rides the machine exactly while it
    /// is *yours to drive*, goes dark the moment you hand the keys back, and comes back
    /// on if you take them again — the disagreement with the bar entry that is the price
    /// of admission here.
    ///
    /// It rides the thing rather than the cell for [`LiveDecoy`](Self::LiveDecoy)'s
    /// reason, and it earns its place in this channel for a second one: a remote flies
    /// over guards, and a guard's `g` outranks it in the glyph layer (§11.3 — a threat
    /// is never hidden). The background survives that, so the one cue that says *this is
    /// what your keys move* cannot be covered by the thing you flew it over.
    PilotedRemote,
}

/// How long an effect mark lives (§11.5/#338). Both arms run on the one decay schedule
/// ([`State::decay_effect_marks`]); neither carries a clock of its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub(super) enum MarkLife {
    /// A **moment**: this many more spent turns, then gone. The effect *is* an event —
    /// a blast's reach, a bore — and the mark is the frame that reports it.
    ///
    /// Usually [`EFFECT_FLASH_TURNS`], the one frame. It is a **count** rather than a
    /// flag because an event whose consequence outlasts it should be readable for as
    /// long as that consequence runs, and no longer: the safety eject is lit for the
    /// stun it deals (#339), so the mark neither expires while the player it is
    /// speaking to is still unable to act nor survives into the frame they act from.
    /// Still a moment, still on the one decay schedule — what varies is how long the
    /// report is left up, never whether something has to notice it end.
    ///
    /// A life of N is **N renders**, the frame it was lit in being the first — the
    /// decay runs at the head of the next spent turn, not at the end of this one.
    Momentary(u32),
    /// A **state**: shown for exactly as long as the effect holds, with no countdown.
    /// It ends when the thing it marks stops being held, or when the ability's window
    /// ends ([`State::clear_effect_marks`]).
    Standing,
}

/// A live **effect mark** (§8.3/§11.5, #308/#324/#338): one ability effect, made
/// visible as a background over a place, for a stated lifetime.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub(super) struct EffectMark {
    /// The ability the mark came from — an [`AbilityId`] rather than an [`Effect`], so
    /// the key reaches a `Behaviour::Coded` ability (Pierce Wall, §8.1) as easily as a
    /// data-driven one, and so a window that ends can clear exactly its own marks.
    pub(super) source: AbilityId,
    pub(super) place: MarkPlace,
    pub(super) life: MarkLife,
}

/// The empty cell slice a non-cell mark contributes to the wash — a `const` so the
/// borrow outlives the match arm.
const NO_CELLS: &[Cell] = &[];

/// Which of the eight cells around `from` lies in the direction of `to` (§8.3/#505) —
/// the compass needle, resolved to a neighbour. `None` when `to` *is* `from`, or when
/// the neighbour would fall outside the grid.
///
/// **Eight equal 45° sectors.** The minor axis is dropped — a pure N/E/S/W bearing —
/// only when the target is inside 22.5° of that axis; otherwise the answer is the
/// diagonal. The test is exact integer arithmetic, which is worth a line of explanation
/// because it looks like a magic inequality:
///
/// ```text
/// axis  ⟺  b/a < tan 22.5° = √2 − 1  ⟺  b(√2 + 1) < a  ⟺  b√2 < a − b  ⟺  2b² < (a − b)²
/// ```
///
/// with `a = max(|dx|, |dy|)` and `b = min(…)`, and the final form valid because `a > b`
/// is checked first. So the sectors are genuinely equal and there is no float anywhere —
/// §12.4 keeps a replay's compass pointing the same way on every machine.
///
/// **Diagonals are kept even though movement is cardinal** (§8.3). It is a needle, not
/// a suggested move: rounding a diagonal bearing to the nearest cardinal would throw
/// away half the information the ability has to give.
fn bearing_cell(from: Cell, to: Cell) -> Option<Cell> {
    let (dx, dy) = (to.x as i64 - from.x as i64, to.y as i64 - from.y as i64);
    if dx == 0 && dy == 0 {
        return None;
    }
    let (ax, ay) = (dx.abs(), dy.abs());
    let (a, b) = (ax.max(ay), ax.min(ay));
    // Inside 22.5° of the major axis: drop the minor component and point straight.
    let axis = a > b && 2 * b * b < (a - b) * (a - b);
    let (mut sx, mut sy) = (dx.signum(), dy.signum());
    if axis {
        if ax >= ay {
            sy = 0;
        } else {
            sx = 0;
        }
    }
    let (nx, ny) = (from.x as i64 + sx, from.y as i64 + sy);
    (nx >= 0 && ny >= 0).then(|| Cell::new(nx as u32, ny as u32))
}

impl State {
    /// The blast **Confusion fires from where the player stands** (§8.3/§9/#240/#325):
    /// the §6.1 box of [`CONFUSION_RADIUS`], clamped down to the player's live
    /// [`sense_range`](Self::sense_range).
    ///
    /// ```text
    /// effective radius = min(CONFUSION_RADIUS, sense_range())
    /// ```
    ///
    /// The clamp can only ever **shrink** the blast, never widen it: [`CONFUSION_RADIUS`]
    /// stays the catalogue's **[START]** cap, so no change to the sense — a Wait's widened
    /// 20, a future modifier, salvaged tech — can make Confusion reach further than its
    /// own row says. What it does do is keep #240's promise as a *rule* rather than as a
    /// coincidence of two constants: the blast never freezes what the player cannot
    /// sense. On open floor it is inert (`min(6, 10)` = 6); inside a duct
    /// ([`DUCT_SENSE_RANGE`] = 5, §10.7) it closes the hole where a crawling player
    /// would otherwise daze a guard at 6 they cannot perceive at all. That nerf is the
    /// point: degraded information is the crawlspace's whole cost.
    ///
    /// It reads the sense ladder *itself*, never a duct check or any other
    /// re-derivation, so whatever changes the sense later is picked up here for free
    /// and there is no second place to keep in step.
    ///
    /// **The `sense_suppressed` modifier is the one thing it deliberately does not pick
    /// up** (§12.6/#493), because that modifier does not move the ladder: it suppresses the
    /// **channel the player perceives** and leaves [`sense_range`](Self::sense_range) — the
    /// rule input — exactly where it was. Had it zeroed the range instead, this clamp would
    /// have zeroed the blast with it and a level modifier would have silently deleted an
    /// ability from the loadout, which is the dead-verb case §13.2's histogram exists to
    /// catch. So with the sense off Confusion still reaches its full radius and still
    /// freezes every guard a sensing player would have sensed — the clamp's **[SETTLED]**
    /// wording holds, and what the player loses is the sight of it landing, not the blast.
    ///
    /// **The read moment is pinned in the reading** (#325/#345): the clamp is
    /// [`acting_sense_range`](Self::acting_sense_range), the sense as an action taken
    /// now sees it, so §9.1's widened Wait can never reach into a blast fired the turn
    /// after it. The cap absorbs a stale one today — `min(6, 20)` is 6 — but pinning it
    /// here is what stops a later change to the cap quietly resurrecting a blast
    /// widened by last turn's Wait. It also makes this a **pure** function of the
    /// board: the ability bar asks it every frame to decide whether the press would
    /// catch anybody (§11.4/#345), and must get the answer the press itself would,
    /// even on the frame straight after a Wait.
    pub fn confusion_blast(&self) -> EffectArea {
        EffectArea {
            centre: self.player,
            radius: area_radius(Effect::Confuse)
                .expect("Confusion is an area effect")
                .min(self.acting_sense_range()),
        }
    }

    /// The reach **a False Call fired from where the player stands would broadcast
    /// over** (§7.7/§8.3/#504): the §6.1 box of [`FALSE_CALL_RADIUS`], read from the one
    /// [`area_radius`] table so the ability's reach and the table cannot drift apart.
    ///
    /// The firing seam, in [`confusion_blast`](Self::confusion_blast)'s and
    /// [`lockdown_area`](Self::lockdown_area)'s shape and for the same reason: one
    /// object carries the geometry to the rule that picks the guards
    /// ([`fire_false_call`](Self::fire_false_call)), to the event, and to the mark the
    /// player reads — so what is painted is what was measured.
    ///
    /// **Unclamped**, which is [`lockdown_area`](Self::lockdown_area)'s answer rather
    /// than [`confusion_blast`](Self::confusion_blast)'s, and the difference between the
    /// two is what decides it. Confusion is narrowed to what the player can *perceive*
    /// because a guard frozen out of sense range is an effect with no readout at all.
    /// Neither of the two facts that makes true is true here. A called guard is not held
    /// out of view: it **walks to you**, arriving inside the §9 sense long before it
    /// arrives anywhere else, so the effect delivers its own readout. And the near line
    /// says **how many answered** at the moment of firing (§11.7), so what was summoned
    /// is stated even where it cannot yet be sensed.
    ///
    /// It is a **radio**, and clamping a transmitter to eyesight is not a rule about
    /// radios — it is the rule for a blast, borrowed. The guard sense is the player's
    /// body; the reach here is the device in their hand, and the two are not the same
    /// channel (§9's "the sense is a separate, innate channel" cuts both ways). The
    /// consequence worth stating is the one that follows for a **duct** (§10.7): a
    /// crawling player broadcasts exactly as far as a standing one, because a
    /// crawlspace degrades *perception* and a transmitter does not perceive.
    ///
    /// It is pure, so the ability bar may ask it every frame to decide whether the press
    /// would reach anybody (§11.4/#345) and get the answer the press itself would — and
    /// unlike Confusion there is no read-moment question at all, since §9.1's widened
    /// Wait is not something this ability is measured in.
    ///
    /// The centre is where the player is standing, and the call names that cell **by
    /// value** ([`fire_false_call`](Self::fire_false_call)): the responders keep walking
    /// to where you *were*. That staleness is the ability, not a limitation of it —
    /// §7.7 already says as much about genuine calls ("the searched cell is stale by
    /// construction… the tail is not the threat, the net is").
    pub fn false_call_area(&self) -> EffectArea {
        EffectArea {
            centre: self.player,
            radius: area_radius(Effect::FakeCall).expect("False Call is an area effect"),
        }
    }

    /// **The Guide's bearing** (§8.3/§11.5a/#505): which of the eight cells around the
    /// player lies in the direction of the nearest **unclaimed objective**, or `None`
    /// when the run does not hold the ability or there is nothing left to point at.
    ///
    /// # A compass, not a route
    ///
    /// "Nearest" here is the **straight line**, ignoring walls, doors and reachability
    /// entirely — the deliberate opposite of §7.3's "nearest means the shortest walk".
    /// Control dispatches by walk because it is routing a guard; this points because it
    /// is pointing. **Do not "fix" this into a pathfind**: a guide that pathed would be
    /// a solver, it would answer §10's exploration outright, and the first bug report
    /// ("it points into a wall") is the specification rather than a defect.
    ///
    /// # What counts as an objective
    ///
    /// Unclaimed intel consoles and unclaimed equipment caches — both things you go and
    /// *take*, both inert once taken (§11.2's "spent objectives"), so a claim drops one
    /// out of the candidate set on the turn it happens. Caches are campaign-only (§8.3),
    /// so in a quick-play facility this is a compass to consoles, which is the same
    /// worded-over-both shape Autodoors already has.
    ///
    /// **Not the comms console.** §7.3 is explicit that "the cost is the route, not the
    /// switch", and that its placement distance is a balance knob the sim sweeps (#448);
    /// a passive that pointed at it would hand the counterplay over for the price of a
    /// slot and quietly re-tune that knob. It is also not an objective — you never have
    /// to take it. **Not the exit** either: the tunnel is drawn as itself from turn one
    /// (§11.5a), so it needs no compass.
    ///
    /// # It reveals nothing else (§11.5a **[SETTLED]**)
    ///
    /// The objective stays unrevealed, unremembered and undrawn until the player has
    /// eyes on it. What this hands over is **an eighth of a circle** and not a location,
    /// which is exactly what leaves #215's v3 intel sink — POI reveal, sold for currency
    /// — something to sell. Nothing here touches tile memory or the fog.
    ///
    /// # It pulses (§8.3/#505)
    ///
    /// The bearing shows on one turn in [`GUIDE_BLINK_TURNS`] and is **dark on the
    /// rest**, turn zero included. A standing needle is a line you follow without
    /// thinking; a pulsing one gives you a *fix* you then have to walk on your own
    /// memory of, which is both what a compass feels like to use and what leaves
    /// §11.5a's exploration intact. The phase is the **turn counter** and nothing else,
    /// so it is deterministic (§12.4), it is the same for every run, and a player can
    /// count to it.
    ///
    /// # Determinism (§12.4)
    ///
    /// Two equidistant objectives are separated by a **fixed** rule and never a draw: the
    /// level's own ordering, consoles in placement order and then caches. A compass that
    /// flickered between two answers on a replay would be a desync.
    pub fn guide_bearing(&self) -> Option<Cell> {
        if !self.abilities.effect_active(Effect::ObjectiveBearing) {
            return None;
        }
        // Dark on turn zero and on the two turns between each fix — see
        // [`GUIDE_BLINK_TURNS`]. A `0 % n == 0` would light the opening frame, which is
        // the one frame the ability must not answer on.
        if self.turn == 0 || !self.turn.is_multiple_of(GUIDE_BLINK_TURNS) {
            return None;
        }
        let candidates = self
            .objectives
            .iter()
            .filter(|o| !o.taken)
            .map(|o| o.cell)
            .chain(self.caches.iter().filter(|c| !c.taken).map(|c| c.cell));
        // Squared Euclidean, in exact integers: the metric is the crow's line, and
        // squaring keeps it off floating point, which §12.4 would otherwise make a
        // hazard. `min_by_key` keeps the **first** minimum, so the candidate order above
        // *is* the tiebreak — fixed, seed-independent, identical on replay.
        let target = candidates.min_by_key(|&cell| {
            let (dx, dy) = (
                cell.x as i64 - self.player.x as i64,
                cell.y as i64 - self.player.y as i64,
            );
            dx * dx + dy * dy
        })?;
        bearing_cell(self.player, target)
    }

    /// The area **a Lockdown fired from where the player stands would seal** (§8.3/#242):
    /// the §6.1 box of [`LOCKDOWN_RADIUS`], read from the one [`area_radius`] table so
    /// the ability's reach and the table cannot drift apart.
    ///
    /// The firing seam, in [`confusion_blast`](Self::confusion_blast)'s shape and for the
    /// same reason: one object carries the geometry to the rule that picks the doors
    /// ([`lockdown_doors`](Self::lockdown_doors)), to the event, and to the mark the
    /// player reads — so what is painted is what was measured, never a redrawing of it.
    ///
    /// **Unclamped**, unlike the blast. Confusion is narrowed to what the player can
    /// *perceive*, because freezing a guard you cannot sense is unreadable; a seal is a
    /// fact about doors, and a door sealed out of sense range is still sealed and still
    /// marked when you walk back to it. The door sense is not the guard sense.
    pub fn lockdown_area(&self) -> EffectArea {
        EffectArea {
            centre: self.player,
            radius: area_radius(Effect::SealDoors).expect("Lockdown is an area effect"),
        }
    }

    /// The cells the §11.5 effect layer washes as its **weakest background** (#338):
    /// every [`MarkPlace::Cells`] mark still alive, in the order they were lit.
    ///
    /// Each set is an explicit one, fixed when the mark was placed — for Confusion, the
    /// very [`EffectArea`] the daze was computed from, so a footprint can never disagree
    /// with what the blast actually caught, and it stays where it went off rather than
    /// following the player who fired it. Painted through walls and fog on purpose: the
    /// reach of your own gadget is not something the fog can keep from you, and it
    /// reveals nothing about the facility (§11.5a).
    /// The **Guide's** bearing joins this reading (§8.3/#505) rather than the mark
    /// record, and it is the layer's one **live** cell read. A passive is held from
    /// level start and has no activation event to latch from
    /// ([`record_effect_marks`](Self::record_effect_marks) keys on *what happened*), so
    /// there is no moment at which a `MarkPlace::Cells` could be lit — and a bearing is
    /// a live query anyway, recomputed as the player walks and as objectives are
    /// claimed. It is the same reasoning that makes
    /// [`ConcealedPlayer`](MarkPlace::ConcealedPlayer) and
    /// [`PhasedPlayer`](MarkPlace::PhasedPlayer) live reads on the other query.
    ///
    /// It lands **here**, in the wash, deliberately: this is the weakest background on
    /// the board, so the compass loses to a sensed dot, a watcher line and the danger
    /// overlay alike. That ordering is not incidental — the Guide is a convenience, and
    /// it must never sit on top of the thing that can kill you (§11.5 **[SETTLED]**).
    /// Yielded last for the same reason, though paint order among washes cannot matter:
    /// one background, painted once.
    pub fn effect_cell_marks(&self) -> impl Iterator<Item = Cell> + '_ {
        self.effect_marks
            .iter()
            .flat_map(|mark| match &mark.place {
                MarkPlace::Cells(cells) => cells.as_slice(),
                MarkPlace::HeldGuards
                | MarkPlace::LiveDecoy
                | MarkPlace::DeployedCover
                | MarkPlace::ConcealedPlayer
                | MarkPlace::PhasedPlayer
                | MarkPlace::PilotedRemote => NO_CELLS,
            })
            .copied()
            .chain(self.guide_bearing())
    }

    /// The cells where a mark rides a **thing** (#338/#340/#341): the position of every
    /// guard an effect currently holds and the player can perceive, the cell of a live
    /// decoy, and the player's own cell on the turns concealment is in force.
    ///
    /// A *recolour* of a cue the thing already draws, never a new mark, which is why it
    /// outranks the `Sensed` channel it refines and can give nothing away to the fog —
    /// [`guard_under_effect`](Self::guard_under_effect) carries the perception gate.
    /// It reads the guards' own counters rather than the box a blast once covered, so
    /// it stays truthful for the guard the player fired at and then ran away from.
    ///
    /// The decoy is read the same way — off [`decoy`](Self::decoy) itself, not off a
    /// cell the mark remembers — so the wash is on the fake exactly while the fake is
    /// on the board and never for a frame after. It carries no perception gate because
    /// the thing it recolours carries none either (§11.5a's second exception, #321):
    /// the fake is drawn wherever it stands, in view or out of it, and a mark that
    /// vanished when you walked away would be a mark the ability cannot use.
    ///
    /// The player is read live too, through the very predicates the rules themselves
    /// consume — [`camouflage_holding`](Self::camouflage_holding) for concealment and
    /// [`can_rematerialize`](Self::can_rematerialize) for a running Dephase — so neither
    /// mark can claim a turn its rule does not. These are the two marks that **blink
    /// while their ability runs**: concealment dark on a turn the player moved and back
    /// on the next still one (#341), the phase mark dark on open floor and lit inside a
    /// solid (#416). In both cases the blinking half is exactly what the §11.4 bar has
    /// no room to say. They share a cell and so share one yield — the background is
    /// painted once either way.
    pub fn effect_thing_marks(&self) -> impl Iterator<Item = Cell> + '_ {
        let riding = |place: MarkPlace| self.effect_marks.iter().any(|mark| mark.place == place);
        let held = riding(MarkPlace::HeldGuards);
        let decoyed = riding(MarkPlace::LiveDecoy);
        // The deployed table is read live off the state, like the decoy: the mark rides
        // the piece through every push and goes out on the frame the window ends.
        let covered = riding(MarkPlace::DeployedCover);
        let concealed = riding(MarkPlace::ConcealedPlayer) && self.camouflage_holding();
        // Both player marks land on the one cell, so they are folded into a single
        // yield: the layer paints a background, and painting it twice would say
        // nothing the first paint did not.
        let phased = riding(MarkPlace::PhasedPlayer) && !self.can_rematerialize();
        // The remote is marked only while the keys are actually its (#273) — read live
        // off [`piloting`](State::piloting), the same flag the turn loop dispatches on,
        // so the cue and the rule are one fact. It carries no perception gate for the
        // decoy's reason (§11.5a's second exception): a machine of your own is drawn
        // wherever it is, and it is trivially inside its own camera anyway.
        let piloted = riding(MarkPlace::PilotedRemote) && self.piloting;
        self.guards
            .iter()
            .filter(move |guard| held && self.guard_under_effect(guard))
            .map(|guard| guard.pos())
            .chain(self.decoy.filter(|_| decoyed))
            .chain(self.cover.filter(|_| covered))
            .chain((concealed || phased).then_some(self.player))
            .chain(self.remote.filter(|_| piloted).map(|remote| remote.cell))
    }

    /// Whether Camouflage's concealment is **in force right now** (§8.3): its window is
    /// open *and* the last spent turn did not move the player.
    ///
    /// The one statement of "am I hidden by the camo this turn?", read by the rule
    /// ([`concealed_from`](Self::concealed_from)) and by the mark that reports it
    /// ([`effect_thing_marks`](Self::effect_thing_marks)) alike, so the board and the
    /// detection set cannot drift apart — the §11.5 discipline that the picture is the
    /// rule rather than a second derivation of it.
    ///
    /// It is deliberately **narrower** than concealment: a cupboard and a duct conceal
    /// too (§10.3/§10.7), and neither is this ability. Marking those would say
    /// "Camouflage is working" about a player who is merely indoors.
    ///
    /// The activation turn counts (§8.2): activating is a spent action that does not
    /// move, so `moved_this_turn` is already false on the frame drawn straight after the
    /// press and the mark is up from the first turn the ability protects — never lagging
    /// a turn behind the rule it draws.
    pub fn camouflage_holding(&self) -> bool {
        self.abilities.effect_active(Effect::ConcealWhileStill) && !self.moved_this_turn
    }

    /// Whether `guard` is currently held by an area effect the player can read — the
    /// predicate behind its [`effect_thing_marks`](Self::effect_thing_marks) mark
    /// (§11.2's [`Category::Effect`], #308).
    ///
    /// It reads the guard's own daze ([`guard_confused`](Self::guard_confused)), not the
    /// box a blast once covered, which is what makes it *truthful* for the guard the
    /// player fired at and then ran away from: still frozen, still marked, wherever
    /// either of them now stands (#325).
    ///
    /// Gated on the guard being **perceived** ([`perceive_guard`](Self::perceive_guard)):
    /// the mark only ever recolours a guard the player is already shown — the seen `g`
    /// or the sensed dot — so it can never draw a guard the fog is hiding (§11.5a). A
    /// guard is always perceivable at the moment it is caught, since the blast is
    /// clamped inside the sense; a dazed guard that later drifts out of the sense simply
    /// stops being drawn at all, and silence is the honest answer there.
    pub fn guard_under_effect(&self, guard: &Guard) -> bool {
        self.perceive_guard(guard).is_some() && self.guard_confused(guard)
    }

    /// Fire the Confusion blast `area` (§8.3/#325): daze every guard standing inside it
    /// **right now**, for [`CONFUSION_DAZE_TURNS`] each, and report what it caught.
    ///
    /// This is the whole mechanic, and it is over in one call. The set is taken here
    /// and nowhere else: a guard that wanders into these cells next turn was not in the
    /// blast and is untouched, and a dazed guard carried out of them keeps its count.
    /// Nothing but the daze is written — state, lead, destination and focus are left
    /// exactly as they were, which is the "pause, not reset" §8.3 asks for.
    ///
    /// Called from the activation seam once the deck has actually switched the ability
    /// on, so a refused press fires nothing.
    pub(super) fn fire_confusion(&mut self, area: EffectArea, events: &mut Vec<Event>) {
        let mut caught = 0;
        for guard in &mut self.guards {
            if area.contains(guard.pos()) {
                guard.daze(CONFUSION_DAZE_TURNS);
                caught += 1;
            }
        }
        events.push(Event::ConfusionFired {
            blast: area,
            caught,
        });
    }

    /// Fire the False Call over `reach` (§7.7/§8.3/#504): call every guard standing
    /// inside it **right now** to the cell it was fired from, and report how many
    /// answered.
    ///
    /// The world change is [`send_call`](Self::send_call) and nothing else — the same
    /// call control makes when a post goes silent and the same one a guard makes on a
    /// lost sighting, with the *who* narrowed to the transmitter's box instead of to a
    /// count. So the responders **search** (§7.6) rather than chase, a guard that has
    /// the live player is never pulled off it, a guard already on an errand is
    /// redirected like any other respondable one, and a killed net answers with nobody
    /// — none of which this function decides, because none of it is this ability's to
    /// decide.
    ///
    /// The set is taken here and nowhere else. A guard that wanders into the reach next
    /// turn was not in the call and is untouched; the cell named is a snapshot passed by
    /// value, so walking away narrows nothing, widens nothing, and moves nothing.
    ///
    /// Nothing steps the alert ladder (§7.3). Nothing was seen and no ping was missed —
    /// a forged transmission is, to the facility, an ordinary call — so escalating here
    /// would be the ability climbing the ladder by a side door.
    pub(super) fn fire_false_call(&mut self, reach: EffectArea, events: &mut Vec<Event>) {
        let answered = self.send_call(reach.centre(), self.guards.len(), |guard, _| {
            reach.contains(guard.pos())
        });
        events.push(Event::FalseCallFired {
            reach,
            answered: answered as u32,
        });
    }

    /// Count one turn off every dazed guard (§8.3/#325). Run once per **spent** turn,
    /// at end of turn beside the ability clocks, on §8.2's convention: a guard dazed
    /// for N is frozen for N turns *including* the one the blast went off in, every
    /// phase of which already saw it frozen.
    ///
    /// It ticks every guard, not only the ones phase 3 let act — a dazed guard is
    /// precisely the one phase 3 skips, so a count that ran inside the guard phase
    /// would never run at all.
    pub(super) fn tick_guard_daze(&mut self) {
        for guard in &mut self.guards {
            guard.shake_off_daze();
        }
    }

    /// Light the marks of every ability effect that acted this turn, read off the
    /// turn's events (§11.5/#308/#325/#338). Called after
    /// [`decay_effect_marks`](Self::decay_effect_marks) has already spent the older
    /// marks' turn, exactly as [`record_door_cues`](Self::record_door_cues) is, so a
    /// mark placed this turn keeps its full life — and **once, at the very end of the
    /// spent turn**, so that every phase that can produce an effect event has already
    /// run (the safety eject, #339, resolves after the ability clocks).
    ///
    /// **This is the whole extension point.** A new effect becomes visible by adding an
    /// arm here that names its place and its lifetime — nothing else in the layer, and
    /// nothing at all in the renderer, has to change. Keying on *what happened* rather
    /// than on which effect is running is what lets a `Behaviour::Coded` ability (Pierce
    /// Wall, §8.1) into a channel it declares no [`Effect`] for.
    ///
    /// Geometry comes off the event rather than being measured again from the player,
    /// who by now may have taken an extra step (§8.3's Run) since the blast went off:
    /// what is drawn is the object the mechanic resolved against, carried through by
    /// value.
    ///
    /// A refusal lights nothing. `Event::BoreRefused` has no arm on purpose (§11.7): a
    /// press that changed nothing is a *message*, and painting the wall it declined to
    /// open would claim an effect that never happened.
    pub(super) fn record_effect_marks(&mut self, events: &[Event]) {
        for event in events {
            match *event {
                // Confusion is one firing in both placements (§8.3/#325): a momentary
                // wash over the box it reached, and a standing mark on what it froze.
                Event::ConfusionFired { blast, .. } => {
                    let cells = blast.cells(self.layout.facility());
                    self.light_mark(
                        AbilityId::Confusion,
                        MarkPlace::Cells(cells),
                        MarkLife::Momentary(EFFECT_FLASH_TURNS),
                    );
                    self.light_mark(
                        AbilityId::Confusion,
                        MarkPlace::HeldGuards,
                        MarkLife::Standing,
                    );
                }
                // Pierce Wall has no window and no clock to hang a mark on (§8.2/#302):
                // the moment of firing is the only thing there is to draw, so the cell
                // it opened is washed for exactly the turn it opened in. One glyph
                // flipping `#` → floor on a 40×40 board is otherwise the whole of a
                // bore's feedback.
                // Lockdown wears **both** lifetimes over cells (§8.3/#242), which is
                // Confusion's shape with the placements swapped: a momentary wash over
                // the box it fired with, and a standing mark on the doorways it holds.
                // The two answer different questions and neither substitutes for the
                // other — *this far*, once, and *these ones*, throughout.
                Event::DoorsSealed { reach, .. } => {
                    // The **wash**: how far the seal reached, for the firing frame only
                    // — the one thing the doors themselves cannot say, and the same
                    // question Confusion's box answers.
                    self.light_mark(
                        AbilityId::Lockdown,
                        MarkPlace::Cells(reach.cells(self.layout.facility())),
                        MarkLife::Momentary(EFFECT_FLASH_TURNS),
                    );
                    // The **state**: which doorways are actually held, for as long as
                    // the window holds them. This is the layer's first *standing cell*
                    // mark — the case #338 left open — and it is what the player plays
                    // off once the wash has gone: a route the guards cannot work.
                    self.light_mark(
                        AbilityId::Lockdown,
                        MarkPlace::Cells(self.sealed_door_cells().collect()),
                        MarkLife::Standing,
                    );
                }
                // The False Call wears the **wash alone** (§7.7/§8.3/#504), which is
                // Confusion's pair with the standing half deliberately absent. The box
                // is a moment — *this far the message carried* — and it is the one
                // thing neither the board nor the near line can otherwise say, since
                // most of what it reached is behind a wall.
                //
                // What it caught needs no mark of its own, and that is the difference
                // from a blast: a called guard is not *held*, it is walking, and the
                // §7.7 legibility tell for a call is already the responder's own sensed
                // dot peeling off toward the cell (§9/§9.3). Recolouring those dots
                // would restate the thing they are doing while the player watches them
                // do it — and would go on claiming an effect over a guard that had long
                // since finished its search and gone back to its beat.
                // Repel wears **one** mark, and the one it does not wear is the
                // interesting half (§8.3/#554). Lockdown flashes its box for a frame and
                // then marks the doorways it holds, because the box and the state are two
                // different facts — *this far*, once, and *these ones*, throughout. Here
                // they are the same fact: the box **is** the wall. So a momentary wash
                // beside the standing one would draw the same cells twice and teach
                // nothing the second time.
                //
                // Standing, over the very [`EffectArea`] the guards are actually held by,
                // so the ground the player is playing off and the ground the rule enforces
                // cannot disagree. It ends with the window, from
                // [`clear_effect_marks`](Self::clear_effect_marks), on the same teardown
                // that releases the field itself.
                Event::RepelFired { field } => self.light_mark(
                    AbilityId::Repel,
                    MarkPlace::Cells(field.cells(self.layout.facility())),
                    MarkLife::Standing,
                ),
                Event::FalseCallFired { reach, .. } => self.light_mark(
                    AbilityId::FalseCall,
                    MarkPlace::Cells(reach.cells(self.layout.facility())),
                    MarkLife::Momentary(EFFECT_FLASH_TURNS),
                ),
                // The **dart's flight** (§8.3/#239) wears the wash alone, and for False
                // Call's reason with the geometry swapped: a line rather than a box. The
                // one thing neither the board nor the near line can say is *where the dart
                // went and how far it got* — the guard it dropped is a body the player can
                // see, and a guard it did not drop leaves nothing behind at all — so the
                // path is drawn once, on the firing frame, and then gone.
                //
                // **It is painted through the fog, and the clamp is what makes that safe.**
                // How far your own gadget reached is your own knowledge (§11.5a), which is
                // why False Call's box needs no perception gate either. But a *ray* says
                // more than a box does: it stops where it stopped, so a wash ending short
                // would report something standing there. That is why the flight is clamped
                // inside the guard sense ([`dart_shot`](State::dart_shot)) — everything the
                // dart can stop on is already drawn for the player as a seen `g` or a §9
                // dot, so the short line restates the board rather than extending it.
                //
                // Reconstructed from the event's own three fields rather than from the
                // player, who by now may have been moved (§8.3's Run takes an extra step,
                // and the mark is lit at the end of the turn): a cardinal ray is exactly its
                // origin, its direction and its length, so what is painted is what was
                // measured. The origin itself is left out — the player's own cell is drawn
                // as the `@` and washing under it would say the dart hit the shooter.
                Event::DartFired {
                    from,
                    dir,
                    travelled,
                    ..
                } => {
                    let path: Vec<Cell> = std::iter::successors(Some(from), |cell| cell.step(dir))
                        .skip(1)
                        .take(travelled as usize)
                        .collect();
                    self.light_mark(
                        AbilityId::Dart,
                        MarkPlace::Cells(path),
                        MarkLife::Momentary(EFFECT_FLASH_TURNS),
                    );
                }
                Event::WallBored { at } => self.light_mark(
                    AbilityId::PierceWall,
                    MarkPlace::Cells(vec![at]),
                    MarkLife::Momentary(EFFECT_FLASH_TURNS),
                ),
                // The safety eject is one event with **two ends** (§8.3/#329/#339): the
                // solid the phase stranded you in, and the cell it threw you onto. Both
                // are washed, because what the stunned player needs is not either cell
                // but the *distance between them* — that span is what priced the stun
                // ([`phase_eject_stun`]), and the `@` simply appearing several cells away
                // says nothing about where it came from.
                //
                // **The pair is lit on exactly the frames the player cannot act from**,
                // and not one more. A one-frame flash would be the one cue in the game
                // that expires while its reader is held down — the eject is followed
                // immediately by turns the player spends helpless, and telling them where
                // they were thrown from *after* it stopped mattering is the #339
                // complaint restated rather than fixed. Overshooting is its own fault:
                // a mark still lit on the frame the player is choosing a real move from
                // is reporting an event they have already finished paying for.
                //
                // `stunned` is exactly that count, with no adjustment. A
                // [`MarkLife::Momentary`] life of N yields N renders, the throw's own
                // frame being the first — and that frame is already one the player cannot
                // act from, since the stun is set before it is drawn. So the mark is lit
                // on every frame whose press will be eaten and dark on the first frame
                // whose press is answered, which is the same thing as saying it is lit
                // exactly while [`stunned`](State::stunned) is non-zero. Taken off the
                // event, so the mark and the helplessness cannot disagree however the
                // stun is later priced.
                //
                // It stays **momentary**, not standing: this is one event given a stated
                // life, not a state being reported. Nothing has to notice when it ends.
                //
                // Marking the *player* while stunned would draw the same picture — they
                // are stunned in place on the landing end, and a thing mark and the wash
                // are one background — so the layer keeps the cheaper of the two shapes
                // and needs no new placement.
                //
                // The origin is a **solid**, and it is marked anyway: the layer paints
                // over the geometry it finds rather than only over floor, and a cell the
                // player occupied a moment ago is their own knowledge, not a reveal
                // (§11.5a). The landing is drawn from the event too, not from
                // `self.player`, so a decoy stomped on arrival — or anything else that
                // moves them afterwards — cannot shift the mark off the cell the throw
                // actually ended on.
                Event::Ejected { from, to, stunned } => self.light_mark(
                    AbilityId::Dephase,
                    MarkPlace::Cells(vec![from, to]),
                    MarkLife::Momentary(stunned),
                ),
                // The eject with nowhere to go (§8.3): one cell, and it is the one that
                // entombed you. The run is over on this frame, so the mark's whole job is
                // to say *where* — the last thing the board has to tell.
                Event::Entombed { at } => self.light_mark(
                    AbilityId::Dephase,
                    MarkPlace::Cells(vec![at]),
                    MarkLife::Momentary(EFFECT_FLASH_TURNS),
                ),
                // A live decoy is a running ability, not a thing you happened to leave
                // (§8.3/#340). The fake already wears the player's own `Owned` `@` and
                // is told from the real one by position alone; the standing mark is what
                // says *this one is the ability*, and it says it for the decoy's whole
                // life, in the same place and on the same clock as the bar's `[12]`.
                //
                // There is no spawn event of its own — the decoy is placed by the
                // activation — so the arm keys on the activation that placed it. Which
                // ability that is comes off the event rather than being assumed, so a
                // second decoy-spawning ability would join the layer for free.
                // A piece of Cover is a §10.3 table in every rule model (§8.3/#562), which
                // is the design — and it leaves the board with nothing to say *whose*
                // table it is. This is that fact, in the one channel §11.5 has for it:
                // a background under a glyph whose own colour is already §10.3's
                // (`Owned` while the run conceals you), so the two compose rather than
                // compete.
                //
                // Lit **once**, at the activation, and then left to ride: the placement
                // is a live read of [`deployed_cover`](State::deployed_cover), so a push
                // moves the mark with nothing to relight and the window ending takes it
                // with the table. Keyed on the ability's identity rather than on an
                // [`Effect`] because Cover is `Behaviour::Coded` (§8.1) and declares
                // none — Pierce Wall's arm has the same shape for the same reason.
                Event::AbilityActivated { ability, .. } if ability == AbilityId::Cover => {
                    self.light_mark(ability, MarkPlace::DeployedCover, MarkLife::Standing)
                }
                Event::AbilityActivated { ability, .. }
                    if declares(ability, Effect::SpawnDecoy) =>
                {
                    self.light_mark(ability, MarkPlace::LiveDecoy, MarkLife::Standing)
                }
                // Camouflage conceals you on any turn you do not move (§8.3/#341), so
                // "the ability is on" and "you are hidden right now" are two different
                // facts, and the §11.4 bar can only ever report the first — it reads
                // `Camo[7]` whether you are standing still and invisible or walking
                // across a lit corridor in plain sight. The mark carries the other half,
                // which is the board answering the only question this ability raises.
                //
                // Lit **once**, at the activation, and then left to blink: the placement
                // is a live read of the concealment rule, so the turns it goes dark on
                // are the turns the rule lapses and nothing has to relight it. A mark
                // relit per still turn would be the same picture drawn by a second
                // schedule that could disagree with the first.
                Event::AbilityActivated { ability, .. }
                    if declares(ability, Effect::ConcealWhileStill) =>
                {
                    self.light_mark(ability, MarkPlace::ConcealedPlayer, MarkLife::Standing)
                }
                // Dephase's window is the same shape (§8.3/#416): lit once at the
                // activation and then left to blink, because the placement is a live
                // read of the eject rule and the turns it goes dark on are the turns
                // the player is standing somewhere a body can stand. What the bar
                // cannot say is *where* you are; this says it, and stops saying it the
                // moment you step out of the wall.
                Event::AbilityActivated { ability, .. } if declares(ability, Effect::Phase) => {
                    self.light_mark(ability, MarkPlace::PhasedPlayer, MarkLife::Standing)
                }
                // A control transfer (§8.1/#273) is lit once, at the launch, and then
                // left to blink with [`piloting`](State::piloting): the placement is a
                // live read of who holds the keys, so handing them back darkens the mark
                // and taking them again relights it with nothing to relight. Keyed on
                // the launch's own event rather than on the ability's identity, so a
                // second control ability joins the layer for free.
                Event::ControlTaken { .. } => {
                    if let Some(remote) = self.remote {
                        self.light_mark(
                            remote.source,
                            MarkPlace::PilotedRemote,
                            MarkLife::Standing,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    /// Light (or relight, at full life) `source`'s mark over `place` (§11.5/#338).
    ///
    /// At most one mark per (ability, placement, **lifetime**): refiring replaces the
    /// geometry and resets the life rather than stacking a second wash over the same
    /// board. The lifetime joins the key because one firing may legitimately want both
    /// kinds over the same placement — Lockdown says *this far* for a frame and *these
    /// doors* for its whole window, and both are cell marks (#242). Confusion's pair
    /// wants the same thing and merely got it for free, its two marks landing on
    /// different placements. Without the lifetime in the key the second call would
    /// silently overwrite the first, which is the bug this shape now cannot have.
    ///
    /// It still cannot stack without bound: the key is finite and small — an ability,
    /// two placements, two lifetimes. Called
    /// with the very geometry the effect resolved against, and after
    /// [`decay_effect_marks`](Self::decay_effect_marks) has already spent the older
    /// marks' turn — exactly as [`record_door_cues`](Self::record_door_cues) is — so a
    /// mark placed this turn keeps its full life.
    pub(super) fn light_mark(&mut self, source: AbilityId, place: MarkPlace, life: MarkLife) {
        let same = |mark: &EffectMark| {
            mark.source == source
                && std::mem::discriminant(&mark.place) == std::mem::discriminant(&place)
                && std::mem::discriminant(&mark.life) == std::mem::discriminant(&life)
        };
        if let Some(mark) = self.effect_marks.iter_mut().find(|mark| same(mark)) {
            mark.place = place;
            mark.life = life;
        } else {
            self.effect_marks.push(EffectMark {
                source,
                place,
                life,
            });
        }
    }

    /// Drop every mark `id` placed — its window is over (§8.2), whether by expiry or by
    /// an early toggle-off (§4.4), and the layer clears with it rather than fading over
    /// an effect that no longer exists.
    ///
    /// Inert for Confusion, which has no window to end (#325): its wash burns out on
    /// its own [`EFFECT_FLASH_TURNS`] clock and its standing mark ends with the last
    /// daze. It stays because an effect *with* a duration — Lockdown (#242), a live
    /// decoy (#340), concealment in force (#341) — is exactly what a standing mark is
    /// for, and a mark outliving its effect is the bug this closes.
    ///
    /// For the decoy it is the belt to [`MarkPlace::LiveDecoy`]'s braces: expiry and an
    /// early toggle-off both take the fake with them (§8.3), so the live read has
    /// already gone quiet by the time this sweeps the record.
    pub(super) fn clear_effect_marks(&mut self, id: AbilityId) {
        self.effect_marks.retain(|mark| mark.source != id);
    }

    /// Age the effect marks by one turn on the **one** decay schedule (#338). Runs once
    /// per **spent** turn, at the head of the world phases beside the door cues (§9.4)
    /// and before this turn's activation can light a fresh one — so a free action never
    /// burns a turn of a mark the player has not yet had a chance to read.
    ///
    /// A **momentary** mark counts down and is dropped at zero. A **standing** mark
    /// never counts: it is dropped when what it marks stops being held, which for
    /// [`MarkPlace::HeldGuards`] is the turn the last daze runs out and for
    /// [`MarkPlace::LiveDecoy`] the turn the fake dies. A standing mark over a fixed
    /// cell set outlives every clock here and ends only with its ability's window
    /// ([`clear_effect_marks`](Self::clear_effect_marks)).
    ///
    /// For the marks that ride a thing this is only **housekeeping**: both readings are
    /// live, so a thing that has gone stops being drawn the instant it goes, whether or
    /// not the record has been swept yet. Dropping the record here is what keeps the
    /// layer from carrying a mark that can never paint again.
    ///
    /// The two **conditional** placements — [`MarkPlace::ConcealedPlayer`] (#341) and
    /// [`MarkPlace::PhasedPlayer`] (#416) — are kept for their ability's whole
    /// **window**, not for the turns their condition holds. The distinction is
    /// load-bearing: each is *drawn* only while its rule is in force, but dropping the
    /// record on a turn the condition lapsed would delete a mark nothing ever relights,
    /// and the concealment (or the wall) the player walks back into next turn would go
    /// unreported.
    pub(super) fn decay_effect_marks(&mut self) {
        let any_held = self.guards.iter().any(|guard| guard.is_dazed());
        let decoy_alive = self.decoy.is_some();
        let cover_out = self.cover.is_some();
        let camouflaged = self.abilities.effect_active(Effect::ConcealWhileStill);
        let phasing = self.abilities.effect_active(Effect::Phase);
        // Kept for the ability's whole **window**, not for the turns anybody is flying —
        // the conditional placements' rule (see above): dropping the record on a turn the
        // player had let go would delete a mark nothing ever relights, and taking the
        // keys back would go unmarked.
        let remote_out = self.remote.is_some();
        self.effect_marks.retain_mut(|mark| match &mut mark.life {
            MarkLife::Momentary(ttl) => {
                *ttl -= 1;
                *ttl > 0
            }
            MarkLife::Standing => match mark.place {
                MarkPlace::HeldGuards => any_held,
                MarkPlace::LiveDecoy => decoy_alive,
                MarkPlace::DeployedCover => cover_out,
                MarkPlace::ConcealedPlayer => camouflaged,
                MarkPlace::PhasedPlayer => phasing,
                MarkPlace::PilotedRemote => remote_out,
                MarkPlace::Cells(_) => true,
            },
        });
    }
}
