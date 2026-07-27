//! The ability bar's display vocabulary (§11.4) — **what the always-on ability
//! bar says**, assembled from the run's real ability economy.
//!
//! §11.4's settled answer to §15 Q9 is one always-on **named bar**: every ability
//! the run holds, drawn by its short bar name with its state notation tucked
//! against it, driven by real per-ability runtime and *actionable* — a click
//! resolves to the ability under it and activates it exactly as its hotkey would
//! (§11.4, §11.6). This module owns the *display* half: the states an ability reads
//! as ([`AbilityState`]) and how each formats, the bounded bar names, and one bar
//! entry ([`AbilityStatus`]). The render composes and hit-tests them
//! ([`render_screen`](crate::render_screen), [`ability_at`](crate::ability_at)).
//!
//! # The bar has to fit, and the compiler is what says so
//!
//! A run holds at most [`AbilityId::MAX_HELD`] abilities (§8.3 — innate Run plus the
//! three-tech grant) and the bar names all of them on one row of a 40-wide board
//! (§10.2). That is a real budget, so it is spent deliberately: the bar names
//! ([`AbilityId::bar_name`]) are short by design, the widest state notation is
//! **derived from the catalog's own numbers** ([`MAX_STATE_NOTATION`]) rather than
//! written down, and the render turns the two into a compile-time assertion that the
//! worst-case bar fits the board. Renaming an ability, raising a cooldown past 99,
//! or raising the grant then fails the *build* — never the frame.
//!
//! # Two halves: the economy model and its display
//!
//! The per-ability **runtime economy** (§8.1/§8.2) lives in this module too —
//! [`AbilityId`] and its data-driven [`Ability`] catalog, the effect vocabulary
//! ([`Effect`]) and the code escape hatch ([`Behaviour`]), and the [`Deck`] the
//! turn loop steps: activation, early toggle-off, and the end-of-turn
//! duration/cooldown tick, with the `duration + cooldown` lockout *emergent* from
//! the rules rather than stored. Alongside the two clocks sits the one non-time
//! axis (§8.2/#302): an optional **per-level use budget**
//! ([`Economy::uses_per_level`]), set at level start, counted down by use, and never
//! given back — scarcity for an effect too strong to hand out on a cooldown alone.
//! The deck reads each ability's state as one of the
//! display [`AbilityState`]s ([`Deck::state`]) — the number the player actually
//! gets (§8.2 timing) — which is how the two halves meet.
//! [`State::ability_statuses`](crate::State::ability_statuses) builds the bar
//! straight from that live state, one entry per held ability.
//!
//! Two things are real and load-bearing across the display:
//!
//! - **Hotkeys come from [`ability_hotkey`](crate::input::ability_hotkey)**, the
//!   settled §11.6 identity→letter map — never from the bar's entry order. A key
//!   is a fixed fact about an ability, so reordering or trimming the list can
//!   never move one (the §11.6 regression this repo already designed out); and a
//!   click resolves by that same identity, never by the entry it lands on. The bar
//!   itself no longer *shows* the letter — it has names to draw instead — so the
//!   help panel's Legend card is where a player reads the keys off (§15 Q9).
//! - **The number shown is the number the player gets** (§8.2 timing): the bar
//!   formats exactly the value it is handed and advertises nothing else, so it
//!   cannot re-introduce the old advertised-vs-real discrepancy.

use crate::input::ability_hotkey;

/// The runtime state of one ability, as the player reads it (§11.4): the cases the
/// bar must keep discoverable — ready, active, cooling, passive, unusable.
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
    /// Usable, and **budgeted** (§8.2/#302): the level allows `uses` more presses of
    /// it, ever. Shown [`(N)`](AbilityState::suffix) — the parenthetical shape is
    /// borrowed from [`PASSIVE_MARKER`] deliberately, because both mean *not a
    /// timer*. A bracket or a slash would have put this number on the same footing
    /// as a duration or a cooldown, and it is the opposite of those: nothing counts
    /// it down but the player, and nothing ever counts it back up.
    ///
    /// Its own state rather than a payload on [`Ready`](AbilityState::Ready), so no
    /// surface can show a budgeted ability as plain ready and quietly drop the one
    /// number that says how much of the level's supply is left.
    Limited { uses: u32 },
    /// The per-level use budget is **spent** (§8.2/#302) — for the rest of this
    /// facility, there is nothing to press. Draws exactly as
    /// [`Unusable`](AbilityState::Unusable) does, because that is what it is; but it
    /// is never [`Ready`](AbilityState::Ready) and never `/0/`, because it is not
    /// waiting for anything. There is no recharge (§8.2's fence).
    Exhausted,
    /// A **passive** ability (§8.2/#264): in effect for as long as it is held, with
    /// no activation, no duration and no cooldown.
    ///
    /// Its own state deliberately, rather than a permanent
    /// [`Active`](AbilityState::Active): the clock states all carry "and then it
    /// ends", and a passive never does. Stretching `Active` to mean always-on would
    /// make the number the bar shows a fiction, which is the one thing §8.2's
    /// timing note forbids — and it would put a countdown on the bar for something
    /// that never counts down.
    ///
    /// It reads [`PASSIVE_MARKER`] — a bare `(on)` where the clock states carry a
    /// number (#264 deferred the marker to this rework, #287). No number, because
    /// there is none; not *nothing*, because a passive sitting undecorated beside
    /// the ready abilities would read as one more thing you could press, and it is
    /// the one entry on the bar you never can.
    Passive,
    /// Not usable right now for a reason other than cooldown — no adjacent target
    /// for a takedown, no body to drag (§8.3). Discoverable, but greyed.
    Unusable,
}

impl AbilityState {
    /// The state's notation appended after the ability name (§11.4): `[N]` while
    /// active, `/N/` while cooling, [`PASSIVE_MARKER`] while passive, a lone `—`
    /// while unusable, and nothing at all when ready — a ready ability needs no
    /// decoration, only its name.
    ///
    /// The number is rendered verbatim from the state, so what the bar shows is
    /// exactly what the player gets (§8.2) — the advertised-vs-real gap the old UI
    /// had cannot open here. The widest this can ever come out is
    /// [`MAX_STATE_NOTATION`], derived from the catalog itself.
    pub fn suffix(self) -> String {
        match self {
            AbilityState::Ready => String::new(),
            AbilityState::Active { remaining } => format!("[{remaining}]"),
            AbilityState::Cooling { remaining } => format!("/{remaining}/"),
            AbilityState::Limited { uses } => format!("({uses})"),
            AbilityState::Passive => PASSIVE_MARKER.to_string(),
            // Spent is unusable, and says so in the one word the bar has for it.
            AbilityState::Exhausted | AbilityState::Unusable => "—".to_string(),
        }
    }
}

/// What a **passive** ability shows where an activated one shows its clock (§11.4,
/// #264/#287): `(on)` — it is in effect, and it will not stop being.
///
/// Deliberately as wide as the widest number notation (`/45/`) rather than wider:
/// the bar's whole budget is per-entry width, so an always-on marker that cost more
/// than a countdown would have made passives the reason names had to shrink.
pub(crate) const PASSIVE_MARKER: &str = "(on)";

/// One entry on the always-on ability bar (§11.4): a held ability's identity and
/// the state it is in. Assembled from live runtime by
/// [`State::ability_statuses`](crate::State::ability_statuses); its hotkey and
/// name come from the [`AbilityId`], never an entry position, so reordering the
/// bar can never move a key (§11.6) and a click resolves by identity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AbilityStatus {
    /// The ability this entry is for — the identity a click resolves to and
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

    /// The ability's full display name (§8.3), by identity ([`AbilityId::name`]).
    pub fn name(&self) -> &'static str {
        self.id.name()
    }

    /// The entry the **always-on ability bar** draws for this ability (§11.4): its
    /// bar name ([`AbilityId::bar_name`]) with its state notation tucked against it
    /// — `Camo[7]`, `Doors/12/`, `Sight(on)` — and the bare name when ready, whose
    /// state colour says all there is to say. Never wider than [`MAX_BAR_ENTRY`],
    /// which is what lets the whole held set be named on one row.
    pub fn bar_entry(&self) -> String {
        format!("{}{}", self.id.bar_name(), self.state.suffix())
    }
}

/// The widest state notation ([`AbilityState::suffix`]) the catalog can produce, in
/// cells: the longest duration or cooldown in it plus the two delimiters of `[N]` /
/// `/N/`, or the [`PASSIVE_MARKER`], whichever is wider. **Derived, not written
/// down** — retuning a §8.3 number past 99 widens this on its own, and trips the
/// ability bar's compile-time width bound (§11.4) instead of quietly truncating a
/// row.
pub(crate) const MAX_STATE_NOTATION: usize = max_state_notation();

/// The widest single **bar entry** (§11.4): the longest bar name plus the widest
/// state notation, tucked together as [`AbilityStatus::bar_entry`] draws them
/// (`Decoy/30/`, `Sight(on)`). The render sizes its compile-time bound off this.
pub(crate) const MAX_BAR_ENTRY: usize = max_bar_name() + MAX_STATE_NOTATION;

/// How many abilities are **innate** (§8.3) — the part of a loadout that is never
/// drawn. `const` so [`AbilityId::MAX_HELD`] is arithmetic over the catalog rather
/// than a second number to keep in step.
const fn innate_count() -> usize {
    let mut count = 0;
    let mut i = 0;
    while i < AbilityId::ALL.len() {
        if AbilityId::ALL[i].is_innate() {
            count += 1;
        }
        i += 1;
    }
    count
}

/// Cells taken by `n` in decimal — the width of a duration or cooldown as the
/// notation prints it.
const fn decimal_width(n: u32) -> usize {
    let mut width = 1;
    let mut rest = n / 10;
    while rest > 0 {
        rest /= 10;
        width += 1;
    }
    width
}

/// Walk the catalog for the widest notation any ability can show: the biggest
/// number an [`Economy`] carries plus its two delimiters, against the passive
/// marker's own width. `const` so [`MAX_STATE_NOTATION`] is a compile-time fact.
const fn max_state_notation() -> usize {
    let mut widest = PASSIVE_MARKER.len();
    let mut i = 0;
    while i < AbilityId::ALL.len() {
        match AbilityId::ALL[i].def().mode {
            // A passive has no clock at all — its marker is the starting value above.
            AbilityMode::Passive => {}
            AbilityMode::Activated(economy) => {
                let duration = decimal_width(economy.duration) + 2;
                let cooldown = decimal_width(economy.cooldown) + 2;
                if duration > widest {
                    widest = duration;
                }
                if cooldown > widest {
                    widest = cooldown;
                }
                // A use budget is a third number the bar can show — `(N)` (#302) —
                // so it is measured here like the two clocks, and a budget wide
                // enough to push the row over fails the build rather than the frame.
                if let Some(uses) = economy.uses {
                    let notation = decimal_width(uses) + 2;
                    if notation > widest {
                        widest = notation;
                    }
                }
            }
        }
        i += 1;
    }
    widest
}

/// The longest [`AbilityId::bar_name`], in cells. Byte length **is** cell width
/// because the names are ASCII, which [`bar_names_are_ascii`] pins right below.
const fn max_bar_name() -> usize {
    let mut widest = 0;
    let mut i = 0;
    while i < AbilityId::ALL.len() {
        let len = AbilityId::ALL[i].bar_name().len();
        if len > widest {
            widest = len;
        }
        i += 1;
    }
    widest
}

/// Whether every bar name is ASCII — the assumption [`max_bar_name`] rests on, since
/// the grid is one cell per *character* and `len()` counts *bytes*.
const fn bar_names_are_ascii() -> bool {
    let mut i = 0;
    while i < AbilityId::ALL.len() {
        let bytes = AbilityId::ALL[i].bar_name().as_bytes();
        let mut b = 0;
        while b < bytes.len() {
            if !bytes[b].is_ascii() {
                return false;
            }
            b += 1;
        }
        i += 1;
    }
    true
}

const _: () = assert!(
    bar_names_are_ascii(),
    "an ability bar name must be ASCII: one byte is one grid cell (§11.1)",
);

/// Whether every declared per-level use budget stays inside its fence (§8.2/#302):
/// **at least one** — a row granting zero uses would ship an ability born
/// [`Exhausted`](AbilityState::Exhausted), which is a deleted ability written the
/// long way — and **single digits**, because it is a bound on what the level lets
/// you do, not a bar to manage. Ten uses is not scarcity, it is an inventory.
const fn use_budgets_are_single_digit() -> bool {
    let mut i = 0;
    while i < AbilityId::ALL.len() {
        if let AbilityMode::Activated(economy) = AbilityId::ALL[i].def().mode {
            if let Some(uses) = economy.uses {
                if uses == 0 || uses > 9 {
                    return false;
                }
            }
        }
        i += 1;
    }
    true
}

const _: () = assert!(
    use_budgets_are_single_digit(),
    "a per-level use budget is 1–9 (§8.2/#302): a bound on the level, not a resource bar",
);

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
    /// Salvaged tech (§8.3/#303): bore through your one adjacent wall, permanently,
    /// a few times per level.
    PierceWall,
}

impl AbilityId {
    /// Every economy-governed ability, in the fixed deck-slot order. The order is
    /// display/iteration order only — hotkeys come from the identity map (§11.6),
    /// never from a position — but it *is* the order [`index`](Self::index) pins,
    /// so the two must not drift.
    pub const ALL: [AbilityId; 8] = [
        AbilityId::Run,
        AbilityId::Camouflage,
        AbilityId::Decoy,
        AbilityId::Dephase,
        AbilityId::Autodoors,
        AbilityId::Confusion,
        AbilityId::Vision,
        AbilityId::PierceWall,
    ];

    /// The **salvaged-tech** abilities (§8.3) — the found-in-the-facility set, as
    /// opposed to innate [`Run`](AbilityId::Run). This is the default eligible pool
    /// a `starting_abilities` grant (#244) draws from: the shipped, non-experimental
    /// tech (the gated experiments #239/#243 are not economy abilities yet, so the
    /// pool is exactly the rows listed here). Quick play grants the whole pool while its size
    /// meets the grant count; the draw only bites once the pool outgrows the grant.
    /// A passive (#264) is drawn from here like any other tech — it competes for the
    /// same slot, which is exactly what it pays with.
    pub const TECH: [AbilityId; 7] = [
        AbilityId::Camouflage,
        AbilityId::Decoy,
        AbilityId::Dephase,
        AbilityId::Autodoors,
        AbilityId::Confusion,
        AbilityId::Vision,
        AbilityId::PierceWall,
    ];

    /// The **innate** abilities (§8.3) — the part of a loadout that is never drawn
    /// or found, and the complement of [`TECH`](Self::TECH) over
    /// [`ALL`](Self::ALL). Derived from [`is_innate`](Self::is_innate) rather than
    /// written out, so promoting an ability cannot leave the two lists disagreeing.
    ///
    /// It is a list rather than a count because the level-seed token (#333) encodes
    /// the innate set positionally: the innate half of a loadout is a bitset over
    /// *this* order, and the tech half a combination index over `TECH`'s.
    pub const INNATE: [AbilityId; innate_count()] = {
        let mut innate = [AbilityId::Run; innate_count()];
        let (mut read, mut write) = (0, 0);
        while read < Self::ALL.len() {
            if Self::ALL[read].is_innate() {
                innate[write] = Self::ALL[read];
                write += 1;
            }
            read += 1;
        }
        innate
    };

    /// The most **salvaged tech** a run holds at once (§8.3/§10.2/#266) — the
    /// `starting_abilities` grant count (#244), and the cap a campaign accumulation
    /// (§2.2) would have to respect too. Settled at three, and load-bearing beyond
    /// the economy: it is what makes the held set small enough for the ability bar
    /// to name every entry on one row (§11.4), and it is the whole price a passive
    /// pays (§8.2/#264 — a slot, permanently).
    pub const MAX_TECH_HELD: usize = 3;

    /// The most abilities a run holds at once: the innate set (§8.3 — Run alone)
    /// plus [`MAX_TECH_HELD`]. The bar draws one named entry per held ability, so
    /// this is the count its compile-time width bound is sized against (§11.4).
    pub const MAX_HELD: usize = innate_count() + Self::MAX_TECH_HELD;

    /// Whether this ability is **innate** (§8.3) — always in the loadout, never
    /// drawn or found. Run is the only innate *economy* ability (Move/Wait/Takedown/
    /// Drag are the turn loop's own verbs, not deck abilities). The innate set is
    /// what a quick-play loadout starts with before its tech grant (#244).
    pub const fn is_innate(self) -> bool {
        matches!(self, AbilityId::Run)
    }

    /// The ability's display name (§8.3) — the identity the settled §11.6 hotkey
    /// map ([`ability_hotkey`]) is keyed by, so a name and its key stay one fact.
    /// This is the **full** name: the help panel, the messages and the level-seed
    /// string all speak it. The ability bar has a row to fit and speaks the short
    /// [`bar_name`](Self::bar_name) instead.
    pub fn name(self) -> &'static str {
        match self {
            AbilityId::Run => "Run",
            AbilityId::Camouflage => "Camouflage",
            AbilityId::Decoy => "Decoy",
            AbilityId::Dephase => "Dephase",
            AbilityId::Autodoors => "Autodoors",
            AbilityId::Confusion => "Confusion",
            AbilityId::Vision => "Vision",
            AbilityId::PierceWall => "Pierce Wall",
        }
    }

    /// The ability's **bar display name** (§11.4) — the short label the always-on
    /// ability bar draws, one per held ability, alongside its state notation.
    ///
    /// Short because the row is: [`MAX_HELD`](Self::MAX_HELD) entries across a
    /// 40-wide board (§10.2), each carrying up to a `/45/` or an `(on)`, leaves about
    /// nine cells an entry. Every name here is a plain word a player can say out loud
    /// — `Camo`, `Phase`, `Doors`, `Daze`, `Sight` — not an abbreviation to decode,
    /// and each pairs with the full §8.3 [`name`](Self::name) on the help panel's
    /// Legend card, which is also where the hotkey is read off. A name that would
    /// overflow the row fails the **build**, not the frame (see [`MAX_BAR_ENTRY`] and
    /// the render's bound).
    pub const fn bar_name(self) -> &'static str {
        match self {
            AbilityId::Run => "Run",
            AbilityId::Camouflage => "Camo",
            AbilityId::Decoy => "Decoy",
            AbilityId::Dephase => "Phase",
            AbilityId::Autodoors => "Doors",
            AbilityId::Confusion => "Daze",
            AbilityId::Vision => "Sight",
            AbilityId::PierceWall => "Bore",
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

    /// This ability's static definition (§8.1): how it is paid for, and its
    /// behaviour. The catalog is `const` data — declaring a new ability is adding a
    /// row here (§8.1), not writing a system. `const` so the display can measure the
    /// catalog's own numbers at compile time ([`MAX_STATE_NOTATION`]).
    pub const fn def(self) -> &'static Ability {
        match self {
            AbilityId::Run => &RUN,
            AbilityId::Camouflage => &CAMOUFLAGE,
            AbilityId::Decoy => &DECOY,
            AbilityId::Dephase => &DEPHASE,
            AbilityId::Autodoors => &AUTODOORS,
            AbilityId::Confusion => &CONFUSION,
            AbilityId::Vision => &VISION,
            AbilityId::PierceWall => &PIERCE_WALL,
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
            AbilityId::PierceWall => 7,
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
/// the shareable level-seed token ([`LevelSeed`](crate::LevelSeed)), so a handed-
/// around run reproduces the exact loadout, not just the geometry. Held as a
/// membership mask over [`AbilityId::ALL`] so it stays `Copy` and small.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Loadout {
    present: [bool; AbilityId::ALL.len()],
}

impl Loadout {
    /// The **full** loadout: every ability present, passives included.
    ///
    /// It is **not a loadout a run can hold** — the whole catalog is well over the
    /// [`AbilityId::MAX_HELD`] cap (§8.3), so no preset resolves to it and the
    /// ability bar's compile-time width bound (§11.4) does not cover it. A bar this
    /// long simply truncates, as it does on any oversized hand-built state.
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
    /// canonical order the level-seed token serialises and the deck iterates.
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
    /// Confusion (§8.3, §9, #325): **fired once**, from the cell it is pressed in.
    /// Every guard standing within the blast at that moment is **blinded and frozen**
    /// for [`CONFUSION_DAZE_TURNS`](crate::CONFUSION_DAZE_TURNS) — it does not sense
    /// and does not move — and the blast reaches **through walls** like the guard sense
    /// (§9). A costed panic-buy of time, not a kill: each guard resumes cleanly (its
    /// state and lead paused, not reset) when its own count runs out. The reach is
    /// [`CONFUSION_RADIUS`](crate::CONFUSION_RADIUS), clamped down to what the player
    /// can actually sense.
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
    uses: Option<u32>,
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

    /// How many times this **whole facility** lets the ability be used, or `None`
    /// for the abilities the time economy alone governs (§8.2/#302).
    ///
    /// It is not a resource: there is nothing to spend it on, nothing that refills
    /// it, and no way to earn one back. It composes *with* the clocks rather than
    /// replacing them — an ability may carry a cooldown and a budget both, and the
    /// turn cost is untouched either way (§4.4) — and it exists so an effect too
    /// strong to hand out on a cooldown alone can still ship (#303). Set at level
    /// start from this row and counted down by the [`Deck`]; the level is the only
    /// thing that ever gives one back, by being a new level.
    pub fn uses_per_level(&self) -> Option<u32> {
        self.uses
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
/// [`AbilityMode::Activated`] row below is built from. The time economy alone
/// governs it: no per-level use budget ([`Economy::uses_per_level`] is `None`), which
/// is every ability shipping today.
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
        uses: None,
    })
}

/// An activated ability whose real scarcity is the **per-level use budget**
/// (§8.2/#302) rather than the clock — the shape a row with `uses_per_level` is built
/// from. Held apart from [`activated`] so that declaring a budget is a different
/// keystroke from forgetting one: a row either says a number here or does not have
/// the field at all.
const fn budgeted(
    cost: u32,
    targeting: TargetingMode,
    duration: u32,
    cooldown: u32,
    uses: u32,
) -> AbilityMode {
    AbilityMode::Activated(Economy {
        cost,
        targeting,
        duration,
        cooldown,
        uses: Some(uses),
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
// Confusion [START] (§8.3, §9, #240/#325): a blind-and-freeze blast around the
// player, through walls — powerful, so the cost is a *large* lockout that keeps it
// rare (§2.3/§13.2). It is **instant** (`duration: 0`), the second such ability after
// Pierce Wall: it fires once from the cell it is pressed in, dazes the guards standing
// inside [`CONFUSION_RADIUS`] then and there, and goes straight to its cooldown. There
// is no window to switch off, and nothing to carry — the time it buys runs down on the
// guards' own counters ([`CONFUSION_DAZE_TURNS`](crate::CONFUSION_DAZE_TURNS)), which
// is what makes it a panic-buy of time rather than a mobile shield (§8.3).
//
// The felt length is unchanged from the six-turn window it replaces: the guards get
// exactly the turns the player used to, only on their own clock. The cooldown is what
// still makes spending it a real decision.
const CONFUSION: Ability = Ability {
    id: AbilityId::Confusion,
    mode: activated(1, TargetingMode::Itself, 0, 45),
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
// Pierce Wall [START] (§8.3/§8.4, #303): **instant** — `duration: 0`, so it resolves
// the turn it is pressed and there is nothing to switch off — and **no cooldown at
// all**. The clock is deliberately empty here because the scarcity is the budget:
// three holes a facility, and no fourth however long you wait (§8.2/#302). Adding a
// cooldown on top would only blur which number the player is actually managing, and
// the ability paces itself anyway — a bore consumes the very wall the precondition
// needed, so the next one is always a walk away.
//
// It is the first **[`Behaviour::Coded`]** ability, which is §8.1's own prescription
// rather than a shortcut: turning a solid into floor is not a primitive the effect
// vocabulary has, and it is a genuine one-off — no second ability would ever declare
// it — so it takes the escape hatch instead of widening the vocabulary for one row.
// The economy does not care: it reads only the numbers, so this steps through
// activation and its budget exactly as a data ability does.
const PIERCE_WALL: Ability = Ability {
    id: AbilityId::PierceWall,
    mode: budgeted(1, TargetingMode::Itself, 0, 0, PIERCE_WALL_USES),
    behaviour: Behaviour::Coded,
};

/// How many walls one facility lets you bore through — **[START]** (§8.3/#303).
///
/// Three is enough to be a plan (break a dead end, cut to a console the partition put
/// a long way round, open a room-to-room route) and few enough that spending one is a
/// decision you feel. It is the balance lever this ability is tuned on, and §8.2's
/// fence keeps it a single digit.
pub const PIERCE_WALL_USES: u32 = 3;

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

    /// Project the economy slot onto the display [`AbilityState`] the bar reads
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
    /// never listed on the ability bar, never activates (a key press is the free
    /// §4.4 no-op), and reads as [`Unusable`](AbilityState::Unusable). For the full
    /// loadout — every bare run today — the deck behaves exactly as before.
    loadout: Loadout,
    /// Uses left this **level** for each ability that declares a budget
    /// ([`Economy::uses_per_level`], §8.2/#302), `None` for the ones the clocks alone
    /// govern. Seeded from the catalog when the deck is built — which is once, at
    /// level start — decremented on each use that actually happens, and never
    /// written upward by anything: a fresh level is the only way a budget comes back,
    /// and it comes back by being a fresh deck.
    uses: [Option<u32>; AbilityId::ALL.len()],
}

impl Deck {
    /// A fresh deck holding `loadout`, every held ability [`Ready`](Slot::Ready)
    /// (§8.3 — the starting set is available from the start, #104) and every use
    /// budget full (§8.2/#302 — this constructor *is* the level-start boot path).
    /// Abilities not in the loadout are inert (see [`Deck::loadout`]).
    pub(crate) fn new(loadout: Loadout) -> Self {
        let mut uses = [None; AbilityId::ALL.len()];
        for id in AbilityId::ALL {
            uses[id.index()] = id.def().economy().and_then(|e| e.uses_per_level());
        }
        Deck {
            slots: [Slot::Ready; AbilityId::ALL.len()],
            loadout,
            uses,
        }
    }

    /// The economy state of `id`, as the bar reads it (§11.4). An ability the run
    /// does not hold (not in the loadout, #244) reads as
    /// [`Unusable`](AbilityState::Unusable) — it is real but not yours. A held
    /// **passive** reads as [`Passive`](AbilityState::Passive) (#264): holding it is
    /// the whole of its state, so its slot is never consulted.
    ///
    /// **The clock and the budget are ranked, so they cannot contradict each other**
    /// (§8.2/#302). Each state reports the number that actually governs the ability
    /// right now (§8.2's timing rule), in this order:
    ///
    /// - **Active wins outright.** The effect is *running*, and how long it has left
    ///   is the fact the player is playing off. A budget spent down to nothing does
    ///   not hide the window it just bought.
    /// - Otherwise a **spent budget** is [`Exhausted`](AbilityState::Exhausted), over
    ///   a cooldown — a cooldown on an ability that is never usable again is a
    ///   countdown to nothing, and reporting it would promise a return that is not
    ///   coming.
    /// - Otherwise the **cooldown** leads while it runs: it is the nearer gate, and
    ///   the uses behind it are still there when it clears.
    /// - Otherwise the budget is what stands between the player and the next use
    ///   ([`Limited`](AbilityState::Limited)) — or nothing does ([`Ready`]).
    ///
    /// [`Ready`]: AbilityState::Ready
    pub(crate) fn state(&self, id: AbilityId) -> AbilityState {
        if !self.loadout.contains(id) {
            return AbilityState::Unusable;
        }
        if id.is_passive() {
            return AbilityState::Passive;
        }
        let uses = self.uses[id.index()];
        match (self.slots[id.index()].display(), uses) {
            (active @ AbilityState::Active { .. }, _) => active,
            (_, Some(0)) => AbilityState::Exhausted,
            (AbilityState::Ready, Some(uses)) => AbilityState::Limited { uses },
            (state, _) => state,
        }
    }

    /// Uses left this level for `id` (§8.2/#302), or `None` for an ability with no
    /// budget — the number the near line and a test read, and the same number
    /// [`state`](Deck::state) shows on the bar.
    pub(crate) fn uses_left(&self, id: AbilityId) -> Option<u32> {
        self.uses[id.index()]
    }

    /// The run's loadout — the abilities it holds (#244), for the UI line to list
    /// and a test to assert what was resolved.
    pub(crate) fn loadout(&self) -> Loadout {
        self.loadout
    }

    /// Activate `id` if the run holds it and it is [`Ready`](Slot::Ready). Returns
    /// whether it activated — `true` means the turn is spent (§4.4). Activating an
    /// ability that is not in the loadout (#244), a **passive** (#264 — there is no
    /// activation path to take), one already active or cooling, or one whose
    /// per-level budget is spent (#302), is a mis-input: a **free** no-op (`false`),
    /// like bumping a wall (§4.4).
    ///
    /// A use is spent **only when the ability actually switches on** — the decrement
    /// is the last thing here, after every refusal has had its say, so a press that
    /// changed nothing costs nothing. Callers that gate on a target of their own
    /// (the decoy's faced cell, §8.4) refuse *before* reaching this, so their
    /// refusals cost neither a use nor the turn either.
    pub(crate) fn activate(&mut self, id: AbilityId) -> bool {
        if !self.loadout.contains(id) {
            return false;
        }
        let Some(economy) = id.def().economy() else {
            return false; // a passive is already on; there is nothing to switch
        };
        if self.uses[id.index()] == Some(0) {
            return false; // the level's supply is spent, and nothing refills it
        }
        let slot = &mut self.slots[id.index()];
        if *slot != Slot::Ready {
            return false;
        }
        *slot = Slot::activated(economy.duration, economy.cooldown);
        if let Some(left) = &mut self.uses[id.index()] {
            *left -= 1; // the `Some(0)` guard above is what makes this safe
        }
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
        assert_eq!(AbilityState::Passive.suffix(), "(on)");
        assert_eq!(AbilityState::Unusable.suffix(), "—");
    }

    /// The **use-budget notation** (§8.2/#302), pinned: a count in parentheses, the
    /// shape [`PASSIVE_MARKER`] uses — because neither is a timer — and never a
    /// clock's brackets or slashes. Exhausted borrows the unusable dash rather than
    /// inventing a `(0)`: `(0)` would read as a number you still have.
    #[test]
    fn a_use_budget_reads_as_a_count_not_a_countdown() {
        assert_eq!(AbilityState::Limited { uses: 3 }.suffix(), "(3)");
        assert_eq!(AbilityState::Limited { uses: 1 }.suffix(), "(1)");
        assert_eq!(
            AbilityState::Exhausted.suffix(),
            AbilityState::Unusable.suffix(),
            "spent draws as unusable, because that is what it is",
        );
        assert_ne!(AbilityState::Limited { uses: 0 }.suffix(), "—");

        // The states themselves stay distinct, so nothing can quietly treat a
        // budgeted ability as an unbounded one or a spent one as merely cooling.
        assert_ne!(AbilityState::Limited { uses: 2 }, AbilityState::Ready);
        assert_ne!(AbilityState::Exhausted, AbilityState::Unusable);
        assert_ne!(AbilityState::Exhausted, AbilityState::Ready);
        for n in 0..4 {
            assert_ne!(
                AbilityState::Exhausted,
                AbilityState::Cooling { remaining: n }
            );
            assert_ne!(
                AbilityState::Limited { uses: n },
                AbilityState::Active { remaining: n }
            );
        }
    }

    /// A budgeted entry draws its count against the bar name, exactly as the ticket's
    /// worked example reads — `Bore(2)` — and the widest one a legal budget can
    /// produce still fits the per-entry budget (§11.4). The single-digit fence is a
    /// `const` assertion over the catalog; this pins what that fence buys.
    #[test]
    fn a_budgeted_bar_entry_fits_the_row() {
        let entry = |id, state| AbilityStatus { id, state }.bar_entry();
        assert_eq!(
            entry(AbilityId::Decoy, AbilityState::Limited { uses: 2 }),
            "Decoy(2)"
        );
        assert_eq!(entry(AbilityId::Run, AbilityState::Exhausted), "Run—");
        for id in AbilityId::ALL {
            let widest = entry(id, AbilityState::Limited { uses: 9 });
            assert!(
                widest.chars().count() <= MAX_BAR_ENTRY,
                "{widest:?} overflows the per-entry budget",
            );
        }
    }

    /// A bar entry is the ability's **bar name** with the notation tucked straight
    /// against it (§11.4) — a ready ability is the bare name, with no trailing
    /// bracket or space to pay for. The name comes from the identity, so the entry
    /// is built from an [`AbilityId`].
    #[test]
    fn a_bar_entry_is_the_bar_name_and_any_notation() {
        let entry = |id, state| AbilityStatus { id, state }.bar_entry();
        assert_eq!(entry(AbilityId::Run, AbilityState::Ready), "Run");
        assert_eq!(
            entry(AbilityId::Camouflage, AbilityState::Active { remaining: 7 }),
            "Camo[7]"
        );
        assert_eq!(
            entry(AbilityId::Decoy, AbilityState::Cooling { remaining: 12 }),
            "Decoy/12/"
        );
        assert_eq!(
            entry(AbilityId::Autodoors, AbilityState::Unusable),
            "Doors—"
        );
    }

    /// A passive reads `(on)` where an activated ability reads its clock (#264/#287)
    /// — the marker #264 deferred to this rework. Undecorated it would have sat on
    /// the bar looking exactly like the ready abilities beside it, which is the one
    /// thing it is not: there is nothing to press.
    ///
    /// What had to survive that decision is the *state*: `Passive` is still its own
    /// case, never `Active { .. }` — so nothing can start showing a countdown for
    /// an ability that never counts down.
    #[test]
    fn a_passive_reads_as_always_on_and_is_still_its_own_state() {
        assert_eq!(AbilityState::Passive.suffix(), PASSIVE_MARKER);
        let status = AbilityStatus {
            id: AbilityId::Vision,
            state: AbilityState::Passive,
        };
        assert_eq!(status.bar_entry(), "Sight(on)");
        // Marked, never the same *state* as Ready or a clock.
        assert_ne!(AbilityState::Passive, AbilityState::Ready);
        for n in 0..4 {
            assert_ne!(AbilityState::Passive, AbilityState::Active { remaining: n });
            assert_ne!(
                AbilityState::Passive,
                AbilityState::Cooling { remaining: n }
            );
        }
    }

    /// **The bar's width budget, as arithmetic** (§11.4/#287). The widest notation is
    /// read off the catalog's own numbers — the longest `[N]`/`/N/` any §8.3 ability
    /// can show, against the passive marker — and the widest entry is that plus the
    /// longest bar name, so a retune or a rename moves these rather than silently
    /// overflowing the row. The render turns them into a `const` assertion against
    /// the board width; this pins the values that assertion is made of.
    #[test]
    fn the_bar_budget_is_measured_from_the_catalog() {
        // The longest number in the catalog is Confusion's 45 → `/45/`, exactly as
        // wide as the passive `(on)`.
        assert_eq!(MAX_STATE_NOTATION, 4);
        assert_eq!(PASSIVE_MARKER.len(), MAX_STATE_NOTATION);
        // The longest bar name is five (`Decoy`/`Phase`/`Doors`/`Sight`).
        assert_eq!(max_bar_name(), 5);
        assert_eq!(MAX_BAR_ENTRY, 9);
        // No ability, in any state its own mode can reach, draws wider than that.
        for id in AbilityId::ALL {
            let mut states = vec![AbilityState::Unusable];
            match id.def().mode() {
                AbilityMode::Passive => states.push(AbilityState::Passive),
                AbilityMode::Activated(economy) => states.extend([
                    AbilityState::Ready,
                    AbilityState::Active {
                        remaining: economy.duration(),
                    },
                    AbilityState::Cooling {
                        remaining: economy.cooldown(),
                    },
                ]),
            }
            for state in states {
                let entry = AbilityStatus { id, state }.bar_entry();
                assert!(
                    entry.chars().count() <= MAX_BAR_ENTRY,
                    "{entry:?} overflows the per-entry budget",
                );
            }
        }
    }

    /// The held-set cap (§8.3/#244/#266), the other half of the budget: innate Run
    /// plus [`AbilityId::MAX_TECH_HELD`] tech. Counted off the catalog, so promoting
    /// an ability to innate moves it rather than leaving a stale number behind.
    #[test]
    fn the_held_cap_is_the_innate_set_plus_the_tech_grant() {
        assert_eq!(AbilityId::MAX_TECH_HELD, 3);
        assert_eq!(AbilityId::MAX_HELD, 4);
        assert_eq!(
            innate_count(),
            AbilityId::ALL.iter().filter(|id| id.is_innate()).count(),
        );
        assert!(
            AbilityId::MAX_TECH_HELD <= AbilityId::TECH.len(),
            "the grant cannot exceed the pool it draws from",
        );
    }

    /// An entry's hotkey and name are the **explicit** §11.6 identity, taken from
    /// the [`AbilityId`] — not its position. Were they derived from list order,
    /// reordering the bar would shuffle the keys (the regression §11.6 rules out).
    #[test]
    fn an_entry_takes_its_hotkey_and_name_from_its_identity() {
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
            // Instant since #325 — the blast fires once and the guards carry the
            // time it bought, so there is no player-side window here to state.
            (
                AbilityId::Confusion,
                1,
                TargetingMode::Itself,
                0,
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

    /// The **coded** catalog (§8.1's escape hatch, #303), pinned separately because
    /// it is the arm the other pin cannot reach: Pierce Wall declares no effects at
    /// all, so its row is `cost 1`, self-targeted, **instant** (`duration: 0`), with
    /// **no cooldown** and a per-level budget instead — the scarcity is the budget,
    /// not the clock (§8.2/#302). Every number here is [START].
    #[test]
    fn the_catalog_matches_the_design_coded() {
        let def = AbilityId::PierceWall.def();
        let economy = def.economy().expect("Pierce Wall is activated");
        assert_eq!(economy.cost(), 1, "activation costs the turn (§4.4)");
        assert_eq!(economy.targeting(), TargetingMode::Itself);
        assert_eq!(economy.duration(), 0, "instant — no window to manage");
        assert_eq!(
            economy.cooldown(),
            0,
            "no clock: the budget is the scarcity"
        );
        assert_eq!(economy.uses_per_level(), Some(PIERCE_WALL_USES));
        assert_eq!(PIERCE_WALL_USES, 3, "[START]");
        assert!(
            matches!(def.behaviour(), Behaviour::Coded),
            "turning a solid into floor is not a primitive (§8.1)",
        );
        assert_eq!(AbilityId::PierceWall.name(), "Pierce Wall");
        assert_eq!(
            AbilityId::PierceWall.bar_name(),
            "Bore",
            "§11.4 fits 5 cells"
        );
        assert_eq!(AbilityId::PierceWall.hotkey(), 'b');
    }

    /// **Every** activated ability is pinned by one of the catalog tests — the
    /// guard against a row being added and quietly escaping the value-by-value pin,
    /// which a hand-written list of tuples otherwise invites.
    #[test]
    fn every_activated_ability_is_pinned_by_a_catalog_test() {
        for id in AbilityId::ALL.into_iter().filter(|id| !id.is_passive()) {
            let pinned = match id.def().behaviour() {
                // The data rows are covered by `..._activated`, which walks a literal
                // list; a row missing from it fails here rather than silently.
                Behaviour::Effects(_) => PINNED_ACTIVATED.contains(&id),
                // The coded rows are covered one by one by `..._coded`.
                Behaviour::Coded => id == AbilityId::PierceWall,
            };
            assert!(pinned, "{} is in no catalog pin", id.name());
        }
    }

    /// The data-driven rows [`the_catalog_matches_the_design_activated`] walks — kept
    /// beside it so the completeness check above reads off the same list.
    const PINNED_ACTIVATED: [AbilityId; 6] = [
        AbilityId::Run,
        AbilityId::Camouflage,
        AbilityId::Decoy,
        AbilityId::Dephase,
        AbilityId::Autodoors,
        AbilityId::Confusion,
    ];

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

    /// A fresh deck is available from the start (§8.3: the v1 set is), in whichever
    /// way each ability *is* available — plain [`Ready`](AbilityState::Ready), or a
    /// **full budget** for one that carries one (§8.2/#302), or [`Passive`] for one
    /// that is simply on because it is held (#264). None of the three is a lockout,
    /// which is the property this pins.
    ///
    /// [`Passive`]: AbilityState::Passive
    #[test]
    fn a_fresh_deck_is_all_ready() {
        let deck = Deck::new(Loadout::full());
        for id in AbilityId::ALL {
            let expected = match id.def().mode() {
                AbilityMode::Passive => AbilityState::Passive,
                AbilityMode::Activated(economy) => match economy.uses_per_level() {
                    Some(uses) => AbilityState::Limited { uses },
                    None => AbilityState::Ready,
                },
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
    /// number the bar shows before the first end-of-turn tick (§8.2 timing).
    #[test]
    fn activation_sets_the_full_duration() {
        let mut deck = Deck::new(Loadout::full());
        assert!(deck.activate(AbilityId::Dephase));
        assert_eq!(
            deck.state(AbilityId::Dephase),
            AbilityState::Active { remaining: 3 },
            "the bar shows the full duration, not duration − 1",
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
            // Available again — for a budgeted ability that means one use lighter
            // (§8.2/#302), which is the budget doing its job, not the clock failing.
            let expected = match economy.uses_per_level() {
                Some(uses) => AbilityState::Limited { uses: uses - 1 },
                None => AbilityState::Ready,
            };
            assert_eq!(deck.state(id), expected, "{}", id.name());
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

    // -----------------------------------------------------------------------
    // The per-level use budget (§8.2/#302)
    // -----------------------------------------------------------------------

    /// A deck in which `id` carries a per-level budget of `uses` — byte for byte the
    /// runtime a catalog row declaring [`Economy::uses_per_level`] produces, seeded
    /// here by hand because **no shipping row declares one yet**: #302 lands the axis
    /// and #303 is the ability that spends it. Everything else is exactly
    /// [`Deck::new`]'s deck, so these tests exercise the real activate/tick/state
    /// paths rather than a parallel model of them.
    fn deck_budgeting(loadout: Loadout, id: AbilityId, uses: u32) -> Deck {
        let mut deck = Deck::new(loadout);
        deck.uses[id.index()] = Some(uses);
        deck
    }

    /// Run one full lockout so a budgeted ability is ready to be used again.
    fn wait_out_the_lockout(deck: &mut Deck, id: AbilityId) {
        let economy = id.def().economy().expect("activated");
        for _ in 0..(economy.duration() + economy.cooldown()) {
            deck.tick(&mut Vec::new());
        }
    }

    /// A fresh deck seeds **every** budget from the catalog, and only from there
    /// (§8.2/#302). This is the level-start boot path — [`Deck::new`] is called once
    /// per level and nowhere else — so "set at level start" is a property of where
    /// this code lives, and the assertion is that the seeding reads the row rather
    /// than any number written down twice.
    #[test]
    fn a_fresh_deck_seeds_every_use_budget_from_the_catalog() {
        let deck = Deck::new(Loadout::full());
        for id in AbilityId::ALL {
            assert_eq!(
                deck.uses_left(id),
                id.def().economy().and_then(|e| e.uses_per_level()),
                "{}",
                id.name(),
            );
        }
    }

    /// **Uses deplete and never come back** (§8.2/#302's fence). Each use costs one,
    /// the last one leaves the ability [`Exhausted`](AbilityState::Exhausted), and no
    /// amount of time moves it off that: a hundred turns of ticking is a whole level
    /// of waiting, and there is nothing to wait for.
    #[test]
    fn uses_deplete_and_never_recharge_across_a_level() {
        let id = AbilityId::Dephase; // dur 3 / cd 30
        let mut deck = deck_budgeting(Loadout::full(), id, 2);

        assert_eq!(deck.state(id), AbilityState::Limited { uses: 2 });
        assert!(deck.activate(id));
        assert_eq!(deck.uses_left(id), Some(1), "one use spent");
        wait_out_the_lockout(&mut deck, id);
        assert_eq!(
            deck.state(id),
            AbilityState::Limited { uses: 1 },
            "off cooldown, and the budget is what is left to report",
        );

        assert!(deck.activate(id), "the last use is a use like any other");
        assert_eq!(deck.uses_left(id), Some(0));
        wait_out_the_lockout(&mut deck, id);

        assert_eq!(deck.state(id), AbilityState::Exhausted);
        for turn in 0..100 {
            deck.tick(&mut Vec::new());
            assert_eq!(deck.state(id), AbilityState::Exhausted, "turn {turn}");
            assert_eq!(deck.uses_left(id), Some(0), "turn {turn}");
            assert!(!deck.activate(id), "turn {turn}: nothing to activate");
        }
    }

    /// **A use is spent only when the ability actually switches on** (§8.2/#302).
    /// Every refusal the deck can give — not held, already cooling, already spent —
    /// leaves the count exactly where it was, so a mis-pressed key never costs a use
    /// any more than it costs a turn (§4.4).
    #[test]
    fn a_refused_activation_consumes_no_use() {
        let id = AbilityId::Dephase;

        // Refused for want of the ability itself: the run does not hold it.
        let mut ungranted = deck_budgeting(Loadout::innate(), id, 3);
        assert!(!ungranted.activate(id));
        assert_eq!(ungranted.uses_left(id), Some(3), "not yours, and not spent");

        // Refused because it is mid-lockout: one use bought the window, and
        // hammering the key through it buys nothing more.
        let mut deck = deck_budgeting(Loadout::full(), id, 3);
        assert!(deck.activate(id));
        assert_eq!(deck.uses_left(id), Some(2));
        for _ in 0..5 {
            assert!(!deck.activate(id), "already running");
            assert_eq!(deck.uses_left(id), Some(2), "a refused press costs nothing");
        }

        // Refused because the budget is gone: the count cannot go below zero, and
        // pressing again does not try to.
        let mut spent = deck_budgeting(Loadout::full(), id, 1);
        assert!(spent.activate(id));
        wait_out_the_lockout(&mut spent, id);
        assert!(!spent.activate(id));
        assert_eq!(
            spent.uses_left(id),
            Some(0),
            "no underflow, no second spend"
        );
    }

    /// **The two lockouts coexist without contradicting each other** (§8.2/#302).
    /// While the clock runs it is the clock that is reported — it is the nearer gate,
    /// and it is true. The moment the clock clears, the budget takes over. A spent
    /// budget outranks the *cooldown*, because a cooldown on an ability that is never
    /// coming back would be a countdown to nothing — but never the **duration**: the
    /// window your last use bought is still running, and hiding its clock would be
    /// the one lie §8.2's timing rule names.
    #[test]
    fn a_cooldown_and_a_budget_report_the_nearer_gate() {
        let id = AbilityId::Dephase; // dur 3 / cd 30
        let mut deck = deck_budgeting(Loadout::full(), id, 2);

        assert!(deck.activate(id));
        assert_eq!(deck.state(id), AbilityState::Active { remaining: 3 });
        for _ in 0..3 {
            deck.tick(&mut Vec::new());
        }
        assert_eq!(
            deck.state(id),
            AbilityState::Cooling { remaining: 30 },
            "the clock leads while it runs — the budget is not the wait",
        );
        for _ in 0..30 {
            deck.tick(&mut Vec::new());
        }
        assert_eq!(
            deck.state(id),
            AbilityState::Limited { uses: 1 },
            "clock clear, so the budget is what stands between you and the next use",
        );

        // The last use. It spends the budget to zero the instant it is pressed — but
        // the window it bought is running, and that is what the player is playing
        // off, so the duration keeps the entry for as long as it lasts.
        assert!(deck.activate(id));
        assert_eq!(deck.uses_left(id), Some(0));
        assert_eq!(
            deck.state(id),
            AbilityState::Active { remaining: 3 },
            "a spent budget never hides the window it just bought",
        );
        for _ in 0..3 {
            deck.tick(&mut Vec::new());
        }
        assert_eq!(
            deck.state(id),
            AbilityState::Exhausted,
            "spent outranks the cooldown: there is no use left for it to lead to",
        );
    }

    /// A **fresh level** restores the budget (§8.2/#302): the only thing that ever
    /// gives one back is a new deck, and a new deck is what a new level builds.
    /// Nothing inside a level can reach this.
    #[test]
    fn a_fresh_level_restores_the_budget() {
        let id = AbilityId::Dephase;
        let mut deck = deck_budgeting(Loadout::full(), id, 1);
        assert!(deck.activate(id));
        wait_out_the_lockout(&mut deck, id);
        assert_eq!(deck.state(id), AbilityState::Exhausted);

        // The next facility is a new deck off the same catalog row.
        let next = deck_budgeting(Loadout::full(), id, 1);
        assert_eq!(next.state(id), AbilityState::Limited { uses: 1 });
    }

    /// **An ability with no budget behaves exactly as it did before #302.** Every
    /// shipping row is one of these, so this is the compatibility statement: the
    /// states are the clock's alone, `uses_left` is `None`, and no number of
    /// activations ever exhausts anything.
    #[test]
    fn an_unbudgeted_ability_is_untouched_by_the_axis() {
        let unbudgeted = AbilityId::ALL.into_iter().filter(|id| {
            id.def()
                .economy()
                .is_some_and(|e| e.uses_per_level().is_none())
        });
        for id in unbudgeted {
            let mut deck = Deck::new(Loadout::full());
            assert_eq!(deck.uses_left(id), None, "{}", id.name());
            assert_eq!(deck.state(id), AbilityState::Ready, "{}", id.name());
            for _ in 0..3 {
                assert!(deck.activate(id), "{}", id.name());
                wait_out_the_lockout(&mut deck, id);
                assert_eq!(
                    deck.state(id),
                    AbilityState::Ready,
                    "{} is never Limited and never Exhausted",
                    id.name(),
                );
                assert_eq!(deck.uses_left(id), None, "{}", id.name());
            }
        }
    }
}
