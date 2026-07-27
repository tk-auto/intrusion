//! Area effects: the abilities whose reach is a **region**, and how the player reads
//! it (§8.3/§11.5, #308).
//!
//! An area effect is an ability that acts on everything within a radius of the player,
//! measured by the §6.1 **box** metric and reaching **through walls** like the guard
//! sense (§9). Confusion is the first ([`Effect::Confuse`], §8.3); Lockdown (#242) and
//! any later radius tech join by adding a row to [`area_radius`] — the one table this
//! module, the mechanics and the renderer all read, so a new effect arrives with its
//! footprint already drawn.
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
//! So the two halves this module owns are both about **one firing**:
//!
//! - **The area.** [`EffectArea`] is the §6.1 box a blast covered, decided at the
//!   firing seam and then fixed. [`State::effect_area`] hands back the one most
//!   recently fired, while its flash is still lit.
//! - **The flash.** The footprint is *shown* for [`EFFECT_FLASH_TURNS`] turns — one,
//!   the firing frame — latched by [`light_effect_flash`](State::light_effect_flash)
//!   and fading turn by turn ([`decay_effect_flashes`](State::decay_effect_flashes)):
//!   enough to answer *how far* at the moment the player asks it, without leaving a
//!   13×13 field of background over the board while the danger overlay is the thing
//!   that matters. The decay machinery is kept general even at a life of one, so
//!   raising it is a one-number change and Lockdown (#242) may want a longer one.
//!
//! The flash **is** the fired area rather than a second drawing of it: the renderer
//! paints the very box the daze was computed from, so the picture cannot disagree with
//! the rule (§11.5: never a guess). What carries the state afterwards is the per-guard
//! mark ([`guard_under_effect`](State::guard_under_effect)), which costs no ink at all
//! — and which reads off the guard's own counter, so it stays truthful for a guard
//! that was caught and has since walked out of the box.
//!
//! # The seam #338 generalises
//!
//! The layer still speaks only one shape — a box, keyed by [`Effect`] — and #338 widens
//! it to a mark over an **explicit cell set**, keyed by *what happened* rather than by
//! which effect is running (which is the only way it can reach `Behaviour::Coded`
//! abilities like Pierce Wall, §8.1). Two of the three things that needs are already
//! here and should stay:
//!
//! - **The flash is latched from the turn's events**
//!   ([`record_effect_flashes`](State::record_effect_flashes)), not from "is this
//!   effect active". `Event::WallBored { at }` joins by adding an arm.
//! - **The footprint is stored, not re-derived.** Nothing about the drawn mark is a
//!   live query over the player's cell any more, so a mark that sits on a fixed cell
//!   (a bore, an eject) needs no new lifetime machinery — only a different geometry in
//!   [`EffectFlash`].
//!
//! What is left for that ticket is the geometry itself (`EffectArea` → cells) and the
//! standing-vs-momentary lifetime. [`area_radius`] is Confusion's own reach, not the
//! layer's: the drawing side no longer reads it at all.

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
}

/// The radius of `effect` when it acts on an **area** around the player, or `None` when
/// it does not — the one table that says which effects have a footprint at all (§8.3).
///
/// A **cap**, read at the firing seam and clamped there by whatever that effect's own
/// rule says (Confusion's is the guard sense, [`confusion_blast`](State::confusion_blast)).
/// Adding Lockdown's radius (#242) here is all that ticket owes the render layer; its
/// own clamp, if it wants one, is its own — the door sense is not the guard sense, and
/// nothing in this table presumes otherwise.
fn area_radius(effect: Effect) -> Option<u32> {
    match effect {
        Effect::Confuse => Some(CONFUSION_RADIUS),
        // Everything else acts on the player themselves, not on a region around them.
        Effect::ExtraStep
        | Effect::ConcealWhileStill
        | Effect::SpawnDecoy
        | Effect::Phase
        | Effect::AutoDoors
        | Effect::EnhancedSight => None,
    }
}

/// A live **effect flash** (§8.3/§11.5, #308/#325): the footprint a just-fired area
/// effect actually went off with, and how many more turns it shows.
///
/// It carries the [`EffectArea`] itself, not a recipe for re-deriving one: the box the
/// renderer paints is the same object the mechanic measured its set from, so the two
/// cannot drift even once the player has walked away from where they fired.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct EffectFlash {
    /// The effect whose footprint is lit.
    pub(super) effect: Effect,
    /// Where it went off, and how far it reached.
    pub(super) area: EffectArea,
    /// Turns of life left; decremented once per spent turn and dropped at zero.
    pub(super) ttl: u32,
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

    /// The footprint `effect` **last fired with**, while its flash is still lit, or
    /// `None` (§8.3/#325).
    ///
    /// Not a live query any more: since Confusion fires once, there is no ongoing area
    /// to ask about — only the box a blast went off in and the guards it marked. This
    /// is what the renderer paints ([`effect_footprint`](Self::effect_footprint)), and
    /// it is the very object the daze was computed from, so the picture is the rule
    /// rather than a drawing of it (§11.5). Whether a *guard* is still held is a
    /// different question with a different answer, asked of the guard
    /// ([`guard_confused`](Self::guard_confused)).
    pub fn effect_area(&self, effect: Effect) -> Option<EffectArea> {
        self.effect_flashes
            .iter()
            .find(|flash| flash.effect == effect)
            .map(|flash| flash.area)
    }

    /// The cells the §11.5 **effect layer** paints as a background: the in-bounds
    /// footprint of every area effect whose flash is still lit (#308).
    ///
    /// Only the flash's turn, not the whole daze — see the module note. Each set is the
    /// stored [`EffectArea`] of one firing, so a footprint can never disagree with what
    /// the blast actually caught, and it stays where it went off rather than following
    /// the player who fired it.
    pub fn effect_footprint(&self) -> impl Iterator<Item = Cell> + '_ {
        let facility = self.layout.facility();
        self.effect_flashes
            .iter()
            .map(|flash| flash.area)
            .flat_map(move |area| {
                let (cx, cy) = (area.centre().x, area.centre().y);
                let r = area.radius();
                let ys = cy.saturating_sub(r)..=(cy + r).min(facility.height().saturating_sub(1));
                let xs = cx.saturating_sub(r)..=(cx + r).min(facility.width().saturating_sub(1));
                ys.flat_map(move |y| xs.clone().map(move |x| Cell::new(x, y)))
            })
    }

    /// Whether `guard` is currently held by an area effect the player can read — the
    /// renderer's **mark** (§11.2's [`Category::Effect`], #308).
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

    /// Light the footprint flash of every area effect fired this turn, read off the
    /// turn's events (§8.3/#308/#325). Called after
    /// [`decay_effect_flashes`](Self::decay_effect_flashes) has already spent the older
    /// flashes' turn, exactly as [`record_door_cues`](Self::record_door_cues) is, so a
    /// flash placed this turn keeps its full life.
    ///
    /// The area comes off the event rather than being measured again from the player,
    /// who by now may have taken an extra step (§8.3's Run) since the blast went off:
    /// what is drawn is the object the daze was computed from, carried through by value.
    ///
    /// Keyed on **what happened**, not on which effect is running — which is the form
    /// #338 needs, because a `Behaviour::Coded` ability (Pierce Wall, §8.1) has no
    /// [`Effect`] to key on at all. A new mark is a new arm here.
    pub(super) fn record_effect_flashes(&mut self, events: &[Event]) {
        for event in events {
            if let Event::ConfusionFired { blast, .. } = *event {
                self.light_effect_flash(Effect::Confuse, blast);
            }
        }
    }

    /// Light (or relight, at full life) the flash of `effect`'s firing over `area`
    /// (§8.3/#308/#325) — the one-off "here is how far this reached" nothing else says.
    ///
    /// Called from the firing seam with the very area the effect resolved against, and
    /// after [`decay_effect_flashes`](Self::decay_effect_flashes) has already spent the
    /// older flashes' turn — exactly as [`record_door_cues`](Self::record_door_cues) is
    /// — so a flash placed this turn keeps its full life.
    fn light_effect_flash(&mut self, effect: Effect, area: EffectArea) {
        debug_assert!(
            AREA_EFFECTS.contains(&effect),
            "{effect:?} has no footprint to light"
        );
        if let Some(flash) = self.effect_flashes.iter_mut().find(|f| f.effect == effect) {
            flash.area = area;
            flash.ttl = EFFECT_FLASH_TURNS;
        } else {
            self.effect_flashes.push(EffectFlash {
                effect,
                area,
                ttl: EFFECT_FLASH_TURNS,
            });
        }
    }

    /// Drop the flash of every area effect `id` carries — its window is over (§8.2),
    /// whether by expiry or by an early toggle-off (§4.4), and the layer clears with it
    /// rather than fading over an area that no longer exists.
    ///
    /// Inert for Confusion, which has no window to end (#325): its flash simply burns
    /// out on its own [`EFFECT_FLASH_TURNS`] clock. It stays because an area effect
    /// *with* a duration is exactly what Lockdown (#242) is likely to be, and a footprint
    /// outliving its effect is the bug this closes.
    pub(super) fn clear_effect_flash(&mut self, id: AbilityId) {
        self.effect_flashes
            .retain(|flash| !declares(id, flash.effect));
    }

    /// Fade the effect flashes by one turn, dropping any that have burned out. Runs
    /// once per **spent** turn, at the head of the world phases beside the door cues
    /// (§9.4) and before this turn's activation can light a fresh one — so a free
    /// action never burns a turn of a flash the player has not yet had a chance to read.
    pub(super) fn decay_effect_flashes(&mut self) {
        self.effect_flashes.retain_mut(|flash| {
            flash.ttl -= 1;
            flash.ttl > 0
        });
    }
}

/// Every effect with a footprint. Kept beside [`area_radius`] — its rows are exactly
/// the `Some` arms — so one edit adds an effect to the table and to the flash together,
/// and a firing that lights a flash for an effect with no radius is caught in tests.
const AREA_EFFECTS: [Effect; 1] = [Effect::Confuse];

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

    /// Every effect [`AREA_EFFECTS`] lists has a radius, and every effect with a radius
    /// is listed: the flash hook asserts against the array while the blast reads the
    /// table, so a new area effect that updated only one of them would light no
    /// footprint (or light one it cannot measure).
    #[test]
    fn the_area_effect_table_and_list_agree() {
        for effect in AREA_EFFECTS {
            assert!(
                area_radius(effect).is_some(),
                "{effect:?} is listed as an area effect but has no radius"
            );
        }
        for effect in [
            Effect::ExtraStep,
            Effect::ConcealWhileStill,
            Effect::SpawnDecoy,
            Effect::Phase,
            Effect::AutoDoors,
            Effect::EnhancedSight,
        ] {
            assert_eq!(
                area_radius(effect).is_some(),
                AREA_EFFECTS.contains(&effect),
                "{effect:?}: having a radius and being listed must agree"
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

    /// The footprint is the §6.1 **box** of [`CONFUSION_RADIUS`] around the cell it
    /// fired from — asserted against the rule, not against a hand-drawn shape: every
    /// painted cell is one [`EffectArea::contains`] accepts, and every in-bounds cell it
    /// accepts is painted. This is the criterion that stops the picture and the mechanic
    /// drifting.
    #[test]
    fn the_footprint_is_exactly_the_rule_s_box() {
        let mut s = level_with_a_target();
        let fired_from = s.player();
        fire(&mut s);
        let area = s.effect_area(Effect::Confuse).expect("the blast is lit");
        let painted: Vec<Cell> = s.effect_footprint().collect();

        for &cell in &painted {
            assert!(
                area.contains(cell),
                "{cell:?} is painted but outside the rule's box"
            );
        }
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

    /// The footprint the renderer paints **is** the object the daze was computed from
    /// (#308/#324): the fired area rides the event, so the picture and the rule are one
    /// value rather than two derivations that happen to agree.
    #[test]
    fn the_painted_footprint_is_the_fired_blast() {
        let mut s = level_with_a_target();
        let events = s.step(Input::Activate(AbilityId::Confusion));
        let Some(Event::ConfusionFired { blast, .. }) = events
            .iter()
            .copied()
            .find(|e| matches!(e, Event::ConfusionFired { .. }))
        else {
            panic!("the blast went off");
        };
        assert_eq!(
            s.effect_area(Effect::Confuse),
            Some(blast),
            "the lit footprint is the blast that fired"
        );
    }

    /// The footprint stays where it fired (§8.3/#325): a step west leaves the box
    /// behind, because the blast is a thing that happened at a place, not a bubble the
    /// player carries.
    #[test]
    fn the_footprint_does_not_follow_the_player() {
        let mut s = level_with_a_target();
        let fired_from = s.player();
        fire(&mut s);
        let painted: Vec<Cell> = s.effect_footprint().collect();
        assert!(painted.contains(&Cell::new(fired_from.x + CONFUSION_RADIUS, fired_from.y)));

        // The flash outlives the step only if `EFFECT_FLASH_TURNS` is raised; what is
        // asserted here is that while it *is* lit, it does not move. At the [START]
        // life of one turn the step burns it out, which is itself the check that the
        // box never reappears somewhere new.
        s.step(Input::Step(Direction::West));
        assert_eq!(s.player().x, fired_from.x - 1, "the step landed");
        assert!(
            s.effect_footprint()
                .all(|c| c.x <= fired_from.x + CONFUSION_RADIUS),
            "nothing is painted east of where the blast reached"
        );
    }

    /// The flash is a *flash* (§11.5): it shows for [`EFFECT_FLASH_TURNS`] renders —
    /// one, the firing frame — and is gone on the next, while the guards it caught are
    /// still very much frozen. The marks carry the rest.
    #[test]
    fn the_flash_fades_long_before_the_daze_ends() {
        assert_eq!(EFFECT_FLASH_TURNS, 1, "the [START] flash life is pinned");
        let mut s = level_with_a_target();
        fire(&mut s);
        // The firing frame counts: it is the first render the player reads, and the
        // fade runs at the head of the *next* turn, so the footprint shows for exactly
        // `EFFECT_FLASH_TURNS` renders.
        for turn in 0..EFFECT_FLASH_TURNS {
            assert!(
                s.effect_footprint().next().is_some(),
                "the flash is still lit on render {turn}"
            );
            s.step(Input::Wait);
        }
        assert!(
            s.effect_footprint().next().is_none(),
            "the flash has burned out"
        );
        assert!(
            s.guards().iter().any(|g| s.guard_confused(g)),
            "…while the daze it dealt out is still running"
        );
    }

    /// The mark is exactly the daze, for a guard the player can see *and* for one felt
    /// only through a wall (§9.2) — the common case, since the blast reaches through
    /// walls.
    #[test]
    fn every_dazed_guard_the_player_perceives_is_marked() {
        let mut s = level_with_a_target();
        fire(&mut s);
        for guard in s.guards() {
            assert_eq!(
                s.guard_under_effect(guard),
                s.guard_confused(guard) && s.perceive_guard(guard).is_some(),
                "the mark is the daze, on a guard that is already drawn"
            );
        }
    }
}
