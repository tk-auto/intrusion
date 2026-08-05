//! The ability bar's display vocabulary (§11.4) — **what the always-on ability
//! bar says**, assembled from the run's real ability economy.
//!
//! §11.4's settled answer to §15 Q9 is one always-on **named bar**: every ability
//! the run holds, drawn by its short bar name with its state notation tucked
//! against it, driven by real per-ability runtime and *actionable* — a tap resolves
//! to the ability under it and activates it exactly as that slot's digit would
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
//! - **Keys come from the bar's own slots** (§11.6/#359): `1`–`4` fire its first
//!   through fourth entries, so the four keys a run can press stay four however far
//!   the catalogue grows past them. The order the bar draws is therefore load-bearing
//!   — it is what the keyboard names — and a tap and a digit both resolve through
//!   [`ability_in_slot`](crate::ability_in_slot), so they cannot disagree. The bar
//!   draws no key at all: the help panel's Abilities tab is where a player reads the
//!   pairing off (§15 Q9, #343). The identity→letter map lives on as the replay
//!   script's spelling ([`ability_script_letter`](crate::replay::ability_script_letter)).
//! - **The number shown is the number the player gets** (§8.2 timing): the bar
//!   formats exactly the value it is handed and advertises nothing else, so it
//!   cannot re-introduce the old advertised-vs-real discrepancy.

use crate::replay::ability_script_letter;

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
/// [`State::ability_statuses`](crate::State::ability_statuses), in the fixed order
/// the bar draws — which is also the order the §11.6 digits fire (#359), so this
/// list *is* the keyboard's slots as well as the row's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AbilityStatus {
    /// The ability this entry is for — the identity a tap or a digit resolves to and
    /// activates (§11.4), and the source of its name.
    pub id: AbilityId,
    /// What state the ability is in right now (§11.4).
    pub state: AbilityState,
}

impl AbilityStatus {
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
    /// Salvaged tech (§8.3/#242): seal the doors around you for a window — guards
    /// cannot work them, you still can.
    Lockdown,
}

impl AbilityId {
    /// Every economy-governed ability, in the fixed deck-slot order. The order is
    /// display/iteration order — it is what [`Loadout::iter`] filters, so it decides
    /// which bar slot a held ability lands in and therefore which digit fires it
    /// (§11.6/#359) — and it *is* the order [`index`](Self::index) pins, so the two
    /// must not drift.
    pub const ALL: [AbilityId; 9] = [
        AbilityId::Run,
        AbilityId::Camouflage,
        AbilityId::Decoy,
        AbilityId::Dephase,
        AbilityId::Autodoors,
        AbilityId::Confusion,
        AbilityId::Vision,
        AbilityId::PierceWall,
        AbilityId::Lockdown,
    ];

    /// The **salvaged-tech** abilities (§8.3) — the found-in-the-facility set, as
    /// opposed to innate [`Run`](AbilityId::Run). This is the default eligible pool
    /// a `starting_abilities` grant (#244) draws from: the shipped, non-experimental
    /// tech (the gated experiments #239/#243 are not economy abilities yet, so the
    /// pool is exactly the rows listed here). Quick play grants the whole pool while its size
    /// meets the grant count; the draw only bites once the pool outgrows the grant.
    /// A passive (#264) is drawn from here like any other tech — it competes for the
    /// same slot, which is exactly what it pays with.
    pub const TECH: [AbilityId; 8] = [
        AbilityId::Camouflage,
        AbilityId::Decoy,
        AbilityId::Dephase,
        AbilityId::Autodoors,
        AbilityId::Confusion,
        AbilityId::Vision,
        AbilityId::PierceWall,
        AbilityId::Lockdown,
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

    /// The ability's display name (§8.3) — the identity the replay script's letter
    /// map ([`ability_script_letter`]) is keyed by, so a name and its spelling stay
    /// one fact.
    /// This is the **full** name: the help panel, the messages and the level-seed
    /// string all speak it. The ability bar has a row to fit and speaks the short
    /// [`bar_name`](Self::bar_name) instead.
    pub fn name(self) -> &'static str {
        match self {
            AbilityId::Run => "Run",
            AbilityId::Camouflage => "Camouflage",
            AbilityId::Decoy => "Decoy",
            AbilityId::Dephase => "Phase Out",
            AbilityId::Autodoors => "Autodoors",
            AbilityId::Confusion => "Confusion",
            AbilityId::Vision => "Vision",
            AbilityId::PierceWall => "Pierce Wall",
            AbilityId::Lockdown => "Lockdown",
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
    /// Abilities tab, which is also where the key is read off. A name that would
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
            AbilityId::Lockdown => "Lock",
        }
    }

    /// What the ability actually **does**, in a sentence or three — the prose the
    /// help panel's Abilities tab reads out (§11.4/#343).
    ///
    /// A run is handed three tech it has never seen (§8.3), and until this existed
    /// the only way to learn Dephase from Confusion was to spend a turn firing one
    /// and watch. It lives here, beside [`name`](Self::name) and
    /// [`bar_name`](Self::bar_name), for the §11.3 reason every other help row
    /// derives from its source: an exhaustive match means a new ability cannot be
    /// added without one, and there is no help-side table to drift from.
    ///
    /// **It states behaviour, never numbers.** The turn cost, duration, cooldown and
    /// use budget are drawn from the [`Economy`] beside it, so retuning a `[START]`
    /// value updates the panel for free and can never leave the prose lying (§8.2's
    /// standing rule: each surface reports the number the player actually gets).
    pub const fn blurb(self) -> &'static str {
        match self {
            AbilityId::Run => {
                "Two cells a turn instead of one — the one reliable way to outrun a \
                 guard that has already seen you."
            }
            AbilityId::Camouflage => {
                "Undetectable on any turn you do not move. Moving shows you again, \
                 and it never stops a guard's touch."
            }
            AbilityId::Decoy => {
                "A fake intruder in the cell you face. It draws a guard that has lost \
                 you; one that can see you ignores it."
            }
            AbilityId::Dephase => {
                "Walk through walls, doors and guards. Hides nothing; you cannot bump, \
                 so no intel. Ending inside a solid throws you out, stunned."
            }
            AbilityId::Autodoors => {
                "Doors open as you step into them and shut behind you — a shut door \
                 breaks the sightline and costs a chaser a turn."
            }
            AbilityId::Confusion => {
                "Fires once. Every guard you can sense in the blast is blinded and \
                 frozen for a few turns, walls no object."
            }
            AbilityId::Vision => {
                "Your sight is the full 360° and reaches further, for as long as you \
                 hold it. The guard sense is unchanged."
            }
            AbilityId::PierceWall => {
                "Bores through your one adjacent wall for good. Needs exactly one wall \
                 neighbour — a corridor or a corner will not do."
            }
            AbilityId::Lockdown => {
                "Seals the doors near you for a while. Guards cannot work a sealed \
                 door and route the long way round; you still open yours."
            }
        }
    }

    /// Whether this ability is **passive** (#264) — always on while held, with no
    /// activation path and no clock ([`Ability::is_passive`]).
    pub fn is_passive(self) -> bool {
        self.def().is_passive()
    }

    /// The ability's **replay script** letter (§12.4), through the one explicit
    /// identity map ([`ability_script_letter`]) — a fact about the ability, so a
    /// stored script reads the same in every run. It is no longer a keyboard binding:
    /// the keys are the bar's slots (§11.6/#359).
    pub fn script_letter(self) -> char {
        ability_script_letter(self.name()).expect("every ability has a script letter")
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
            AbilityId::Lockdown => &LOCKDOWN,
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
            AbilityId::Lockdown => 8,
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Loadout {
    /// `Default` is the **empty** set, matching [`empty`](Loadout::empty) — a loadout is
    /// built up, never carved down, so the neutral value is *nothing held* rather than
    /// the innate set. It is what makes [`RunStats`](crate::RunStats) derive `Default`
    /// with an empty salvage haul (#209).
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

    /// How many pieces of **salvaged tech** the loadout holds (§8.3) — the innate set
    /// does not count, because it is never found, never drawn and never traded.
    ///
    /// This is the number [`AbilityId::MAX_TECH_HELD`] caps, and the one an equipment
    /// cache's bump checks before handing anything over (#209/#266).
    pub fn tech_held(self) -> usize {
        self.iter().filter(|id| !id.is_innate()).count()
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
    /// Lockdown (§8.3, §10.4, #242): while active, every door within
    /// [`LOCKDOWN_RADIUS`](crate::LOCKDOWN_RADIUS) of the cell it fired on is **shut
    /// and sealed** — a guard cannot work the handle, so its route goes the long way
    /// round (§7.6). The player can still bump one open, because it is their lock;
    /// doing so spends a turn and hands the door to whoever is behind them. Every seal
    /// is released when the window closes (§8.2), which is what keeps a temporary wall
    /// from ever becoming the permanent one §2.2/§7.2 forbid.
    ///
    /// Unlike [`Confuse`](Effect::Confuse), the set is a **snapshot** taken where the
    /// ability fired: a door does not unseal because you walked away from it, or the
    /// wall you raised behind you would dissolve exactly as you fled down it.
    SealDoors,
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
// Dephase [START] (§8.3, #449): the window is **4**, counting the activation that
// opens it — so a phase begun on open floor buys three steps into a solid, which is
// the depth the ability is actually for. It was 3 (two steps in), and the third step
// is the smaller half of the change: the same arithmetic that lets you reach further
// lets the safety eject throw you further back, and the stun is as long as the throw
// (appendix 12). One more turn of reach is therefore also one more turn of worst-case
// helplessness — the §2.3 trade an ability is supposed to carry. The lockout stays at
// 30: two knobs moved at once is neither knob measured.
const DEPHASE: Ability = Ability {
    id: AbilityId::Dephase,
    mode: activated(1, TargetingMode::Itself, 4, 30),
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

// Lockdown [START] (§8.3/§10.4, #242): shut and seal the doors around you, then hand
// them all back. A **short** window against a **long** lockout, because what it buys is
// not a hiding place but a detour — a pursuer that has to route around a sealed doorway
// loses the turns the long way costs, and once you have those turns the wall has done
// its work. Eight turns is enough to spend them; the 40-turn lockout is what makes
// spending it a decision rather than a habit (§2.3).
//
// **The cost is not only the lockout.** The seal is a snapshot around *you*, so it
// takes the doors ahead as readily as the ones behind: fire it in a junction you still
// have to cross and you have walled your own route, and reopening one costs a turn and
// gives the door to whoever is chasing you. That is the "when would a good player not
// use this" the design asks for — a lockdown is worth its turn when the geometry is
// already behind you, and worth nothing when it is not.
//
// Data-driven, not [`Behaviour::Coded`]: sealing an area's doors is the same shape as
// freezing an area's guards, so it is one more row in the vocabulary (§8.1) rather than
// an escape hatch — the effect table already knew it was coming.
const LOCKDOWN: Ability = Ability {
    id: AbilityId::Lockdown,
    mode: activated(1, TargetingMode::Itself, 8, 40),
    behaviour: Behaviour::Effects(&[Effect::SealDoors]),
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

    /// **Salvage `id` into the run** (§2.2/§8.3/#209): add it to the loadout mid-level,
    /// so an ability found in a crate is usable from the turn the crate was opened.
    ///
    /// The one thing that grows a loadout after boot, and it grows it only. A deck's
    /// slots and use budgets were seeded for the whole catalog in
    /// [`new`](Deck::new) — every ability starts [`Ready`](Slot::Ready) with its budget
    /// full, held or not — so what was missing was never state, it was **permission**:
    /// [`state`](Deck::state) and [`activate`](Deck::activate) both refuse an ability the
    /// loadout does not contain. Granting it lifts exactly that, which is why this
    /// touches nothing else: a salvaged ability arrives ready, with its full per-level
    /// budget (§8.2/#302), and rebuilding the deck to achieve that would instead reset
    /// the clocks and budgets of everything the run was already carrying.
    ///
    /// Idempotent. Nothing here enforces the §8.3 held cap: the cap and its discard
    /// prompt are #266's, and silently dropping a pickup the player was never told about
    /// is the failure that ticket exists to avoid.
    pub(crate) fn grant(&mut self, id: AbilityId) {
        self.loadout = self.loadout.with(id);
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
mod tests;

#[cfg(test)]
mod economy_tests;
