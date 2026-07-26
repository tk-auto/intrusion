//! The ability line/panel's display vocabulary (§11.4) — **what the always-on
//! ability line and the deployable panel say**, assembled from the run's real
//! ability economy.
//!
//! §11.4 leaves *where* ability state lives on screen **[OPEN]** (§15 Q9). The
//! current experiment is the always-on **compact line** plus an on-demand
//! **deployable panel** (`Tab`, or the deploy button) — both now driven by real
//! per-ability runtime, and both *actionable*: a click resolves to the ability
//! under it and activates it exactly as its hotkey would (§11.4, §11.6). This
//! module owns the *display* half: the four states an ability reads as
//! ([`AbilityState`]) and how each formats, plus one panel row ([`AbilityStatus`]).
//! The render composes and hit-tests them
//! ([`render_screen`](crate::render_screen), [`ability_at`](crate::ability_at)).
//!
//! # Two halves: the economy model and its display
//!
//! The per-ability **runtime economy** (§8.1/§8.2) lives in this module too —
//! [`AbilityId`] and its data-driven [`Ability`] catalog, the effect vocabulary
//! ([`Effect`]) and the code escape hatch ([`Behaviour`]), and the [`Deck`] the
//! turn loop steps: activation, early toggle-off, and the end-of-turn
//! duration/cooldown tick, with the `duration + cooldown` lockout *emergent* from
//! the rules rather than stored. The deck reads each ability's state as one of the
//! display [`AbilityState`]s ([`Deck::state`]) — the number the player actually
//! gets (§8.2 timing) — which is how the two halves meet.
//! [`State::ability_statuses`](crate::State::ability_statuses) builds the line and
//! panel straight from that live state, one row per economy ability.
//!
//! Two things are real and load-bearing across the display:
//!
//! - **Hotkeys come from [`ability_hotkey`](crate::input::ability_hotkey)**, the
//!   settled §11.6 identity→letter map — never from the panel's row order. A key
//!   is a fixed fact about an ability, so reordering or trimming the list can
//!   never move one (the §11.6 regression this repo already designed out); and a
//!   click resolves by that same identity, never by the row it lands on.
//! - **The number shown is the number the player gets** (§8.2 timing): the panel
//!   formats exactly the value it is handed and advertises nothing else, so it
//!   cannot re-introduce the old advertised-vs-real discrepancy.

use crate::input::ability_hotkey;

/// The runtime state of one ability, as the player reads it (§11.4): the cases the
/// panel must keep discoverable — ready, active, cooling, passive, unusable.
///
/// The numbers are turn counts under the §8.2 economy — a duration ticking down
/// while active, a cooldown draining once inactive — and [`AbilityState::suffix`]
/// renders them in the design's notation (`[N]` active, `/N/` cooling).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AbilityState {
    /// Available to use this turn.
    Ready,
    /// Switched on, with `remaining` turns of duration left — shown `[N]` (§11.4).
    Active { remaining: u32 },
    /// Recharging, with `remaining` turns of cooldown left — shown `/N/` (§11.4).
    Cooling { remaining: u32 },
    /// A **passive** ability (§8.2/#264): in effect for as long as it is held, with
    /// no activation, no duration and no cooldown — shown `(on)`.
    ///
    /// Its own state deliberately, rather than a permanent
    /// [`Active`](AbilityState::Active): the clock states all carry "and then it
    /// ends", and a passive never does. Stretching `Active` to mean always-on would
    /// make the number the panel shows a fiction, which is the one thing §8.2's
    /// timing note forbids.
    Passive,
    /// Not usable right now for a reason other than cooldown — no adjacent target
    /// for a takedown, no body to drag (§8.3). Discoverable, but greyed.
    Unusable,
}

impl AbilityState {
    /// The state's notation appended after the ability name (§11.4): `[N]` while
    /// active, `/N/` while cooling, `(on)` for a passive, a lone `—` while
    /// unusable, and nothing at all when ready — a ready ability needs no
    /// decoration, only its name and key.
    ///
    /// The number is rendered verbatim from the state, so what the panel shows is
    /// exactly what the player gets (§8.2) — the advertised-vs-real gap the old UI
    /// had cannot open here. A passive carries no number for the same reason: it
    /// has none to carry (#264).
    pub fn suffix(self) -> String {
        match self {
            AbilityState::Ready => String::new(),
            AbilityState::Active { remaining } => format!("[{remaining}]"),
            AbilityState::Cooling { remaining } => format!("/{remaining}/"),
            AbilityState::Passive => "(on)".to_string(),
            AbilityState::Unusable => "—".to_string(),
        }
    }
}

/// One row of the ability line and deployed panel (§11.4): an economy ability's
/// identity and the state it is in. Assembled from live runtime by
/// [`State::ability_statuses`](crate::State::ability_statuses); its hotkey and
/// name come from the [`AbilityId`], never a row position, so reordering the
/// panel can never move a key (§11.6) and a click resolves by identity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AbilityStatus {
    /// The economy ability this row is for — the identity a click resolves to and
    /// activates (§11.4), and the source of its hotkey and name.
    pub id: AbilityId,
    /// What state the ability is in right now (§11.4).
    pub state: AbilityState,
}

impl AbilityStatus {
    /// The ability's explicit §11.6 hotkey, by identity ([`AbilityId::hotkey`]) —
    /// never a row position (the regression [`ability_hotkey`] designs out).
    pub fn hotkey(&self) -> char {
        self.id.hotkey()
    }

    /// The ability's display name (§8.3), by identity ([`AbilityId::name`]).
    pub fn name(&self) -> &'static str {
        self.id.name()
    }

    /// The one line the deployed panel draws for this ability: `<key> <Name>` with
    /// the state notation tacked on when there is one — `r Run`, `c Camouflage [7]`,
    /// `d Decoy /12/`.
    pub fn label(&self) -> String {
        let suffix = self.state.suffix();
        if suffix.is_empty() {
            format!("{} {}", self.hotkey(), self.name())
        } else {
            format!("{} {} {}", self.hotkey(), self.name(), suffix)
        }
    }

    /// The compact readout for the **always-on ability line** (§11.4): just the
    /// hotkey, with the active/cooling number — or a passive's `(on)` — tucked
    /// inline (`c[7]`, `d/12/`, `v(on)`). Ready and unusable abilities show the
    /// bare key — their state is carried by colour alone, keeping the strip to one
    /// glyph each so the whole set fits a single row. The full name lives only in
    /// the deployed panel ([`Self::label`]).
    pub fn compact(&self) -> String {
        match self.state {
            AbilityState::Active { .. } | AbilityState::Cooling { .. } | AbilityState::Passive => {
                format!("{}{}", self.hotkey(), self.state.suffix())
            }
            AbilityState::Ready | AbilityState::Unusable => self.hotkey().to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// The economy model (§8.1, §8.2)
// ---------------------------------------------------------------------------

/// Identifies an ability the [`Deck`] holds — the activated ones it runs the clock
/// on (§8.2: activate → duration → cooldown), plus the **passives** that are simply
/// in effect while held (#264).
///
/// It is deliberately *not* every §8.3 row. Move and Wait are the turn loop's own
/// [`Input`](crate::Input)s, not deck abilities (§8.3: Move is "Not shown in the
/// UI"). Takedown and Drag are innate *bump* / held-state verbs (§7.2, §8.3) with
/// no duration or cooldown to govern — they resolve in their own tickets (#102,
/// #103) and stay out of this deck. What is left is exactly the activated set:
/// innate Run plus the salvaged tech.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum AbilityId {
    /// Innate escape (§8.3): 2 cells/turn while active.
    Run,
    /// Salvaged tech (§8.3): undetectable while still.
    Camouflage,
    /// Salvaged tech (§8.3): a fake intruder that draws Investigating.
    Decoy,
    /// Salvaged tech (§8.3): walk through solids, no concealment.
    Dephase,
    /// Salvaged tech (§8.3): doors open ahead and shut behind while active.
    Autodoors,
    /// Salvaged tech (§8.3): blinds and freezes guards in a radius, through walls.
    Confusion,
    /// Salvaged tech (§8.3), **passive**: 360° sight at extended range while held.
    Vision,
}

impl AbilityId {
    /// Every economy-governed ability, in the fixed deck-slot order. The order is
    /// display/iteration order only — hotkeys come from the identity map (§11.6),
    /// never from a position — but it *is* the order [`index`](Self::index) pins,
    /// so the two must not drift.
    pub const ALL: [AbilityId; 7] = [
        AbilityId::Run,
        AbilityId::Camouflage,
        AbilityId::Decoy,
        AbilityId::Dephase,
        AbilityId::Autodoors,
        AbilityId::Confusion,
        AbilityId::Vision,
    ];

    /// The **salvaged-tech** abilities (§8.3) — the found-in-the-facility set, as
    /// opposed to innate [`Run`](AbilityId::Run). This is the default eligible pool
    /// a `starting_abilities` grant (#244) draws from: the shipped, non-experimental
    /// tech (the gated experiments #239/#243 are not economy abilities yet, so the
    /// pool is exactly these six). Quick play grants the whole pool while its size
    /// meets the grant count; the draw only bites once the pool outgrows the grant.
    /// A passive (#264) is drawn from here like any other tech — it competes for the
    /// same slot, which is exactly what it pays with.
    pub const TECH: [AbilityId; 6] = [
        AbilityId::Camouflage,
        AbilityId::Decoy,
        AbilityId::Dephase,
        AbilityId::Autodoors,
        AbilityId::Confusion,
        AbilityId::Vision,
    ];

    /// Whether this ability is **innate** (§8.3) — always in the loadout, never
    /// drawn or found. Run is the only innate *economy* ability (Move/Wait/Takedown/
    /// Drag are the turn loop's own verbs, not deck abilities). The innate set is
    /// what a quick-play loadout starts with before its tech grant (#244).
    pub fn is_innate(self) -> bool {
        matches!(self, AbilityId::Run)
    }

    /// The ability's display name (§8.3) — the identity the settled §11.6 hotkey
    /// map ([`ability_hotkey`]) is keyed by, so a name and its key stay one fact.
    pub fn name(self) -> &'static str {
        match self {
            AbilityId::Run => "Run",
            AbilityId::Camouflage => "Camouflage",
            AbilityId::Decoy => "Decoy",
            AbilityId::Dephase => "Dephase",
            AbilityId::Autodoors => "Autodoors",
            AbilityId::Confusion => "Confusion",
            AbilityId::Vision => "Vision",
        }
    }

    /// Whether this ability is **passive** (#264) — always on while held, with no
    /// activation path and no clock ([`Ability::is_passive`]).
    pub fn is_passive(self) -> bool {
        self.def().is_passive()
    }

    /// The settled §11.6 hotkey, through the one explicit identity map — never a
    /// list position (the regression [`ability_hotkey`] designs out).
    pub fn hotkey(self) -> char {
        ability_hotkey(self.name()).expect("every economy ability has a settled §11.6 hotkey")
    }

    /// This ability's static definition (§8.1): its economy numbers, targeting, and
    /// behaviour. The catalog is `const` data — declaring a new ability is adding a
    /// row here (§8.1), not writing a system.
    pub fn def(self) -> &'static Ability {
        match self {
            AbilityId::Run => &RUN,
            AbilityId::Camouflage => &CAMOUFLAGE,
            AbilityId::Decoy => &DECOY,
            AbilityId::Dephase => &DEPHASE,
            AbilityId::Autodoors => &AUTODOORS,
            AbilityId::Confusion => &CONFUSION,
            AbilityId::Vision => &VISION,
        }
    }

    /// This ability's [`Deck`] slot index. Must match its position in [`ALL`](Self::ALL).
    fn index(self) -> usize {
        match self {
            AbilityId::Run => 0,
            AbilityId::Camouflage => 1,
            AbilityId::Decoy => 2,
            AbilityId::Dephase => 3,
            AbilityId::Autodoors => 4,
            AbilityId::Confusion => 5,
            AbilityId::Vision => 6,
        }
    }
}

/// The set of economy abilities a run **starts with** (§8.3/#244) — its ability
/// loadout, one of the three pieces of a run's reproducible config alongside the
/// seed and the [`LevelModifiers`](crate::LevelModifiers) (#245).
///
/// Which abilities a player holds is no longer fixed: quick play grants the innate
/// set plus a seeded draw of tech (#244), a campaign accumulates its set across
/// facilities (§2.2). Both resolve to *this* — a concrete, explicit set carried in
/// the shareable level-seed string ([`LevelSeed`](crate::LevelSeed)), so a handed-
/// around run reproduces the exact loadout, not just the geometry. Held as a
/// membership mask over [`AbilityId::ALL`] so it stays `Copy` and small.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Loadout {
    present: [bool; AbilityId::ALL.len()],
}

impl Loadout {
    /// The **full** loadout: every ability present, passives included.
    pub fn full() -> Self {
        Self {
            present: [true; AbilityId::ALL.len()],
        }
    }

    /// Every **activated** ability and no passives (#264) — the set the deck held
    /// before passives existed.
    ///
    /// Built up from [`empty`](Self::empty), like every other loadout here: a
    /// loadout is a set you *add* abilities to, never one you start full and carve
    /// down, so "which abilities does this run hold" always has an explicit answer
    /// rather than an implicit everything-minus. Handing a passive out implicitly
    /// would be the worst version of that — it reshapes perception for the whole
    /// run (Vision makes sight 360°, §8.3/#265), so it is always asked for.
    pub fn activated() -> Self {
        let mut loadout = Self::empty();
        for id in AbilityId::ALL {
            if !id.is_passive() {
                loadout = loadout.with(id);
            }
        }
        loadout
    }

    /// The **empty** loadout: no abilities at all. Not a state any *run* boots — a
    /// run always holds at least its innate set — but the neutral start the level-
    /// seed decoder (#245) and tests build a set up from, one [`with`](Self::with)
    /// at a time.
    pub fn empty() -> Self {
        Self {
            present: [false; AbilityId::ALL.len()],
        }
    }

    /// The **innate-only** loadout: just the always-available set (§8.3, Run) and no
    /// tech — the base a quick-play grant ([`with`](Self::with)) or a campaign
    /// pickup adds to.
    pub fn innate() -> Self {
        let mut loadout = Self::empty();
        for id in AbilityId::ALL {
            if id.is_innate() {
                loadout = loadout.with(id);
            }
        }
        loadout
    }

    /// The same loadout with `id` added — the granted-tech / found-tech step, one
    /// ability at a time.
    #[must_use]
    pub fn with(mut self, id: AbilityId) -> Self {
        self.present[id.index()] = true;
        self
    }

    /// Whether `id` is in the loadout — the run holds this ability.
    pub fn contains(self, id: AbilityId) -> bool {
        self.present[id.index()]
    }

    /// The abilities in the loadout, in the fixed [`AbilityId::ALL`] order — the
    /// canonical order the level-seed string serialises and the deck iterates.
    pub fn iter(self) -> impl Iterator<Item = AbilityId> {
        AbilityId::ALL
            .into_iter()
            .filter(move |id| self.present[id.index()])
    }
}

/// How an ability picks what it acts on (§8.4).
///
/// This is the ability's *declared* targeting, stored as data. Resolving it to a
/// concrete target — the cursor, validation, and the self/direction/tile resolution
/// — is the [`Targeting`](crate::Targeting) session's job (shipped in #149), driven
/// from [`State::begin_ability_targeting`](crate::State::begin_ability_targeting).
/// Range, where an ability has one, rides in [`TargetingMode::Tile`] as the §6.1
/// **box** radius, so "within range" is the single box notion sight already uses
/// (§6.1) rather than a second field that could disagree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TargetingMode {
    /// The player's own cell — Run, Camouflage, Dephase (§8.3).
    Itself,
    /// A cardinal from the player, the cell they face — Decoy (§8.3).
    Direction,
    /// A cell within a §6.1 box of `range`. No v1 ability uses it; it is here so the
    /// vocabulary is complete — the cursor that resolves it lives in the
    /// [`Targeting`](crate::Targeting) session.
    Tile { range: u32 },
}

/// The **effect vocabulary** (§8.1) — the small set of primitives a data-driven
/// ability's behaviour is built from.
///
/// This ticket *declares and stores* effects; it never interprets one. Applying an
/// effect is each ability's own ticket — Run (#101), Camouflage (#104), Decoy
/// (#105), Dephase (#106) — which reads the active deck and does the world-change.
/// The economy below runs purely on duration and cooldown and is blind to this
/// enum, which is what lets those tickets land one at a time.
///
/// There is one entry per starting-tech ability today. §8.1's standing warning
/// applies: **resist growing this to cover a one-off** — a behaviour the primitives
/// can't express reaches for [`Behaviour::Coded`], not a new variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    /// Run (§8.3): one extra step per turn while active → 2 cells/turn.
    ExtraStep,
    /// Camouflage (§8.3): undetectable on any turn the player does not move.
    ConcealWhileStill,
    /// Decoy (§8.3): spawn a fake intruder that draws Investigating, not Chasing.
    SpawnDecoy,
    /// Dephase (§8.3): fill → 0, pass through solids; **does not conceal**.
    Phase,
    /// Autodoors (§8.3, §7.6): while active, a door in the player's path opens as
    /// they step into it — no manual bump — and shuts behind them once they clear
    /// the throat, breaking a pursuer's line of sight (§10.3/§10.4).
    AutoDoors,
    /// Confusion (§8.3, §9): while active, every guard within a radius of the player
    /// is **blinded and frozen** — it does not sense and does not move — reaching
    /// **through walls** like the guard sense (§9). A costed panic-buy of time, not a
    /// kill: the guard resumes cleanly (its state and lead paused, not reset) when the
    /// window ends. The radius is [`CONFUSION_RADIUS`](crate::CONFUSION_RADIUS).
    Confuse,
    /// Vision (§8.3, §5/§6.1, #265): while in effect, the player's own sight is the
    /// full 360° arc ([`FULL_SIGHT_ARC`](crate::FULL_SIGHT_ARC)) at the extended
    /// range ([`ENHANCED_SIGHT_RANGE`](crate::ENHANCED_SIGHT_RANGE)) instead of §5's
    /// forward half-disc at 15. **Vision only** — the guard sense (§9) is a separate,
    /// innate channel and is deliberately not widened with it.
    EnhancedSight,
}

/// A data-driven ability's behaviour, or the code escape hatch (§8.1).
///
/// The distinction is for the *effect* tickets, not the economy: the [`Deck`] reads
/// only [`Ability::duration`] and [`Ability::cooldown`], never this, so an ability
/// activates, times out, and cools down identically whichever arm it is — that
/// sameness *is* the "behind the same interface" the escape hatch promises.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Behaviour {
    /// **Data — the common case (§8.1).** Behaviour is the interpretation of these
    /// effect primitives, applied by the ability's own ticket.
    Effects(&'static [Effect]),
    /// **Code — the escape hatch (§8.1).** For behaviour the vocabulary genuinely
    /// can't express (piloting a drone, rewinding time), implemented in plain code
    /// keyed on the [`AbilityId`]. No v1 ability needs it; the seam exists so adding
    /// one never means bending the data model to cover a one-off.
    Coded,
}

/// The **time economy** of an activated ability (§8.2): what it costs to switch
/// on, how it is aimed, and the two clocks the [`Deck`] runs on it.
///
/// Held apart from [`Ability`] so that a **passive** (#264) — which has none of
/// these — cannot state one. A passive is not "an ability with duration 0 and
/// cooldown 0"; it is an ability with no clock at all, and
/// [`AbilityMode`] is where that difference is made unrepresentable rather than
/// merely conventional.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Economy {
    cost: u32,
    targeting: TargetingMode,
    duration: u32,
    cooldown: u32,
}

impl Economy {
    /// The turn cost of activating (§4.4). Always one turn in v1 — activation costs
    /// *the turn*, no more — and recorded here only because §8.1's field list names
    /// it: a future multi-turn ritual would raise it here, not special-case the loop.
    pub fn cost(&self) -> u32 {
        self.cost
    }

    /// How the ability targets (§8.4), resolved by the
    /// [`Targeting`](crate::Targeting) session.
    pub fn targeting(&self) -> TargetingMode {
        self.targeting
    }

    /// Turns the ability stays active once switched on (§8.2). Zero means instant —
    /// no active window, straight to cooldown.
    pub fn duration(&self) -> u32 {
        self.duration
    }

    /// Turns of cooldown after the duration ends (§8.2). Frozen while active; the
    /// true lockout is `duration + cooldown`.
    pub fn cooldown(&self) -> u32 {
        self.cooldown
    }
}

/// How an ability is paid for — the §8.2 economy, or the **passive** extension of
/// it this repo added in #264.
///
/// §8.2 settles that the economy is *time*: turn cost, duration, cooldown. A
/// passive spends none of those, so it would be free — and §2.3 is explicit that a
/// costless ability is not a decision. The reconciliation: **a passive's cost is
/// the loadout slot it occupies** (§8.3, capped at 3 by #266). You hold it
/// *instead of* something else, permanently, and that is the whole price.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AbilityMode {
    /// **Activated** (§8.2): switched on deliberately, for a turn, and governed by
    /// the [`Deck`]'s duration/cooldown clocks.
    Activated(Economy),
    /// **Passive** (§8.2 extension, #264): never activated, never switched off, in
    /// effect for exactly as long as the run holds it. It has no slot in the deck
    /// to be in — "held" *is* "on" — so there is no activation moment for a replay
    /// or a mid-run pickup to get out of step with.
    Passive,
}

/// One ability declared as **data** (§8.1): how it is paid for ([`AbilityMode`])
/// and the behaviour the effect tickets consume.
///
/// Built as `const` catalog rows ([`AbilityId::def`]). Every number is `[START]`
/// (§8.3) — tunable, and pinned by a test so a change is a visible decision.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ability {
    id: AbilityId,
    mode: AbilityMode,
    behaviour: Behaviour,
}

impl Ability {
    /// Which ability this defines.
    pub fn id(&self) -> AbilityId {
        self.id
    }

    /// The display name (§8.3), via [`AbilityId::name`].
    pub fn name(&self) -> &'static str {
        self.id.name()
    }

    /// How the ability is paid for (§8.2/#264) — a time economy, or the slot.
    pub fn mode(&self) -> AbilityMode {
        self.mode
    }

    /// The ability's time economy (§8.2), or `None` for a **passive** — which has
    /// no cost, no targeting, no duration and no cooldown to report (#264).
    pub fn economy(&self) -> Option<Economy> {
        match self.mode {
            AbilityMode::Activated(economy) => Some(economy),
            AbilityMode::Passive => None,
        }
    }

    /// Whether this ability is **passive** (#264): always on while held, never
    /// activated, never stepped by the [`Deck`].
    pub fn is_passive(&self) -> bool {
        matches!(self.mode, AbilityMode::Passive)
    }

    /// The ability's behaviour — data effects or the code escape hatch (§8.1).
    pub fn behaviour(&self) -> Behaviour {
        self.behaviour
    }
}

// The §8.3 starting-set catalog. All numbers `[START]` (§8.3), pinned by
// `the_catalog_matches_the_design`. Effects are declared here and applied by each
// ability's own ticket; the economy is blind to them.

/// An activated ability's §8.2 economy, as one `const` expression — the shape every
/// [`AbilityMode::Activated`] row below is built from.
const fn activated(
    cost: u32,
    targeting: TargetingMode,
    duration: u32,
    cooldown: u32,
) -> AbilityMode {
    AbilityMode::Activated(Economy {
        cost,
        targeting,
        duration,
        cooldown,
    })
}

const RUN: Ability = Ability {
    id: AbilityId::Run,
    mode: activated(1, TargetingMode::Itself, 5, 12),
    behaviour: Behaviour::Effects(&[Effect::ExtraStep]),
};
const CAMOUFLAGE: Ability = Ability {
    id: AbilityId::Camouflage,
    mode: activated(1, TargetingMode::Itself, 10, 20),
    behaviour: Behaviour::Effects(&[Effect::ConcealWhileStill]),
};
const DECOY: Ability = Ability {
    id: AbilityId::Decoy,
    mode: activated(1, TargetingMode::Direction, 20, 30),
    behaviour: Behaviour::Effects(&[Effect::SpawnDecoy]),
};
const DEPHASE: Ability = Ability {
    id: AbilityId::Dephase,
    mode: activated(1, TargetingMode::Itself, 3, 30),
    behaviour: Behaviour::Effects(&[Effect::Phase]),
};
// Autodoors [START] (§8.3): a long active window — enough to walk a whole stretch
// of corridor door-to-door through a chase — paid for by a long lockout before the
// next flight (§7.6). Self-target toggle; free to cancel (§4.4).
const AUTODOORS: Ability = Ability {
    id: AbilityId::Autodoors,
    mode: activated(1, TargetingMode::Itself, 16, 40),
    behaviour: Behaviour::Effects(&[Effect::AutoDoors]),
};
// Confusion [START] (§8.3, §9, #240): a blind-and-freeze bubble around the player,
// through walls — powerful, so the cost is a *large* lockout that keeps it rare
// (§2.3/§13.2). A self/area toggle: it centres on the player and reaches
// [`CONFUSION_RADIUS`]. **Six** protected turns (§8.2 timing, the activation turn
// covered) — enough of a window to actually walk out of the bubble you bought, which
// three was not — then a long recharge before the next panic-buy. Duration and radius
// were raised together from the first pass (3/4): the window and the bubble are the
// levers, and the cooldown is what still makes spending it a real decision.
const CONFUSION: Ability = Ability {
    id: AbilityId::Confusion,
    mode: activated(1, TargetingMode::Itself, 6, 45),
    behaviour: Behaviour::Effects(&[Effect::Confuse]),
};
// Vision [START] (§5/§6.1, §8.3, #265): the first **passive** — no activation, no
// clock, in effect while held. Its whole price is the loadout slot (§8.2/#264),
// which is why it is allowed to be this good: standing 360° sight plus a longer
// reach ([`ENHANCED_SIGHT_RANGE`](crate::ENHANCED_SIGHT_RANGE)) is the strongest
// standing awareness in the game, and it costs a slot that could have carried a
// flight tool for the moment it all goes wrong.
const VISION: Ability = Ability {
    id: AbilityId::Vision,
    mode: AbilityMode::Passive,
    behaviour: Behaviour::Effects(&[Effect::EnhancedSight]),
};

/// The live economy state of one deck ability (§8.2): the three states the *time*
/// economy moves an ability through.
///
/// Distinct from the display [`AbilityState`], whose fourth case `Unusable` is
/// contextual (no adjacent target, no body to drag) and is never produced by the
/// clock; [`Deck::state`] projects a slot onto the display type. The transitions
/// below take only the economy *numbers*, never an [`Ability`] or its
/// [`Behaviour`] — that is what makes the economy provably blind to behaviour, so
/// a `Coded` ability rides the identical interface.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Slot {
    /// Inactive and off cooldown — usable this turn.
    #[default]
    Ready,
    /// Switched on, `remaining` turns of duration left (§8.2). Always `>= 1`.
    Active { remaining: u32 },
    /// Inactive, `remaining` turns of cooldown left — cooldown drains only here
    /// (§8.2), so this is the second half of the `duration + cooldown` lockout.
    /// Always `>= 1`.
    Cooling { remaining: u32 },
}

impl Slot {
    /// The slot an ability goes into when it switches off — its **frozen** cooldown
    /// begins to run (§8.2): [`Cooling`](Slot::Cooling) for `cooldown` turns, or
    /// straight back to [`Ready`](Slot::Ready) when there is no cooldown.
    fn cooling(cooldown: u32) -> Slot {
        if cooldown > 0 {
            Slot::Cooling {
                remaining: cooldown,
            }
        } else {
            Slot::Ready
        }
    }

    /// Begin using an ability with these economy numbers (§8.2): [`Active`] for its
    /// whole duration, or — an instant ability (duration 0) — straight into the
    /// cooldown. Valid only from [`Ready`](Slot::Ready); the caller gates on it.
    fn activated(duration: u32, cooldown: u32) -> Slot {
        if duration > 0 {
            Slot::Active {
                remaining: duration,
            }
        } else {
            Slot::cooling(cooldown)
        }
    }

    /// One **end-of-turn** tick (§8.2 timing — after all three phases, only on a
    /// spent turn). Duration drains while [`Active`](Slot::Active) and, on hitting
    /// 0, switches the ability off into its frozen cooldown; cooldown drains only
    /// while inactive. Returns the next slot and whether the duration *just ended*
    /// this tick (the near line's "faded" event, §11.7).
    fn ticked(self, cooldown: u32) -> (Slot, bool) {
        match self {
            Slot::Ready => (Slot::Ready, false),
            Slot::Active { remaining } => {
                // `remaining >= 1` always (see the variant), so this cannot underflow.
                let left = remaining - 1;
                if left == 0 {
                    (Slot::cooling(cooldown), true)
                } else {
                    (Slot::Active { remaining: left }, false)
                }
            }
            Slot::Cooling { remaining } => {
                let left = remaining - 1;
                let next = if left == 0 {
                    Slot::Ready
                } else {
                    Slot::Cooling { remaining: left }
                };
                (next, false)
            }
        }
    }

    /// Project the economy slot onto the display [`AbilityState`] the panel reads
    /// (§11.4): the number shown is the number the slot holds (§8.2 timing), and
    /// `Unusable` — being contextual — is never produced here.
    fn display(self) -> AbilityState {
        match self {
            Slot::Ready => AbilityState::Ready,
            Slot::Active { remaining } => AbilityState::Active { remaining },
            Slot::Cooling { remaining } => AbilityState::Cooling { remaining },
        }
    }
}

/// Per-ability economy runtime for the whole deck (§8.2) — one [`Slot`] per
/// [`AbilityId`], indexed by [`AbilityId::index`].
///
/// Owned by [`State`](crate::State) and stepped by the turn loop: [`activate`] and
/// [`deactivate`] in the player phase, [`tick`] at end of turn (§8.2 timing). The
/// `duration + cooldown` lockout is **emergent, not stored** — the deck keeps only
/// the current slot and reads every number fresh from [`AbilityId::def`], so
/// retuning a catalog value moves the lockout with it and nothing here needs to
/// change (§8.2). For v1 the whole set is available from the start (#104): a fresh
/// deck is all [`Ready`](Slot::Ready).
///
/// [`activate`]: Deck::activate
/// [`deactivate`]: Deck::deactivate
/// [`tick`]: Deck::tick
#[derive(Clone, Copy, Debug)]
pub(crate) struct Deck {
    slots: [Slot; AbilityId::ALL.len()],
    /// Which abilities the run actually holds (§8.3/#244) — the resolved
    /// [`Loadout`]. An ability absent here has no slot the player can drive: it is
    /// never listed on the ability line, never activates (a key press is the free
    /// §4.4 no-op), and reads as [`Unusable`](AbilityState::Unusable). For the full
    /// loadout — every bare run today — the deck behaves exactly as before.
    loadout: Loadout,
}

impl Deck {
    /// A fresh deck holding `loadout`, every held ability [`Ready`](Slot::Ready)
    /// (§8.3 — the starting set is available from the start, #104). Abilities not in
    /// the loadout are inert (see [`Deck::loadout`]).
    pub(crate) fn new(loadout: Loadout) -> Self {
        Deck {
            slots: [Slot::Ready; AbilityId::ALL.len()],
            loadout,
        }
    }

    /// The economy state of `id`, as the panel reads it (§11.4). An ability the run
    /// does not hold (not in the loadout, #244) reads as
    /// [`Unusable`](AbilityState::Unusable) — it is real but not yours. A held
    /// **passive** reads as [`Passive`](AbilityState::Passive) (#264): holding it is
    /// the whole of its state, so its slot is never consulted.
    pub(crate) fn state(&self, id: AbilityId) -> AbilityState {
        if !self.loadout.contains(id) {
            return AbilityState::Unusable;
        }
        if id.is_passive() {
            return AbilityState::Passive;
        }
        self.slots[id.index()].display()
    }

    /// The run's loadout — the abilities it holds (#244), for the UI line to list
    /// and a test to assert what was resolved.
    pub(crate) fn loadout(&self) -> Loadout {
        self.loadout
    }

    /// Activate `id` if the run holds it and it is [`Ready`](Slot::Ready). Returns
    /// whether it activated — `true` means the turn is spent (§4.4). Activating an
    /// ability that is not in the loadout (#244), a **passive** (#264 — there is no
    /// activation path to take), or one already active or cooling, is a mis-input: a
    /// **free** no-op (`false`), like bumping a wall (§4.4).
    pub(crate) fn activate(&mut self, id: AbilityId) -> bool {
        if !self.loadout.contains(id) {
            return false;
        }
        let Some(economy) = id.def().economy() else {
            return false; // a passive is already on; there is nothing to switch
        };
        let slot = &mut self.slots[id.index()];
        if *slot != Slot::Ready {
            return false;
        }
        *slot = Slot::activated(economy.duration, economy.cooldown);
        true
    }

    /// Toggle `id` off early if it is [`Active`](Slot::Active) (§4.4's free
    /// exception). Refunds nothing — the **full** cooldown still runs (§8.2:
    /// cancelling saves you nothing). Returns whether anything switched off; a
    /// toggle of a ready or cooling ability is a no-op, and a **passive** can never
    /// be switched off at all (#264: it ends when the loadout stops holding it, not
    /// on a keypress). Never spends the turn.
    pub(crate) fn deactivate(&mut self, id: AbilityId) -> bool {
        let Some(economy) = id.def().economy() else {
            return false;
        };
        let slot = &mut self.slots[id.index()];
        if !matches!(slot, Slot::Active { .. }) {
            return false;
        }
        *slot = Slot::cooling(economy.cooldown);
        true
    }

    /// Whether any ability **in effect** declares `effect` (§8.1) — how the turn
    /// loop asks "is an extra step owed?" without naming an ability: the loop
    /// interprets the effect vocabulary, so a future ability declaring the same
    /// effect gets the same behaviour for free, and a `Coded` ability never
    /// matches (its behaviour lives in code keyed on its id, not here).
    ///
    /// "In effect" is `Active` for an activated ability and simply **held** for a
    /// passive (#264) — the one place the two modes meet, so every effect the
    /// vocabulary already has works passively without a parallel system.
    pub(crate) fn effect_active(&self, effect: Effect) -> bool {
        AbilityId::ALL.into_iter().any(|id| {
            self.in_effect(id)
                && matches!(id.def().behaviour(), Behaviour::Effects(effects) if effects.contains(&effect))
        })
    }

    /// Whether `id`'s behaviour applies right now: a held passive always, an
    /// activated ability only while its duration runs (§8.2).
    fn in_effect(&self, id: AbilityId) -> bool {
        if id.is_passive() {
            return self.loadout.contains(id);
        }
        matches!(self.slots[id.index()], Slot::Active { .. })
    }

    /// The **end-of-turn** tick for every activated ability (§8.2 timing). Pushes one
    /// [`AbilityId`] per ability whose duration ended this tick — in
    /// [`AbilityId::ALL`] order — so the caller can raise its "faded" event (§11.7).
    /// Passives are not stepped (#264): they have no clock to advance and can never
    /// expire, so they never appear in `expired`.
    pub(crate) fn tick(&mut self, expired: &mut Vec<AbilityId>) {
        for id in AbilityId::ALL {
            let Some(economy) = id.def().economy() else {
                continue;
            };
            let (next, just_expired) = self.slots[id.index()].ticked(economy.cooldown);
            self.slots[id.index()] = next;
            if just_expired {
                expired.push(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The design's notation, pinned (§11.4): ready shows only the name, active is
    /// `[N]`, cooling is `/N/`, unusable is a lone dash — and the number is the
    /// state's own, rendered verbatim (§8.2).
    #[test]
    fn each_state_formats_in_the_design_notation() {
        assert_eq!(AbilityState::Ready.suffix(), "");
        assert_eq!(AbilityState::Active { remaining: 3 }.suffix(), "[3]");
        assert_eq!(AbilityState::Cooling { remaining: 2 }.suffix(), "/2/");
        assert_eq!(AbilityState::Unusable.suffix(), "—");
    }

    /// A status line reads `<key> <Name>` with the notation appended only when
    /// there is one — a ready ability carries no trailing space or bracket. Key
    /// and name come from the identity, so the row is built from an [`AbilityId`].
    #[test]
    fn a_status_line_joins_key_name_and_notation() {
        let ready = AbilityStatus {
            id: AbilityId::Run,
            state: AbilityState::Ready,
        };
        assert_eq!(ready.label(), "r Run");

        let cooling = AbilityStatus {
            id: AbilityId::Decoy,
            state: AbilityState::Cooling { remaining: 12 },
        };
        assert_eq!(cooling.label(), "d Decoy /12/");
    }

    /// The always-on line's compact form (§11.4): active and cooling tuck the
    /// number against the key, ready and unusable are the bare key (colour carries
    /// their state) — one glyph-group each so the whole set fits a single row.
    #[test]
    fn the_compact_readout_is_the_key_and_any_number() {
        let compact = |state| {
            AbilityStatus {
                id: AbilityId::Camouflage,
                state,
            }
            .compact()
        };
        assert_eq!(compact(AbilityState::Ready), "c");
        assert_eq!(compact(AbilityState::Unusable), "c");
        assert_eq!(compact(AbilityState::Active { remaining: 7 }), "c[7]");
        assert_eq!(compact(AbilityState::Cooling { remaining: 12 }), "c/12/");
    }

    /// A passive reads as **its own** state everywhere the panel speaks (#264):
    /// `(on)` — never `Ready` (which would invite a press), never `Active [N]`
    /// (which would promise a countdown that does not exist).
    #[test]
    fn a_passive_reads_as_on_and_never_as_a_clock_state() {
        assert_eq!(AbilityState::Passive.suffix(), "(on)");
        let status = AbilityStatus {
            id: AbilityId::Vision,
            state: AbilityState::Passive,
        };
        assert_eq!(status.label(), "v Vision (on)");
        assert_eq!(status.compact(), "v(on)");
        // Distinct from every clock state, with no number to be a fiction.
        assert_ne!(AbilityState::Passive.suffix(), AbilityState::Ready.suffix());
        for n in 0..4 {
            assert_ne!(
                AbilityState::Passive.suffix(),
                AbilityState::Active { remaining: n }.suffix(),
            );
        }
    }

    /// A row's hotkey and name are the **explicit** §11.6 identity, taken from the
    /// [`AbilityId`] — not the row's position. Were they derived from list order,
    /// reordering the panel would shuffle the keys (the regression §11.6 rules out).
    #[test]
    fn a_row_takes_its_hotkey_and_name_from_its_identity() {
        for id in AbilityId::ALL {
            let status = AbilityStatus {
                id,
                state: AbilityState::Ready,
            };
            assert_eq!(status.hotkey(), id.hotkey(), "{}'s key", id.name());
            assert_eq!(status.hotkey(), ability_hotkey(id.name()).unwrap());
            assert_eq!(status.name(), id.name());
        }
    }
}

#[cfg(test)]
mod economy_tests {
    use super::*;

    /// The §8.3 [START] catalog, pinned value by value: duration, cooldown,
    /// targeting, and the declared effect. A retune of any number must be a
    /// deliberate edit here, never a silent drift — and a moved number will move
    /// the emergent lockout with it (§8.2).
    #[test]
    fn the_catalog_matches_the_design_activated() {
        for (id, cost, targeting, duration, cooldown, effect) in [
            (
                AbilityId::Run,
                1,
                TargetingMode::Itself,
                5,
                12,
                Effect::ExtraStep,
            ),
            (
                AbilityId::Camouflage,
                1,
                TargetingMode::Itself,
                10,
                20,
                Effect::ConcealWhileStill,
            ),
            (
                AbilityId::Decoy,
                1,
                TargetingMode::Direction,
                20,
                30,
                Effect::SpawnDecoy,
            ),
            (
                AbilityId::Dephase,
                1,
                TargetingMode::Itself,
                3,
                30,
                Effect::Phase,
            ),
            (
                AbilityId::Autodoors,
                1,
                TargetingMode::Itself,
                16,
                40,
                Effect::AutoDoors,
            ),
            (
                AbilityId::Confusion,
                1,
                TargetingMode::Itself,
                6,
                45,
                Effect::Confuse,
            ),
        ] {
            let def = id.def();
            let economy = def
                .economy()
                .unwrap_or_else(|| panic!("{} is an activated ability", id.name()));
            assert_eq!(def.id(), id);
            assert_eq!(economy.cost(), cost, "{}", id.name());
            assert_eq!(economy.targeting(), targeting, "{}", id.name());
            assert_eq!(economy.duration(), duration, "{}", id.name());
            assert_eq!(economy.cooldown(), cooldown, "{}", id.name());
            match def.behaviour() {
                Behaviour::Effects(effects) => {
                    assert_eq!(effects, &[effect][..], "{}", id.name())
                }
                Behaviour::Coded => panic!("{} should be data-driven", id.name()),
            }
        }
    }

    /// The **passive** catalog (#264/#265), pinned: Vision is the one passive, it
    /// declares [`Effect::EnhancedSight`], and — the property that matters — it has
    /// **no economy at all**. Not a zeroed one: `economy()` is `None`, so there is no
    /// duration or cooldown for the deck to run and none for a future edit to set by
    /// accident (§8.2's extension, #264).
    #[test]
    fn the_catalog_matches_the_design_passive() {
        let passives: Vec<AbilityId> = AbilityId::ALL
            .into_iter()
            .filter(|id| id.is_passive())
            .collect();
        assert_eq!(passives, vec![AbilityId::Vision], "the shipped passive set");

        let def = AbilityId::Vision.def();
        assert!(def.is_passive());
        assert_eq!(def.mode(), AbilityMode::Passive);
        assert_eq!(def.economy(), None, "a passive spends no time (§8.2/#264)");
        match def.behaviour() {
            Behaviour::Effects(effects) => {
                assert_eq!(effects, &[Effect::EnhancedSight][..]);
            }
            Behaviour::Coded => panic!("Vision should be data-driven (§8.1)"),
        }
    }

    /// Every ability is exactly one of the two modes, and the two accessors agree —
    /// [`Ability::is_passive`] is `true` precisely when there is no economy. A row
    /// that claimed both (or neither) would leave the deck without a rule to run it
    /// by; the type makes that unrepresentable and this pins that it stays so.
    #[test]
    fn every_ability_is_activated_or_passive_and_never_both() {
        for id in AbilityId::ALL {
            let def = id.def();
            assert_eq!(
                def.is_passive(),
                def.economy().is_none(),
                "{} disagrees with itself about its mode",
                id.name(),
            );
            match def.mode() {
                AbilityMode::Activated(economy) => {
                    assert_eq!(def.economy(), Some(economy), "{}", id.name());
                    assert_eq!(
                        economy.cost(),
                        1,
                        "{} costs the turn it is activated on (§4.4)",
                        id.name(),
                    );
                }
                AbilityMode::Passive => assert!(def.is_passive(), "{}", id.name()),
            }
        }
    }

    /// [`AbilityId::ALL`] and [`AbilityId::index`] must agree — the deck indexes
    /// slots by `index`, so a drift would alias two abilities onto one slot.
    #[test]
    fn all_and_index_agree() {
        for (i, id) in AbilityId::ALL.into_iter().enumerate() {
            assert_eq!(id.index(), i, "{}", id.name());
        }
    }

    /// Name and hotkey come from the identity map (§11.6), reachable from the id.
    #[test]
    fn each_id_carries_its_settled_hotkey() {
        assert_eq!(AbilityId::Run.hotkey(), 'r');
        assert_eq!(AbilityId::Camouflage.hotkey(), 'c');
        assert_eq!(AbilityId::Decoy.hotkey(), 'd');
        assert_eq!(AbilityId::Dephase.hotkey(), 'x');
        assert_eq!(AbilityId::Autodoors.hotkey(), 'a');
        assert_eq!(AbilityId::Confusion.hotkey(), 'z');
    }

    /// A fresh deck is all Ready (§8.3: the v1 set is available from the start) —
    /// bar the passives, which are never "ready" because there is nothing to ready
    /// them for: they are already on (#264).
    #[test]
    fn a_fresh_deck_is_all_ready() {
        let deck = Deck::new(Loadout::full());
        for id in AbilityId::ALL {
            let expected = if id.is_passive() {
                AbilityState::Passive
            } else {
                AbilityState::Ready
            };
            assert_eq!(deck.state(id), expected, "{}", id.name());
        }
    }

    /// [`Loadout::activated`] is exactly [`Loadout::full`] minus the passives — the
    /// default a hand-built state boots with, so no test acquires a passive's
    /// run-long perception change without asking for it (#264/#265).
    #[test]
    fn the_activated_loadout_is_the_full_one_without_passives() {
        let activated = Loadout::activated();
        for id in AbilityId::ALL {
            assert_eq!(activated.contains(id), !id.is_passive(), "{}", id.name());
        }
        assert_ne!(
            activated,
            Loadout::full(),
            "a passive ships, so they differ"
        );
    }

    /// Activation moves a Ready ability to Active for its **whole** duration — the
    /// number the panel shows before the first end-of-turn tick (§8.2 timing).
    #[test]
    fn activation_sets_the_full_duration() {
        let mut deck = Deck::new(Loadout::full());
        assert!(deck.activate(AbilityId::Dephase));
        assert_eq!(
            deck.state(AbilityId::Dephase),
            AbilityState::Active { remaining: 3 },
            "the panel shows the full duration, not duration − 1",
        );
        // Re-activating an active ability is a free no-op — nothing changes.
        assert!(!deck.activate(AbilityId::Dephase));
        assert_eq!(
            deck.state(AbilityId::Dephase),
            AbilityState::Active { remaining: 3 }
        );
    }

    /// The §8.2 timing convention, at the economy level: an N-turn ability is
    /// **Active for exactly N ticks including activation**, then flips to cooling —
    /// so a freshly activated N yields N protected turns, the activation turn
    /// covered. (Dephase, N = 3.)
    #[test]
    fn an_n_turn_ability_is_active_for_n_ticks_including_activation() {
        let mut deck = Deck::new(Loadout::full());
        deck.activate(AbilityId::Dephase); // the activation turn is protected turn 1
        let mut active_ticks = 1;
        loop {
            let mut expired = Vec::new();
            deck.tick(&mut expired);
            if matches!(deck.state(AbilityId::Dephase), AbilityState::Active { .. }) {
                active_ticks += 1;
            } else {
                // The tick that ended the duration reports it exactly once.
                assert_eq!(expired, vec![AbilityId::Dephase]);
                break;
            }
        }
        assert_eq!(active_ticks, 3, "N protected turns, activation included");
    }

    /// The full `duration + cooldown` lockout (§8.2), emergent from the rules:
    /// Run (dur 5 / cd 12) is unusable for 5 + 12 = 17 ticks and Ready again on the
    /// 18th, with the cooldown **frozen** for the whole duration (it never drains
    /// while Active).
    #[test]
    fn the_lockout_is_duration_plus_cooldown() {
        let mut deck = Deck::new(Loadout::full());
        deck.activate(AbilityId::Run);

        let mut seen_active = 0;
        let mut seen_cooling = 0;
        for tick in 1..=17 {
            // Cooldown is frozen while active: the first 5 ticks are still Active,
            // and the cooling that follows starts at the *full* 12, never partway.
            match deck.state(AbilityId::Run) {
                AbilityState::Active { .. } => seen_active += 1,
                AbilityState::Cooling { remaining } => {
                    seen_cooling += 1;
                    if seen_cooling == 1 {
                        assert_eq!(remaining, 12, "cooldown was frozen through the duration");
                    }
                }
                other => panic!("tick {tick}: still locked out, got {other:?}"),
            }
            let mut expired = Vec::new();
            deck.tick(&mut expired);
        }
        assert_eq!(seen_active, 5, "5 active turns");
        assert_eq!(seen_cooling, 12, "12 cooling turns");
        assert_eq!(
            deck.state(AbilityId::Run),
            AbilityState::Ready,
            "Ready again on the 18th turn — lockout is exactly duration + cooldown",
        );
    }

    /// Toggling off early is free and refunds nothing: the ability drops straight
    /// into its **full** cooldown (§8.2 — cancelling saves you nothing).
    #[test]
    fn toggling_off_early_pays_the_full_cooldown() {
        let mut deck = Deck::new(Loadout::full());
        deck.activate(AbilityId::Camouflage); // dur 10 / cd 20
        let mut expired = Vec::new();
        deck.tick(&mut expired); // one turn of duration used (Active 10 → 9)
        assert_eq!(
            deck.state(AbilityId::Camouflage),
            AbilityState::Active { remaining: 9 }
        );
        assert!(deck.deactivate(AbilityId::Camouflage));
        assert_eq!(
            deck.state(AbilityId::Camouflage),
            AbilityState::Cooling { remaining: 20 },
            "early cancel still pays the whole cooldown",
        );
        // Toggling off a non-active ability is a no-op.
        assert!(!deck.deactivate(AbilityId::Run));
    }

    /// The **escape hatch** (§8.1): a `Coded` ability rides the *identical* economy.
    /// The transitions read only the numbers ([`Slot::activated`]/[`Slot::ticked`]
    /// take no [`Ability`]), so a coded ability with the same duration/cooldown
    /// steps through activation, its active window, and cooldown exactly as a data
    /// ability does — only its effect *application* (elsewhere) would differ.
    #[test]
    fn the_economy_is_blind_to_behaviour() {
        // A hypothetical coded ability whose behaviour the vocabulary can't express.
        const CODED: Ability = Ability {
            id: AbilityId::Run, // id is irrelevant to the economy; reuse one
            mode: activated(1, TargetingMode::Itself, 2, 3),
            behaviour: Behaviour::Coded,
        };
        // A data ability with the *same* numbers steps identically.
        let data_duration = 2;
        let data_cooldown = 3;

        assert!(matches!(CODED.behaviour(), Behaviour::Coded));

        let coded_economy = CODED.economy().expect("the coded ability is activated");
        let coded = Slot::activated(coded_economy.duration(), coded_economy.cooldown());
        let data = Slot::activated(data_duration, data_cooldown);
        assert_eq!(coded, data, "activation ignores behaviour");

        // Walk both through the full lockout in lockstep.
        let (mut c, mut d) = (coded, data);
        for _ in 0..(2 + 3 + 1) {
            let (cn, _) = c.ticked(coded_economy.cooldown());
            let (dn, _) = d.ticked(data_cooldown);
            assert_eq!(cn, dn, "each tick ignores behaviour");
            c = cn;
            d = dn;
        }
        assert_eq!(c, Slot::Ready);
    }

    /// The loadout seam (#244): a deck built from a partial [`Loadout`] holds only
    /// the granted abilities. An ability the run does not have reads as
    /// [`Unusable`](AbilityState::Unusable) and refuses activation as a **free**
    /// no-op (§4.4), while a held one activates normally — so a key press for an
    /// ability you were not granted does nothing, exactly like bumping a wall.
    #[test]
    fn a_partial_loadout_holds_only_its_granted_abilities() {
        // Innate-only: Run is held, the tech is not.
        let mut deck = Deck::new(Loadout::innate());
        assert_eq!(
            deck.state(AbilityId::Run),
            AbilityState::Ready,
            "Run is held"
        );
        for tech in AbilityId::TECH {
            assert_eq!(
                deck.state(tech),
                AbilityState::Unusable,
                "{} is not in the loadout",
                tech.name(),
            );
            assert!(
                !deck.activate(tech),
                "{} cannot activate — a free no-op",
                tech.name(),
            );
            assert_eq!(deck.state(tech), AbilityState::Unusable, "still not yours");
        }
        // The held ability activates as usual.
        assert!(deck.activate(AbilityId::Run), "the held ability activates");
        assert!(matches!(
            deck.state(AbilityId::Run),
            AbilityState::Active { .. }
        ));
    }

    /// A **passive** is on because it is *held* (#264) — there is no activation to
    /// perform and no toggle to pull. Both inputs are refused as **free** no-ops
    /// (§4.4, exactly like pressing a key for an ability you weren't granted), and
    /// the effect is in force the whole time regardless.
    #[test]
    fn a_passive_is_in_effect_because_it_is_held() {
        let mut deck = Deck::new(Loadout::innate().with(AbilityId::Vision));

        assert_eq!(deck.state(AbilityId::Vision), AbilityState::Passive);
        assert!(
            deck.effect_active(Effect::EnhancedSight),
            "held is on — no activation needed",
        );

        assert!(!deck.activate(AbilityId::Vision), "nothing to switch on");
        assert!(!deck.deactivate(AbilityId::Vision), "nothing to switch off");
        assert_eq!(
            deck.state(AbilityId::Vision),
            AbilityState::Passive,
            "neither input moved it",
        );
        assert!(deck.effect_active(Effect::EnhancedSight));
    }

    /// A passive the run does **not** hold is [`Unusable`](AbilityState::Unusable)
    /// like any ungranted ability, and — the half that matters — its effect is not
    /// in force. Holding it is the whole switch, so not holding it is the off state.
    #[test]
    fn a_passive_not_held_is_unusable_and_out_of_effect() {
        let deck = Deck::new(Loadout::innate());
        assert_eq!(deck.state(AbilityId::Vision), AbilityState::Unusable);
        assert!(!deck.effect_active(Effect::EnhancedSight));
    }

    /// A passive is **never stepped by the clock** (#264): ticking a deck that holds
    /// one leaves it `Passive` forever and never reports it as expired, so it cannot
    /// silently run out mid-run the way a duration does.
    #[test]
    fn a_passive_never_ticks_and_never_expires() {
        let mut deck = Deck::new(Loadout::full());
        for turn in 0..100 {
            let mut expired = Vec::new();
            deck.tick(&mut expired);
            assert_eq!(
                deck.state(AbilityId::Vision),
                AbilityState::Passive,
                "turn {turn}",
            );
            assert!(
                !expired.contains(&AbilityId::Vision),
                "turn {turn}: a passive has no duration to end",
            );
            assert!(deck.effect_active(Effect::EnhancedSight), "turn {turn}");
        }
    }

    /// **The activated economy is untouched by the passive's arrival** (#264). Every
    /// activated ability, in a deck that also holds the passive, walks its exact
    /// §8.2 lockout — `duration` turns active then `cooldown` turns cooling, Ready on
    /// the turn after — with the passive ticking alongside and changing nothing.
    #[test]
    fn a_passive_in_the_deck_changes_no_activated_ability_timing() {
        for id in AbilityId::ALL.into_iter().filter(|id| !id.is_passive()) {
            let economy = id.def().economy().expect("activated");
            let (duration, cooldown) = (economy.duration(), economy.cooldown());

            let mut deck = Deck::new(Loadout::full());
            assert!(deck.activate(id), "{}", id.name());

            let mut active = 0;
            let mut cooling = 0;
            for _ in 0..(duration + cooldown) {
                match deck.state(id) {
                    AbilityState::Active { .. } => active += 1,
                    AbilityState::Cooling { .. } => cooling += 1,
                    other => panic!("{}: locked out, got {other:?}", id.name()),
                }
                let mut expired = Vec::new();
                deck.tick(&mut expired);
            }
            assert_eq!(active, duration, "{} active turns", id.name());
            assert_eq!(cooling, cooldown, "{} cooling turns", id.name());
            assert_eq!(deck.state(id), AbilityState::Ready, "{}", id.name());
        }
    }

    /// An **instant** ability (duration 0) has no active window: it activates
    /// straight into its cooldown — the machinery the innate/instant abilities
    /// (their own tickets) can lean on without a special case here.
    #[test]
    fn an_instant_ability_skips_straight_to_cooldown() {
        assert_eq!(Slot::activated(0, 4), Slot::Cooling { remaining: 4 });
        // Instant with no cooldown loops right back to Ready.
        assert_eq!(Slot::activated(0, 0), Slot::Ready);
    }
}
