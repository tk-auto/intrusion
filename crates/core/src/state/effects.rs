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
//!   decoy still standing).
//! - **How long it lives** ([`MarkLife`]) — **momentary** where the effect *is* a
//!   moment (a bore, a blast's reach: [`EFFECT_FLASH_TURNS`], or as long as the moment's
//!   consequence runs — an eject is lit on just the frames its stun holds the player
//!   down), or **standing** where the effect is a state (a guard still frozen, a live
//!   decoy, and later concealment in force). One decay schedule serves both
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
//! the thing is.
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

use super::*;

/// The footprint one area effect **fired** with (§8.3/#325): the §6.1 **box** of its
/// radius around the cell it went off in. A box, not a disc — [`Cell::sight_distance`]
/// is the metric the effects themselves are measured in, so a round footprint would be
/// a picture that disagreed with the rule.
///
/// Decided once, at the firing seam ([`State::confusion_blast`]), and then fixed: the
/// player walking away narrows nothing and widens nothing, because the set of guards
/// it caught was settled the moment it went off.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EffectArea {
    centre: Cell,
    radius: u32,
}

impl EffectArea {
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
fn area_radius(effect: Effect) -> Option<u32> {
    match effect {
        Effect::Confuse => Some(CONFUSION_RADIUS),
        Effect::SealDoors => Some(LOCKDOWN_RADIUS),
        // Everything else acts on the player themselves, not on a region around them.
        Effect::ExtraStep
        | Effect::ConcealWhileStill
        | Effect::SpawnDecoy
        | Effect::Phase
        | Effect::AutoDoors
        | Effect::EnhancedSight => None,
    }
}

/// Where an effect mark lands (§11.5/#338) — one of the two shapes the layer speaks.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum MarkPlace {
    /// An explicit set of cells, fixed when the mark was lit: a blast's footprint, the
    /// cell a bore opened, the pair an eject threw you between. Nothing about it is a
    /// live query, so it stays where it happened rather than following the player.
    Cells(Vec<Cell>),
    /// The guards an effect currently **holds** — the mark rides each one wherever it
    /// walks, for as long as it is held, and is gated on perception (§11.5a).
    HeldGuards,
    /// The **live decoy** an effect is running (§8.3/#340) — the mark sits on the fake
    /// for exactly as long as it exists, and on nothing at all once it does not.
    ///
    /// Read live from [`State::decoy`] rather than latched as a cell, which is what
    /// makes "no mark outlives the decoy" a property of the shape instead of a clear
    /// call to remember: a decoy stomped in the guard phase — after this turn's decay
    /// has already run — stops being drawn on the very frame it dies.
    LiveDecoy,
}

/// How long an effect mark lives (§11.5/#338). Both arms run on the one decay schedule
/// ([`State::decay_effect_marks`]); neither carries a clock of its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
#[derive(Clone, PartialEq, Eq, Debug)]
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
    /// stays the catalog's **[START]** cap, so no change to the sense — a Wait's widened
    /// 20, a future modifier, salvaged tech — can make Confusion reach further than its
    /// own row says. What it does do is keep #240's promise as a *rule* rather than as a
    /// coincidence of two constants: the blast never freezes what the player cannot
    /// sense. On open floor it is inert (`min(6, 10)` = 6); inside a duct
    /// ([`DUCT_SENSE_RANGE`] = 5, §10.7) it closes the hole where a crawling player
    /// would otherwise daze a guard at 6 they cannot perceive at all. That nerf is the
    /// point: degraded information is the crawlspace's whole cost.
    ///
    /// It reads [`sense_range`](Self::sense_range) *itself*, never a duct check or any
    /// other re-derivation, so whatever changes the sense later is picked up here for
    /// free and there is no second place to keep in step.
    pub fn confusion_blast(&self) -> EffectArea {
        // Pinned read moment (#325): §9.1's widened sense belongs to the Wait that
        // bought it, and firing is not that Wait, so the flag is already down by the
        // time this is asked (see `Input::Activate`). The cap absorbs a stale one
        // today — `min(6, 20)` is 6 — but the order is what stops a later change to
        // the cap quietly resurrecting a blast widened by last turn's Wait.
        debug_assert!(
            !self.waited,
            "the blast's reach is read after the Wait that widened the sense is spent"
        );
        EffectArea {
            centre: self.player,
            radius: area_radius(Effect::Confuse)
                .expect("Confusion is an area effect")
                .min(self.sense_range()),
        }
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
    pub fn effect_cell_marks(&self) -> impl Iterator<Item = Cell> + '_ {
        self.effect_marks
            .iter()
            .flat_map(|mark| match &mark.place {
                MarkPlace::Cells(cells) => cells.as_slice(),
                MarkPlace::HeldGuards | MarkPlace::LiveDecoy => NO_CELLS,
            })
            .copied()
    }

    /// The cells where a mark rides a **thing** (#338/#340): the position of every guard
    /// an effect currently holds and the player can perceive, and the cell of a live
    /// decoy.
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
    pub fn effect_thing_marks(&self) -> impl Iterator<Item = Cell> + '_ {
        let riding = |place: MarkPlace| self.effect_marks.iter().any(|mark| mark.place == place);
        let held = riding(MarkPlace::HeldGuards);
        let decoyed = riding(MarkPlace::LiveDecoy);
        self.guards
            .iter()
            .filter(move |guard| held && self.guard_under_effect(guard))
            .map(|guard| guard.pos())
            .chain(self.decoy.filter(|_| decoyed))
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
                Event::AbilityActivated { ability, .. }
                    if declares(ability, Effect::SpawnDecoy) =>
                {
                    self.light_mark(ability, MarkPlace::LiveDecoy, MarkLife::Standing)
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
    fn light_mark(&mut self, source: AbilityId, place: MarkPlace, life: MarkLife) {
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
    pub(super) fn decay_effect_marks(&mut self) {
        let any_held = self.guards.iter().any(|guard| guard.is_dazed());
        let decoy_alive = self.decoy.is_some();
        self.effect_marks.retain_mut(|mark| match &mut mark.life {
            MarkLife::Momentary(ttl) => {
                *ttl -= 1;
                *ttl > 0
            }
            MarkLife::Standing => match mark.place {
                MarkPlace::HeldGuards => any_held,
                MarkPlace::LiveDecoy => decoy_alive,
                MarkPlace::Cells(_) => true,
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::Loadout;
    use crate::guard::Guard;
    use crate::test_support::open_room;

    /// A player at (20, 20) of a 40×40 room facing north, carrying Confusion, with
    /// `guards` posted around them. The bare world the footprint tests need: room
    /// enough that the whole `CONFUSION_RADIUS` box is in bounds on every side, so a
    /// clipped edge never masquerades as a footprint rule.
    fn level_with(guards: Vec<Guard>) -> State {
        State::new(
            open_room(40, 40),
            Cell::new(20, 20),
            Direction::North,
            guards,
            Vec::new(),
            Cell::new(38, 38),
        )
        .with_loadout(Loadout::innate().with(AbilityId::Confusion))
    }

    /// The same world with one guard inside the blast — the firing tests all need one,
    /// since a blast that would catch nobody is refused (§8.3/#325).
    fn level_with_a_target() -> State {
        level_with(vec![Guard::stationary(Cell::new(22, 20))])
    }

    /// A player at (10, 10) of a 40×40 room facing south with a decoy already out at
    /// (10, 11) — the world the #340 tests need, with room to walk away from the fake
    /// in every direction and no guard to stomp it by accident.
    fn level_with_a_live_decoy() -> State {
        let mut s = State::new(
            open_room(40, 40),
            Cell::new(10, 10),
            Direction::South,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 38),
        )
        .with_loadout(Loadout::innate().with(AbilityId::Decoy));
        s.step(Input::Activate(AbilityId::Decoy));
        assert_eq!(
            s.decoy(),
            Some(Cell::new(10, 11)),
            "precondition: the fake is out, on the faced cell"
        );
        s
    }

    /// Fire Confusion, spending the turn (§4.4).
    fn fire(state: &mut State) {
        let events = state.step(Input::Activate(AbilityId::Confusion));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ConfusionFired { .. })),
            "the blast went off"
        );
    }

    /// The blast fired by the last `step`, straight off the event — the object the daze
    /// was computed from, which is what the mark is asserted against.
    fn last_blast(state: &State) -> EffectArea {
        state
            .last_events()
            .iter()
            .find_map(|e| match e {
                Event::ConfusionFired { blast, .. } => Some(*blast),
                _ => None,
            })
            .expect("the blast went off")
    }

    /// [`area_radius`] holds each effect's **own reach** and nothing else's (§8.3): the
    /// two that act on a region around the player, and no row for the rest. The layer's
    /// geometry no longer reads the table at all — it is read at each firing seam
    /// ([`confusion_blast`](State::confusion_blast),
    /// [`lockdown_doors`](State::lockdown_doors)) — so this is pinned to keep a new
    /// radius tech a visible edit here rather than a silent one at the seam.
    #[test]
    fn only_the_area_effects_declare_a_radius() {
        for effect in [
            Effect::Confuse,
            Effect::SealDoors,
            Effect::ExtraStep,
            Effect::ConcealWhileStill,
            Effect::SpawnDecoy,
            Effect::Phase,
            Effect::AutoDoors,
            Effect::EnhancedSight,
        ] {
            assert_eq!(
                area_radius(effect).is_some(),
                matches!(effect, Effect::Confuse | Effect::SealDoors),
                "{effect:?}: only an effect that acts on a region has a radius",
            );
        }
    }

    /// The blast's own numbers, pinned so a later change is a visible edit (§8.3
    /// **[START]**): the reach it fires with and how long what it catches stays frozen.
    /// The ability itself is **instant** — the time it buys lives on the guards, so
    /// there is no player-side window at all.
    #[test]
    fn the_confusion_numbers_are_pinned() {
        assert_eq!(CONFUSION_RADIUS, 6);
        assert_eq!(CONFUSION_DAZE_TURNS, 6);
        assert_eq!(
            AbilityId::Confusion
                .def()
                .economy()
                .expect("Confusion is an activated ability")
                .duration(),
            0,
            "instant: fired, not carried"
        );
    }

    /// The wash is the §6.1 **box** of [`CONFUSION_RADIUS`] around the cell it fired
    /// from — asserted against the rule, not against a hand-drawn shape: every painted
    /// cell is one [`EffectArea::contains`] accepts, and every in-bounds cell it accepts
    /// is painted. This is the criterion that stops the picture and the mechanic
    /// drifting.
    #[test]
    fn the_cell_mark_is_exactly_the_rule_s_box() {
        let mut s = level_with_a_target();
        let fired_from = s.player();
        fire(&mut s);
        let area = last_blast(&s);
        let painted: Vec<Cell> = s.effect_cell_marks().collect();

        let facility = s.layout().facility();
        for y in 0..facility.height() {
            for x in 0..facility.width() {
                let cell = Cell::new(x, y);
                assert_eq!(
                    painted.contains(&cell),
                    area.contains(cell),
                    "{cell:?}: painted and in-the-box must agree"
                );
            }
        }
        // …and it is a box, not a disc: the corner of the square is in.
        let corner = Cell::new(
            fired_from.x + CONFUSION_RADIUS,
            fired_from.y + CONFUSION_RADIUS,
        );
        assert!(painted.contains(&corner), "the diagonal corner is inside");
    }

    /// The wash the renderer paints **is** the object the daze was computed from
    /// (#308/#324): the fired area rides the event and is turned into the mark's cell
    /// set there and then, so the picture and the rule are one value rather than two
    /// derivations that happen to agree.
    #[test]
    fn the_painted_cells_are_the_fired_blast() {
        let mut s = level_with_a_target();
        fire(&mut s);
        let painted: Vec<Cell> = s.effect_cell_marks().collect();
        assert_eq!(
            painted,
            last_blast(&s).cells(s.layout().facility()),
            "the lit cells are the blast that fired"
        );
    }

    /// The wash stays where it fired (§8.3/#325): a step west leaves the box behind,
    /// because the blast is a thing that happened at a place, not a bubble the player
    /// carries.
    #[test]
    fn the_cell_mark_does_not_follow_the_player() {
        let mut s = level_with_a_target();
        let fired_from = s.player();
        fire(&mut s);
        let painted: Vec<Cell> = s.effect_cell_marks().collect();
        assert!(painted.contains(&Cell::new(fired_from.x + CONFUSION_RADIUS, fired_from.y)));

        // The wash outlives the step only if `EFFECT_FLASH_TURNS` is raised; what is
        // asserted here is that while it *is* lit, it does not move. At the [START]
        // life of one turn the step burns it out, which is itself the check that the
        // box never reappears somewhere new.
        s.step(Input::Step(Direction::West));
        assert_eq!(s.player().x, fired_from.x - 1, "the step landed");
        assert!(
            s.effect_cell_marks()
                .all(|c| c.x <= fired_from.x + CONFUSION_RADIUS),
            "nothing is painted east of where the blast reached"
        );
    }

    /// A **momentary** mark is a flash (§11.5): it shows for [`EFFECT_FLASH_TURNS`]
    /// renders — one, the firing frame — and is gone on the next, while the **standing**
    /// mark on the guards it caught is still very much lit. The two lifetimes, on one
    /// firing, doing exactly the different jobs they are for.
    #[test]
    fn the_momentary_mark_fades_while_the_standing_one_holds() {
        assert_eq!(EFFECT_FLASH_TURNS, 1, "the [START] flash life is pinned");
        let mut s = level_with_a_target();
        fire(&mut s);
        // The firing frame counts: it is the first render the player reads, and the
        // fade runs at the head of the *next* turn, so the wash shows for exactly
        // `EFFECT_FLASH_TURNS` renders.
        for turn in 0..EFFECT_FLASH_TURNS {
            assert!(
                s.effect_cell_marks().next().is_some(),
                "the wash is still lit on render {turn}"
            );
            s.step(Input::Wait);
        }
        assert!(
            s.effect_cell_marks().next().is_none(),
            "the wash has burned out"
        );
        assert!(
            s.effect_thing_marks().next().is_some(),
            "…while the daze it dealt out is still marked"
        );
    }

    /// A **standing** mark ends with the state it reports and leaves no residue: the
    /// turn the last daze runs out, the mark is gone from the layer — not merely
    /// yielding nothing, but dropped, so a mark can never outlive its effect.
    #[test]
    fn the_standing_mark_ends_with_the_daze() {
        let mut s = level_with_a_target();
        fire(&mut s);
        for _ in 0..CONFUSION_DAZE_TURNS {
            s.step(Input::Wait);
        }
        assert!(
            s.guards().iter().all(|g| !s.guard_confused(g)),
            "precondition: the daze has run out"
        );
        assert!(
            s.effect_thing_marks().next().is_none(),
            "the mark went with it"
        );
        assert!(s.effect_marks.is_empty(), "…and left nothing behind");
    }

    /// The thing mark is exactly the daze, for a guard the player can see *and* for one
    /// felt only through a wall (§9.2) — the common case, since the blast reaches
    /// through walls — and never for one the fog is hiding (§11.5a).
    #[test]
    fn every_dazed_guard_the_player_perceives_is_marked() {
        let mut s = level_with_a_target();
        fire(&mut s);
        let marked: Vec<Cell> = s.effect_thing_marks().collect();
        for guard in s.guards() {
            assert_eq!(
                marked.contains(&guard.pos()),
                s.guard_confused(guard) && s.perceive_guard(guard).is_some(),
                "the mark is the daze, on a guard that is already drawn"
            );
        }
    }

    /// Pierce Wall is the first fixed-cell reader (#303/#338): boring a wall lights a
    /// **momentary** mark on the cell it opened, and nothing else on the board.
    #[test]
    fn a_bore_marks_the_cell_it_opened() {
        let mut layout = open_room(20, 20);
        let wall = Cell::new(10, 9);
        layout.place(wall, Terrain::Wall);
        let mut s = State::new(
            layout,
            Cell::new(10, 10),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(18, 18),
        )
        .with_loadout(Loadout::innate().with(AbilityId::PierceWall));

        let events = s.step(Input::Activate(AbilityId::PierceWall));
        assert!(
            events.contains(&Event::WallBored { at: wall }),
            "precondition: the bore went through: {events:?}",
        );
        assert_eq!(
            s.effect_cell_marks().collect::<Vec<_>>(),
            vec![wall],
            "the opened cell, and only it",
        );
        assert!(
            s.effect_thing_marks().next().is_none(),
            "a bore holds nothing",
        );
        // Momentary: gone on the very next turn, like the blast's wash.
        s.step(Input::Wait);
        assert!(
            s.effect_cell_marks().next().is_none(),
            "the bore mark is a moment, not a monument",
        );
    }

    /// A **refusal** lights nothing (§11.7/#338): it is a message, not an effect, so
    /// the wall Pierce Wall declined to open is never washed as though it had been.
    #[test]
    fn a_refused_bore_marks_nothing() {
        // Standing in the open with no adjacent wall: `BoreRefusal::NothingToBore`.
        let mut s = State::new(
            open_room(20, 20),
            Cell::new(10, 10),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(18, 18),
        )
        .with_loadout(Loadout::innate().with(AbilityId::PierceWall));

        let events = s.step(Input::Activate(AbilityId::PierceWall));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::BoreRefused { .. })),
            "precondition: the bore was refused: {events:?}",
        );
        assert!(
            s.effect_cell_marks().next().is_none(),
            "a refusal paints nothing",
        );
    }

    /// A 12×12 room with a wall at `(5,4)` and the player one cell west of it holding
    /// Dephase, seeded so the landing is reproducible (§12.4). Phasing east and waiting
    /// out the duration strands them inside the wall and fires the safety eject.
    fn phased_into_a_wall() -> State {
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(5, 4), Terrain::Wall);
        let mut s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(10, 10),
        )
        .with_loadout(Loadout::innate().with(AbilityId::Dephase))
        .with_rng(crate::Rng::new(7));
        s.step(Input::Activate(AbilityId::Dephase));
        s.step(Input::Step(Direction::East));
        assert_eq!(
            s.player(),
            Cell::new(5, 4),
            "precondition: inside the solid"
        );
        s
    }

    /// The two ends of the throw the last `step` reported, straight off the event — the
    /// pair the stun was priced from, which is what the mark is asserted against.
    fn last_throw(state: &State) -> (Cell, Cell) {
        state
            .last_events()
            .iter()
            .find_map(|e| match e {
                Event::Ejected { from, to, .. } => Some((*from, *to)),
                _ => None,
            })
            .expect("the eject fired")
    }

    /// §8.3/#329/#339: the safety eject lights a **momentary** mark on both of its ends
    /// — the solid it stranded you in and the cell it threw you onto — and on nothing
    /// else. Two marks for one event, because the span between them is the fact the
    /// stunned player is being told.
    #[test]
    fn an_eject_marks_both_ends_of_the_throw() {
        let mut s = phased_into_a_wall();
        s.step(Input::Wait); // the duration ends inside the wall
        let (from, to) = last_throw(&s);
        assert_eq!(
            from,
            Cell::new(5, 4),
            "precondition: thrown out of the wall"
        );
        assert_eq!(s.player(), to, "precondition: standing where it put them");

        let mut painted: Vec<Cell> = s.effect_cell_marks().collect();
        painted.sort_by_key(|c| (c.y, c.x));
        let mut both = vec![from, to];
        both.sort_by_key(|c| (c.y, c.x));
        assert_eq!(painted, both, "both ends, and only them");
        assert!(
            s.effect_thing_marks().next().is_none(),
            "an eject holds nothing",
        );
    }

    /// The origin end is a **solid** and is washed anyway (§11.5a/#339): the layer paints
    /// over whatever geometry it finds, and a cell the player occupied a moment ago is
    /// their own knowledge rather than a reveal. Gating the mark on walkability would
    /// silently drop the half of the throw that explains it.
    #[test]
    fn the_origin_mark_draws_even_though_it_is_solid() {
        let mut s = phased_into_a_wall();
        s.step(Input::Wait);
        let (from, _) = last_throw(&s);
        assert!(
            !s.layout().facility().can_enter(from, ACTOR_FILL),
            "precondition: {from:?} is a solid no body can stand in",
        );
        assert!(
            s.effect_cell_marks().any(|c| c == from),
            "the solid end is marked all the same",
        );
    }

    /// The pair is lit on **exactly the frames the player cannot act from** (#339) —
    /// stated as the invariant rather than as a count, since the two ways to get this
    /// wrong are opposite and both are bugs: go out too early and the cue expires while
    /// its reader is still held down; stay one frame too long and it is still reporting
    /// the throw on the frame they are choosing a real move from.
    ///
    /// `stunned() > 0` is precisely "the next press will be eaten", so asserting the
    /// mark against it — rather than against a number — leaves nothing for a later
    /// repricing of the stun (§8.3 **[START]**) to knock out of step.
    #[test]
    fn the_eject_marks_are_lit_exactly_while_the_player_cannot_act() {
        let mut s = phased_into_a_wall();
        s.step(Input::Wait);
        let stun = s.stunned();
        assert!(stun > 0, "precondition: the throw cost some helplessness");
        let landed = s.player();

        let mut helpless_frames = 0;
        while s.stunned() > 0 {
            assert_eq!(
                s.effect_cell_marks().count(),
                2,
                "both ends are lit on a frame the player cannot act from",
            );
            assert_eq!(
                s.player(),
                landed,
                "a stunned player cannot move off the mark"
            );
            helpless_frames += 1;
            s.step(Input::Wait);
        }
        assert_eq!(
            helpless_frames, stun,
            "precondition: one wasted press per turn of stun",
        );
        assert!(
            s.effect_cell_marks().next().is_none(),
            "the first frame whose press is answered is already dark",
        );
    }

    /// §8.3: the entombment — nowhere in the facility to be thrown clear to — marks
    /// **one** cell, the one that took you. The run ends on this frame, so saying where
    /// is the last thing the board has to do.
    #[test]
    fn an_entombment_marks_the_one_cell_that_took_you() {
        let mut f = crate::facility::Facility::walled_box(9, 9);
        for y in 0..9 {
            for x in 0..9 {
                f.set_terrain(x, y, Terrain::Wall);
            }
        }
        let entombing = Cell::new(4, 4);
        let mut s = State::new(
            crate::Layout::from_facility(f),
            entombing,
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(7, 7),
        )
        .with_loadout(Loadout::innate().with(AbilityId::Dephase));

        s.step(Input::Activate(AbilityId::Dephase));
        for _ in 0..4 {
            if s.last_events().contains(&Event::Entombed { at: entombing }) {
                break;
            }
            s.step(Input::Wait);
        }
        assert_eq!(
            s.outcome(),
            Outcome::Lost,
            "precondition: the wall took them"
        );
        assert_eq!(
            s.effect_cell_marks().collect::<Vec<_>>(),
            vec![entombing],
            "the entombing cell, and only it",
        );
    }

    /// §8.3/#340: a live decoy wears the mark on the **thing**, not a wash on the
    /// board. The fake is what is running, so the mark rides it and claims no geometry
    /// around it.
    #[test]
    fn a_live_decoy_wears_a_mark_on_the_thing() {
        let s = level_with_a_live_decoy();
        assert_eq!(
            s.effect_thing_marks().collect::<Vec<_>>(),
            vec![s.decoy().expect("the fake is out")],
            "the fake, and nothing else",
        );
        assert!(
            s.effect_cell_marks().next().is_none(),
            "a decoy washes no cells: it is a thing, not a footprint",
        );
    }

    /// §8.3/#340: the mark is **standing**, so it lasts the whole of the decoy's life
    /// rather than flashing — and it ends with the window that placed it, leaving
    /// nothing behind. The two halves of "for as long as it lives", asserted against
    /// the ability's own duration rather than a hand-copied number.
    #[test]
    fn the_decoy_mark_lasts_the_window_and_dies_with_it() {
        let mut s = level_with_a_live_decoy();
        let duration = AbilityId::Decoy
            .def()
            .economy()
            .expect("Decoy is an activated ability")
            .duration();
        assert!(
            duration > EFFECT_FLASH_TURNS,
            "precondition: a standing mark is only distinguishable past the flash life",
        );
        // The activation turn is the first of the window (§8.2) and the clock is ticked
        // at the **end** of each turn, so the fake is still out after every wait up to
        // the window's last turn, and gone the moment that one is spent.
        for turn in 2..duration {
            s.step(Input::Wait);
            assert!(
                s.decoy().is_some(),
                "precondition: the fake is still alive on turn {turn}",
            );
            assert_eq!(
                s.effect_thing_marks().count(),
                1,
                "still marked on turn {turn}",
            );
        }
        s.step(Input::Wait);
        assert!(s.decoy().is_none(), "the window ran out and took the fake");
        assert!(
            s.effect_thing_marks().next().is_none(),
            "the mark went with it",
        );
        assert!(s.effect_marks.is_empty(), "…and left nothing behind");
    }

    /// §11.5a's second exception (#321/#326/#340): the mark follows the glyph it sits
    /// under, and the decoy's glyph is drawn out of the FOV. A fake you have walked
    /// away from is the whole point of the ability, so a mark that needed line of sight
    /// would be a mark the ability cannot use.
    #[test]
    fn the_decoy_mark_is_drawn_out_of_view() {
        let mut s = level_with_a_live_decoy();
        let decoy = s.decoy().expect("the fake is out");
        while s.player_fov().contains(decoy) {
            s.step(Input::Step(Direction::North));
        }
        assert_eq!(
            s.decoy(),
            Some(decoy),
            "precondition: nothing stepped on it"
        );
        assert!(
            s.effect_thing_marks().any(|cell| cell == decoy),
            "the fake is still marked with the player's back to it",
        );
    }

    /// §8.3/#340: no mark outlives the decoy. A stomp kills the fake in the middle of a
    /// turn — after the layer's own decay has already run — and the mark is gone on
    /// that very frame, because the mark reads the decoy itself rather than a cell it
    /// once remembered.
    #[test]
    fn the_mark_dies_on_the_same_turn_the_decoy_does() {
        let mut s = level_with_a_live_decoy();
        let decoy = s.decoy().expect("the fake is out");
        let events = s.step(Input::Step(Direction::South));
        assert!(
            events.contains(&Event::DecoyDied { at: decoy }),
            "precondition: the player stepped on their own fake: {events:?}",
        );
        assert!(
            s.effect_thing_marks().next().is_none(),
            "the mark went on the same turn",
        );
        // …and the record is swept on the next turn's decay, so the layer never carries
        // a mark that can no longer paint.
        s.step(Input::Wait);
        assert!(s.effect_marks.is_empty(), "the record went too");
    }

    /// §4.4/§8.3 (#340): an early toggle-off is free and takes the fake with it, so the
    /// mark is cleared outright rather than merely falling silent.
    #[test]
    fn the_decoy_mark_goes_with_an_early_toggle_off() {
        let mut s = level_with_a_live_decoy();
        s.step(Input::Deactivate(AbilityId::Decoy));
        assert!(s.decoy().is_none(), "precondition: the fake is gone");
        assert!(
            s.effect_thing_marks().next().is_none(),
            "and so is its mark"
        );
        assert!(s.effect_marks.is_empty(), "cleared, not merely quiet");
    }

    /// Refiring replaces a mark rather than stacking one: the layer holds at most one
    /// mark per (ability, placement), so a second blast cannot leave the first box on
    /// the board beside its own.
    #[test]
    fn refiring_replaces_the_mark_it_relights() {
        let mut s = level_with(vec![
            Guard::stationary(Cell::new(22, 20)),
            Guard::stationary(Cell::new(18, 20)),
        ]);
        fire(&mut s);
        let first: Vec<Cell> = s.effect_cell_marks().collect();
        assert_eq!(s.effect_marks.len(), 2, "one wash, one standing mark");

        // Walk clear of the first box and fire again — the ability's own cooldown is
        // waived by relighting the mark directly, which is what this test is about.
        let blast = EffectArea {
            centre: Cell::new(30, 30),
            radius: 2,
        };
        let cells = blast.cells(s.layout().facility());
        s.light_mark(
            AbilityId::Confusion,
            MarkPlace::Cells(cells.clone()),
            MarkLife::Momentary(EFFECT_FLASH_TURNS),
        );
        assert_eq!(s.effect_marks.len(), 2, "still one wash, not two");
        assert_eq!(s.effect_cell_marks().collect::<Vec<_>>(), cells);
        assert_ne!(first, cells, "precondition: a genuinely different box");
    }
}
