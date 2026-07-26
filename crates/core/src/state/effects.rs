//! Area effects: the abilities whose reach is a **region**, and how the player reads
//! it (§8.3/§11.5, #308).
//!
//! An area effect is an active ability that acts on everything within a radius of the
//! player, measured by the §6.1 **box** metric and reaching **through walls** like the
//! guard sense (§9). Confusion is the first ([`Effect::Confuse`], §8.3); Lockdown
//! (#242) and any later radius tech join by adding a row to [`area_radius`] — the one
//! table this module, the mechanics and the renderer all read, so a new effect arrives
//! with its footprint already drawn.
//!
//! Two halves, mirroring the door cues (§9.4) they are modelled on:
//!
//! - **The area itself.** [`State::effect_area`] answers "where does this effect reach
//!   *right now*" as an [`EffectArea`] centred on the player's current cell. It is
//!   re-measured every frame because the bubble travels with the player (§8.3): step
//!   away and a guard thaws. Both the mechanics
//!   ([`guard_confused`](State::guard_confused)) and the painted footprint go through
//!   this one query, so the picture cannot disagree with the rule (§11.5: never a
//!   guess).
//! - **The flash.** The footprint is *shown* only for the first
//!   [`EFFECT_FLASH_TURNS`] turns of the window, latched when the ability fires
//!   ([`record_effect_flashes`](State::record_effect_flashes)) and fading turn by turn
//!   ([`decay_effect_flashes`](State::decay_effect_flashes)) — long enough to teach the
//!   extent, short enough that a 13×13 wash is not sitting over the board at the moment
//!   the danger overlay matters most. What holds the state for the rest of the window is
//!   the per-guard mark ([`guard_under_effect`](State::guard_under_effect)), which costs
//!   no ink at all.

use super::*;

/// The live footprint of one area effect (§8.3): the §6.1 **box** of its radius around
/// the cell it is centred on. A box, not a disc — [`Cell::sight_distance`] is the metric
/// the effects themselves are measured in, so a round footprint would be a picture that
/// disagreed with the rule.
///
/// Produced by [`State::effect_area`] and centred on the player's cell *at the moment
/// it is asked for*: the area is not a placed object, it is a query, which is exactly
/// how a bubble that travels with the player is kept honest.
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

    /// The cell the area is measured from (the player, while it is live).
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
/// Every consumer reads it: the freeze itself
/// ([`guard_confused`](State::guard_confused)), the painted footprint
/// ([`effect_footprint`](State::effect_footprint)), and the mark on what the effect
/// holds ([`guard_under_effect`](State::guard_under_effect)). Adding Lockdown's radius
/// (#242) here is all that ticket owes the render layer.
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

/// The area effects that hold **guards** — the ones a guard inside the footprint is
/// marked for (§11.2's [`Category::Effect`]). Confusion freezes; Lockdown (#242) will
/// seal doors instead and belongs in its own list, not this one, because what an effect
/// paints its mark on is what it *acts on*.
const GUARD_HOLDING: [Effect; 1] = [Effect::Confuse];

/// A live **effect flash** (§8.3/§11.5, #308): the footprint of a just-fired area
/// effect, and how many more turns it shows. The area itself is a live query
/// ([`State::effect_area`]) — this only carries *whether it is being drawn*, so a
/// flash that has burned out silences the picture without touching the mechanic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct EffectFlash {
    /// The effect whose footprint is lit.
    pub(super) effect: Effect,
    /// Turns of life left; decremented once per spent turn and dropped at zero.
    pub(super) ttl: u32,
}

impl State {
    /// Where `effect` reaches **right now**, or `None` when it is not running as an
    /// area effect (§8.3): the §6.1 box of its [`area_radius`] centred on the player's
    /// current cell.
    ///
    /// The single definition of an effect's reach. The mechanics ask it whether a guard
    /// is held ([`guard_confused`](Self::guard_confused)) and the renderer asks it which
    /// cells to paint ([`effect_footprint`](Self::effect_footprint)), so the footprint
    /// on screen is the rule itself rather than a drawing of it (§11.5).
    pub fn effect_area(&self, effect: Effect) -> Option<EffectArea> {
        let radius = area_radius(effect)?;
        self.abilities.effect_active(effect).then_some(EffectArea {
            centre: self.player,
            radius,
        })
    }

    /// The cells the §11.5 **effect layer** paints as a background: the in-bounds
    /// footprint of every area effect whose flash is still lit (#308).
    ///
    /// Only the flash's few turns, not the whole window — see the module note. The set
    /// is derived from [`effect_area`](Self::effect_area), so a footprint can never
    /// disagree with what the effect actually holds, and it follows the player while it
    /// lasts, which is how the travelling boundary gets taught at all.
    pub fn effect_footprint(&self) -> impl Iterator<Item = Cell> + '_ {
        let facility = self.layout.facility();
        self.effect_flashes
            .iter()
            .filter_map(|flash| self.effect_area(flash.effect))
            .flat_map(move |area| {
                let (cx, cy) = (area.centre().x, area.centre().y);
                let r = area.radius();
                let ys = cy.saturating_sub(r)..=(cy + r).min(facility.height().saturating_sub(1));
                let xs = cx.saturating_sub(r)..=(cx + r).min(facility.width().saturating_sub(1));
                ys.flat_map(move |y| xs.clone().map(move |x| Cell::new(x, y)))
            })
    }

    /// Whether `guard` is currently held by an area effect the player can read — the
    /// renderer's **mark** (§11.2's [`Category::Effect`], #308), keyed by the effect
    /// table rather than by any one ability, so Confusion's freeze and whatever radius
    /// tech lands next are one mark with one meaning.
    ///
    /// Gated on the guard being **perceived** ([`perceive_guard`](Self::perceive_guard)):
    /// the mark only ever recolours a guard the player is already shown — the seen `g`
    /// or the sensed dot — so it can never draw a guard the fog is hiding (§11.5a). That
    /// is normally no restriction at all, since [`CONFUSION_RADIUS`] is pinned inside
    /// [`PLAYER_SENSE_RANGE`] at compile time; it bites only where the *sense* shrinks
    /// below the bubble — inside a duct (§10.7) — and there the honest answer is
    /// silence, not a free reveal.
    pub fn guard_under_effect(&self, guard: &Guard) -> bool {
        self.perceive_guard(guard).is_some()
            && GUARD_HOLDING
                .iter()
                .filter_map(|&effect| self.effect_area(effect))
                .any(|area| area.contains(guard.pos()))
    }

    /// Light the footprint flash of every area effect the player just switched on,
    /// read off this turn's events (§8.3/#308) — the one-off "here is how far this
    /// reaches" the start-of-window message cannot say. Called after
    /// [`decay_effect_flashes`](Self::decay_effect_flashes) has already spent the older
    /// flashes' turn, exactly as [`record_door_cues`](Self::record_door_cues) is, so a
    /// flash placed this turn keeps its full life.
    pub(super) fn record_effect_flashes(&mut self, events: &[Event]) {
        for event in events {
            if let Event::AbilityActivated { ability } = *event {
                self.light_effect_flash(ability);
            }
        }
    }

    /// Light (or relight, at full life) the flash of every area effect `id` carries.
    /// An ability with no area effect lights nothing.
    fn light_effect_flash(&mut self, id: AbilityId) {
        for effect in AREA_EFFECTS
            .into_iter()
            .filter(|&effect| declares(id, effect))
        {
            if let Some(flash) = self.effect_flashes.iter_mut().find(|f| f.effect == effect) {
                flash.ttl = EFFECT_FLASH_TURNS;
            } else {
                self.effect_flashes.push(EffectFlash {
                    effect,
                    ttl: EFFECT_FLASH_TURNS,
                });
            }
        }
    }

    /// Drop the flash of every area effect `id` carries — the window is over (§8.2),
    /// whether by expiry or by an early toggle-off (§4.4), and the layer clears with it
    /// rather than fading over a bubble that no longer exists.
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

/// Every effect with a footprint, for the activation hook to scan. Kept beside
/// [`area_radius`] — its rows are exactly the `Some` arms — so one edit adds an effect
/// to the table and to the flash together.
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

    /// Switch Confusion on, spending the turn (§4.4).
    fn activate(state: &mut State) {
        state.step(Input::Activate(AbilityId::Confusion));
        assert!(
            state.effect_area(Effect::Confuse).is_some(),
            "Confusion is running"
        );
    }

    /// Every effect [`AREA_EFFECTS`] lists has a radius, and every effect with a radius
    /// is listed: the flash hook scans the array while the mechanics read the table, so
    /// a new area effect that updated only one of them would light no footprint (or
    /// light one it cannot measure).
    #[test]
    fn the_area_effect_table_and_list_agree() {
        for effect in AREA_EFFECTS {
            assert!(
                area_radius(effect).is_some(),
                "{effect:?} is listed as an area effect but has no radius"
            );
        }
        for effect in GUARD_HOLDING {
            assert!(
                AREA_EFFECTS.contains(&effect),
                "{effect:?} holds guards but is not an area effect"
            );
        }
    }

    /// The bubble's own numbers, pinned so a later change is a visible edit (§8.3
    /// **[START]**): the radius the footprint draws and the window the marks outlast
    /// the flash by.
    #[test]
    fn the_confusion_numbers_are_pinned() {
        assert_eq!(CONFUSION_RADIUS, 6);
        assert_eq!(confusion_duration(), 6);
    }

    /// Confusion's window, read off its own definition rather than restated here.
    fn confusion_duration() -> u32 {
        AbilityId::Confusion
            .def()
            .economy()
            .expect("Confusion is an activated ability")
            .duration()
    }

    /// The footprint is the §6.1 **box** of [`CONFUSION_RADIUS`] around the player —
    /// asserted against the rule, not against a hand-drawn shape: every painted cell is
    /// one [`EffectArea::contains`] accepts, and every in-bounds cell it accepts is
    /// painted. This is the criterion that stops the picture and the mechanic drifting.
    #[test]
    fn the_footprint_is_exactly_the_rule_s_box() {
        let mut s = level_with(Vec::new());
        activate(&mut s);
        let area = s
            .effect_area(Effect::Confuse)
            .expect("Confusion is running");
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
            s.player().x + CONFUSION_RADIUS,
            s.player().y + CONFUSION_RADIUS,
        );
        assert!(painted.contains(&corner), "the diagonal corner is inside");
    }

    /// The footprint travels with the player and is re-measured every turn (§8.3): a
    /// step west drags the whole box west, so the picture never promises reach the
    /// bubble has already left behind.
    #[test]
    fn the_footprint_follows_the_player() {
        let mut s = level_with(Vec::new());
        activate(&mut s);
        let before = s.player();
        let west_edge = before.x - CONFUSION_RADIUS;
        assert!(s
            .effect_footprint()
            .any(|c| c.x == west_edge && c.y == before.y));

        s.step(Input::Step(Direction::West));
        assert_eq!(s.player().x, before.x - 1, "the step landed");
        assert!(
            s.effect_footprint().all(|c| c.x >= west_edge - 1),
            "the box moved with the player"
        );
        assert!(
            !s.effect_footprint()
                .any(|c| c.x == before.x + CONFUSION_RADIUS),
            "the cell the bubble left is no longer painted"
        );
    }

    /// The flash is a *few* turns, not the whole window (§11.5): it shows for
    /// [`EFFECT_FLASH_TURNS`] renders and is gone on the next, while the ability is
    /// still very much running — the marks carry the rest.
    #[test]
    fn the_flash_fades_long_before_the_window_ends() {
        assert_eq!(EFFECT_FLASH_TURNS, 3, "the [START] flash life is pinned");
        let mut s = level_with(Vec::new());
        activate(&mut s);
        // The activation frame counts: it is the first render the player reads, and
        // the fade runs at the head of the *next* turn, so the footprint shows for
        // exactly `EFFECT_FLASH_TURNS` renders.
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
            s.effect_area(Effect::Confuse).is_some(),
            "…while the window itself is still open"
        );
    }

    /// An early toggle-off (§4.4) clears the layer on the spot — no footprint fading
    /// over a bubble that has already gone.
    #[test]
    fn a_toggle_off_clears_the_flash_at_once() {
        let mut s = level_with(Vec::new());
        activate(&mut s);
        assert!(s.effect_footprint().next().is_some());

        s.step(Input::Deactivate(AbilityId::Confusion));
        assert!(s.effect_footprint().next().is_none(), "layer cleared");
        assert!(s.effect_area(Effect::Confuse).is_none(), "window closed");
    }

    /// The window's own expiry leaves no residue either — belt and braces, since the
    /// flash is much shorter than the duration, but a longer-lived flash (or a shorter
    /// ability) must not outlive its effect.
    #[test]
    fn expiry_leaves_no_residue() {
        let mut s = level_with(Vec::new());
        activate(&mut s);
        for _ in 0..confusion_duration() + 1 {
            s.step(Input::Wait);
        }
        assert!(s.effect_area(Effect::Confuse).is_none(), "window over");
        assert!(s.effect_footprint().next().is_none(), "and nothing painted");
    }

    /// The mark is exactly the freeze, for a guard the player can see *and* for one
    /// felt only through a wall (§9.2) — the common case, since the bubble reaches
    /// through walls.
    #[test]
    fn every_frozen_guard_the_player_perceives_is_marked() {
        let mut s = level_with(Vec::new());
        activate(&mut s);
        for guard in s.guards() {
            assert_eq!(
                s.guard_under_effect(guard),
                s.guard_confused(guard) && s.perceive_guard(guard).is_some(),
                "the mark is the freeze, on a guard that is already drawn"
            );
        }
    }
}
