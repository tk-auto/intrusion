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
//! **derived from the catalogue's own numbers** ([`MAX_STATE_NOTATION`]) rather than
//! written down, and the render turns the two into a compile-time assertion that the
//! worst-case bar fits the board. Renaming an ability, raising a cooldown past 99,
//! or raising the grant then fails the *build* — never the frame.
//!
//! # Two halves: the economy model and its display
//!
//! The per-ability **runtime economy** (§8.1/§8.2) lives in this module too —
//! [`AbilityId`] and its data-driven [`Ability`] catalogue, the effect vocabulary
//! ([`Effect`]) and the code escape hatch ([`Behaviour`]), and the [`Deck`] the
//! turn loop steps: activation, early toggle-off, and the end-of-turn
//! duration/cooldown tick, with the `duration + cooldown` lockout *emergent* from
//! the rules rather than stored. Alongside the two clocks sits the one non-time
//! axis (§8.2/#302): an optional **per-level use budget**
//! ([`Ability::uses_per_level`]), set at level start, counted down by use, and never
//! given back — scarcity for an effect too strong to hand out on a cooldown alone.
//! It sits on the ability rather than inside the time economy, so a **passive** can
//! declare one too (#243) and be spent by the world rather than by a press
//! ([`Deck::spend_effect`]).
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

use serde::{Deserialize, Serialize};

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
    /// **The tech an open crate is offering** (§8.3/#266) — not held, not a clock, and
    /// the one entry on an exchange's bar row that is not already yours.
    ///
    /// It exists because the exchange is drawn on the ability bar rather than in a
    /// modal of its own ([`State::exchange`](crate::State::exchange)): the row lists the
    /// four candidates and the player presses the one to drop, so exactly one of them
    /// has to read as *the new one*.
    ///
    /// It says so in **colour alone** — Interest, the reward channel of the `¤` it came
    /// out of, against the three Owned entries beside it (§11.2) — and carries no
    /// notation of its own. It wore a `(+)` first, and the mark was redundant the moment
    /// the row started drawing its slot numbers: three cells spent restating what the
    /// colour already says, on the one row where the width is worth spending on the
    /// **keys** instead.
    Offered,
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
    /// [`MAX_STATE_NOTATION`], derived from the catalogue itself.
    pub fn suffix(self) -> String {
        match self {
            AbilityState::Ready => String::new(),
            AbilityState::Active { remaining } => format!("[{remaining}]"),
            AbilityState::Cooling { remaining } => format!("/{remaining}/"),
            AbilityState::Limited { uses } => format!("({uses})"),
            AbilityState::Passive => PASSIVE_MARKER.to_string(),
            // An exchange candidate draws its bare name (#266): the row is a choice, not
            // a readout, and which entry is the crate's is said in colour.
            AbilityState::Offered => String::new(),
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

/// The widest state notation ([`AbilityState::suffix`]) the catalogue can produce, in
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
/// drawn. `const` so [`AbilityId::MAX_HELD`] is arithmetic over the catalogue rather
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

/// Walk the catalogue for the widest notation any ability can show: the biggest
/// number an [`Economy`] carries plus its two delimiters, against the passive
/// marker's own width. `const` so [`MAX_STATE_NOTATION`] is a compile-time fact.
const fn max_state_notation() -> usize {
    let mut widest = PASSIVE_MARKER.len();
    let mut i = 0;
    while i < AbilityId::ALL.len() {
        let def = AbilityId::ALL[i].def();
        match def.mode {
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
            }
        }
        // A use budget is a third number the bar can show — `(N)` (#302) — so it is
        // measured here like the two clocks, and a budget wide enough to push the row
        // over fails the build rather than the frame. Measured off the row itself
        // rather than off the economy inside it, because **either** mode may carry one
        // (#243): a budgeted passive shows `(1)` where an unbudgeted one shows `(on)`.
        if let Some(uses) = def.uses {
            let notation = decimal_width(uses) + 2;
            if notation > widest {
                widest = notation;
            }
        }
        i += 1;
    }
    widest
}

/// The longest [`AbilityId::bar_name`], in cells. Byte length **is** cell width
/// because the names are ASCII, which [`bar_names_are_ascii`] pins right below.
pub(crate) const fn max_bar_name() -> usize {
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
        if let Some(uses) = AbilityId::ALL[i].def().uses {
            if uses == 0 || uses > 9 {
                return false;
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
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
    /// Salvaged tech (§8.3/#243), **passive and budgeted**: the one guard that lays
    /// hands on you this facility goes down instead of taking you.
    Saver,
    /// Salvaged tech (§8.3/#273): launch a drone and **fly it yourself** — then let go
    /// and leave it watching for the rest of the window.
    Drone,
    /// Salvaged tech (§8.3/#504): forge control's own call — every guard in reach is
    /// sent to search **the cell you fired from**.
    FalseCall,
    /// Salvaged tech (§8.3/#505), **passive**: a compass to the nearest unclaimed
    /// objective, painted as one of the eight cells around you.
    Guide,
    /// Salvaged tech (§8.3/#239), **on trial**: a dart fired along the cardinal
    /// you face, taking down the first unaware guard on the line.
    Dart,
}

impl AbilityId {
    /// Every economy-governed ability, in the fixed deck-slot order. The order is
    /// display/iteration order — it is what [`Loadout::iter`] filters, so it decides
    /// which bar slot a held ability lands in and therefore which digit fires it
    /// (§11.6/#359) — and it *is* the order [`index`](Self::index) pins, so the two
    /// must not drift.
    pub const ALL: [AbilityId; 14] = [
        AbilityId::Run,
        AbilityId::Camouflage,
        AbilityId::Decoy,
        AbilityId::Dephase,
        AbilityId::Autodoors,
        AbilityId::Confusion,
        AbilityId::Vision,
        AbilityId::PierceWall,
        AbilityId::Lockdown,
        AbilityId::Saver,
        AbilityId::Drone,
        AbilityId::FalseCall,
        AbilityId::Guide,
        AbilityId::Dart,
    ];

    /// The **salvaged-tech** abilities (§8.3) — the found-in-the-facility set, as
    /// opposed to innate [`Run`](AbilityId::Run), in their **permanent slot order**:
    /// the order the level-seed token encodes a held set against (#333), so entries are
    /// appended and never moved or removed.
    ///
    /// This is also the eligible pool a `starting_abilities` grant (#244) draws from
    /// and an equipment cache is stocked out of (#209) — the roster and the pool are
    /// one list, **with nothing held out of it**. There is no experimental tier (§0/#564):
    /// an ability either ships, in which case it is here and gets drawn, played and
    /// measured like every other, or it does not exist — a shipped ability nobody can
    /// draw is inert (§2.3). Scepticism about one lives in its prose and in the sim's
    /// numbers, never in an exclusion from this list.
    /// Quick play grants the whole pool while its size meets the grant count;
    /// the draw only bites once the pool outgrows the grant. A passive (#264) is drawn
    /// from here like any other tech — it competes for the same slot, which is exactly
    /// what it pays with.
    pub const TECH: [AbilityId; 13] = [
        AbilityId::Camouflage,
        AbilityId::Decoy,
        AbilityId::Dephase,
        AbilityId::Autodoors,
        AbilityId::Confusion,
        AbilityId::Vision,
        AbilityId::PierceWall,
        AbilityId::Lockdown,
        AbilityId::Saver,
        AbilityId::Drone,
        AbilityId::FalseCall,
        AbilityId::Guide,
        AbilityId::Dart,
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
    pub const fn name(self) -> &'static str {
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
            AbilityId::Saver => "Saver",
            AbilityId::Drone => "Drone",
            // Two words, on Phase Out's precedent, and the pairing is the design
            // (§8.3/#504): the full name says the transmission is **forged**, the bar
            // name says what it *does*. A name suggesting bait would be wrong — bait is
            // the Decoy's job, and the two are complements rather than variants.
            AbilityId::FalseCall => "False Call",
            AbilityId::Guide => "Guide",
            // One word, and the shortest name in the catalogue: what it is, and nothing
            // about what it does to a guard. The §7.2 verb is called a *takedown*, and
            // naming this one after the reach — "remote takedown" — would have advertised
            // the thing §7.2 keeps **[SETTLED]** as adjacent-only, on the bar, every
            // frame. The dart is the object you fire; whether it takes anything down is
            // the line's business.
            AbilityId::Dart => "Dart",
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
            AbilityId::Saver => "Saver",
            AbilityId::Drone => "Drone",
            AbilityId::FalseCall => "Call",
            AbilityId::Guide => "Guide",
            AbilityId::Dart => "Dart",
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
            AbilityId::Saver => {
                "The next guard to lay hands on you goes down instead of taking you, \
                 leaving a body. Once a facility, and then never again."
            }
            AbilityId::Drone => {
                "Fly a drone while your body stands still. Press again to let go: it \
                 hovers on, watching, till the window ends. Guards never see it."
            }
            AbilityId::FalseCall => {
                "Forges a call naming your cell: every guard in reach goes there and \
                 searches. Be elsewhere by then; a dead radio spoofs nothing."
            }
            AbilityId::Guide => {
                "Washes the neighbouring cell lying toward the nearest thing left to \
                 take. A bearing as the crow flies — it will point through walls."
            }
            // It states the **aim and the gate**, in that order, because those are the
            // two things a player has to get right and neither is on the board: the
            // dart goes where you are already facing, and it only drops a guard that
            // has not seen you. The miss is spelled out too — the one sentence that
            // stops the first firing feeling like a bug (§8.4: it is never refused for
            // want of a target).
            AbilityId::Dart => {
                "Fires the way you face. The first guard on the line drops if it has not \
                 seen you. A shot that finds nobody is spent too."
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
    /// behaviour. The catalogue is `const` data — declaring a new ability is adding a
    /// row here (§8.1), not writing a system. `const` so the display can measure the
    /// catalogue's own numbers at compile time ([`MAX_STATE_NOTATION`]).
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
            AbilityId::Saver => &SAVER,
            AbilityId::Drone => &DRONE,
            AbilityId::FalseCall => &FALSE_CALL,
            AbilityId::Guide => &GUIDE,
            AbilityId::Dart => &DART,
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
            AbilityId::Saver => 9,
            AbilityId::Drone => 10,
            AbilityId::FalseCall => 11,
            AbilityId::Guide => 12,
            AbilityId::Dart => 13,
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
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
    /// It is **not a loadout a run can hold** — the whole catalogue is well over the
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

    /// The same loadout with `id` **gone** — the exchange's half of [`with`](Self::with)
    /// (§8.3/#266): what a run gives up to make room for what a crate is offering.
    ///
    /// The one way a loadout shrinks, and it is deliberately as blunt as `with`: it
    /// removes what it is told to and asks nothing about caps or innateness, because
    /// *which* ability may be traded away is the exchange's question
    /// ([`Exchange`](crate::Exchange)) and not the set's. A no-op for an ability the
    /// loadout does not hold.
    #[must_use]
    pub fn without(mut self, id: AbilityId) -> Self {
        self.present[id.index()] = false;
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
    /// Saver (§8.3, §4.5, #243): while in effect, a guard's **capturing step**
    /// (§4.5 — contact, the only loss condition) is turned into a **takedown of that
    /// guard** (§7.2) and the run goes on. The attacker alone: a second guard reaching
    /// the player the same turn captures them as it always did.
    ///
    /// The one declared exception to a **[SETTLED]** rule, so what bounds it is not a
    /// clock but the level itself — the ability is passive with a per-level use budget
    /// (§8.2/#302), and the moment it fires is the moment it is spent. It leaves the
    /// §7.2 body and starts the §7.3 clock like any other takedown, because it *is*
    /// one: surviving still costs a body, an aware guard's noise, and a radio silence
    /// coming due.
    ReverseCapture,
    /// False Call (§8.3/§7.7, #504): **fired once**, from the cell it is pressed in.
    /// A forged control transmission naming that cell — every guard within
    /// [`FALSE_CALL_RADIUS`](crate::FALSE_CALL_RADIUS) of it, reaching **through
    /// walls** like the guard sense (§9) and clamped down to what the player can
    /// actually sense, is **called to it and searches** (§7.6) exactly as a real call's
    /// responder does.
    ///
    /// It adds **no second verb** to §7.7. Cooperation has exactly one — *a call sends
    /// guards to search a cell* — and this hands the player that one rather than
    /// inventing another; the world change runs through the very seam control's own
    /// dispatch and both call-ins run through
    /// ([`send_call`](crate::State::send_call)). The radius belongs to the **player's
    /// transmitter**, not to control's net: §7.7's "no radio range" is a standing rule
    /// about the facility's own calls, and nothing here weakens it.
    ///
    /// Like [`Confuse`](Effect::Confuse) the set is decided at the firing and the cell
    /// named is a **snapshot** — walking away does not move the destination, which is
    /// the whole play: you call them here, and then you are not here.
    FakeCall,
    /// Guide (§8.3/§11.5a, #505): while held, one of the eight cells around the player
    /// is washed [`Effect`](crate::Category::Effect) — the one lying in the direction of
    /// the nearest **unclaimed objective** (an intel console or an equipment cache).
    ///
    /// **A compass, not a route.** The bearing is taken as the crow flies, with no
    /// regard for walls, doors or reachability, and that restraint is the whole design:
    /// a guide that pathed would answer §10's exploration outright and turn a facility
    /// into a corridor to follow. It tells you *which way* and nothing else — expect it
    /// to point straight through a wall, often; that is the tool working.
    ///
    /// **It reveals nothing.** The objective's cell, glyph and distance stay fogged
    /// until seen (§11.5a **[SETTLED]**); what the player gains is an eighth of a
    /// circle. Anything that reveals more belongs to §12.6's `full_layout_known` or to
    /// #215's v3 intel sink, which sells exactly that.
    ObjectiveBearing,
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
/// Built as `const` catalogue rows ([`AbilityId::def`]). Every number is `[START]`
/// (§8.3) — tunable, and pinned by a test so a change is a visible decision.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ability {
    id: AbilityId,
    mode: AbilityMode,
    uses: Option<u32>,
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

    /// How many times this **whole facility** lets the ability be used, or `None`
    /// for the abilities their [`mode`](Ability::mode) alone governs (§8.2/#302).
    ///
    /// It is not a resource: there is nothing to spend it on, nothing that refills
    /// it, and no way to earn one back. It exists so an effect too strong to hand out
    /// on a cooldown alone can still ship (#303). Set at level start from this row and
    /// counted down by the [`Deck`]; the level is the only thing that ever gives one
    /// back, by being a new level.
    ///
    /// **It sits beside the mode rather than inside it** (#243). §8.2 says the budget
    /// "composes with the time economy, it does not replace it" — it is the one
    /// *non-time* axis, and an axis that composes with the clocks is not a field of
    /// them. Kept inside [`Economy`] it was structurally an activated ability's
    /// privilege, which made a budgeted **passive** unrepresentable rather than merely
    /// unbuilt. Out here either mode may declare one, and neither has to know the
    /// other can.
    pub fn uses_per_level(&self) -> Option<u32> {
        self.uses
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

// The §8.3 starting-set catalogue. All numbers `[START]` (§8.3), pinned by
// `the_catalog_matches_the_design`. Effects are declared here and applied by each
// ability's own ticket; the economy is blind to them.

/// An activated ability's §8.2 economy, as one `const` expression — the shape every
/// [`AbilityMode::Activated`] row below is built from.
///
/// It states the **time** economy and only that. Whether the row also carries a
/// per-level use budget is [`Ability::uses`]'s to say (§8.2/#302/#243), and every
/// row states that field explicitly — a budget is never inherited from a
/// constructor's default, so declaring one stays a different keystroke from
/// forgetting one.
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
    uses: None,
    behaviour: Behaviour::Effects(&[Effect::ExtraStep]),
};
const CAMOUFLAGE: Ability = Ability {
    id: AbilityId::Camouflage,
    mode: activated(1, TargetingMode::Itself, 10, 20),
    uses: None,
    behaviour: Behaviour::Effects(&[Effect::ConcealWhileStill]),
};
const DECOY: Ability = Ability {
    id: AbilityId::Decoy,
    mode: activated(1, TargetingMode::Direction, 20, 30),
    uses: None,
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
    uses: None,
    behaviour: Behaviour::Effects(&[Effect::Phase]),
};
// Autodoors [START] (§8.3): a long active window — enough to walk a whole stretch
// of corridor door-to-door through a chase — paid for by a long lockout before the
// next flight (§7.6). Self-target toggle; free to cancel (§4.4).
const AUTODOORS: Ability = Ability {
    id: AbilityId::Autodoors,
    mode: activated(1, TargetingMode::Itself, 16, 40),
    uses: None,
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
    uses: None,
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
    uses: None,
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
    mode: activated(1, TargetingMode::Itself, 0, 0),
    uses: Some(PIERCE_WALL_USES),
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
    uses: None,
    behaviour: Behaviour::Effects(&[Effect::SealDoors]),
};

// Saver [START] (§4.5/§8.2/§8.3, #243): **on trial** — a costed exception to
// §4.5's [SETTLED] "a guard touches you … that is the only loss condition". The guard
// that lays hands on you is taken down instead, once, and the run continues.
//
// **Passive with a budget, and the pair is the whole design.** The ticket proposed a
// toggle on a short duration against a very long cooldown; both halves of that shape
// fight the ability. A defensive toggle has to be *predicted* — you spend a turn
// standing still, in the moment a guard is closing, on a guess about the next three
// turns — and §8.2's timing trap says the activation turn must be protected, which is
// a trap you can only fall into if there is an activation turn at all. There is none
// here: held is on (§8.2/#264), so the protection is never mistimed and there is no
// turn spent buying it. What replaces the cooldown is the scarcer bound the same
// section already offers — **one use per facility** (§8.2/#302) — which is stricter
// than any cooldown can be: a lockout ends, and this does not (appendix 43).
//
// **What it costs.** The loadout slot, permanently (§8.2's answer for every passive)
// — this is one of three, held instead of a flight tool for every other crisis of the
// run — and then the body. A save leaves a guard on the floor where you were standing,
// in a cell a guard was walking to, with the §7.3 clock started and its colleagues
// nearby; §7.2's economy is that a takedown you cannot hide is a takedown that finds
// you later. So the good player's "when would I not use this" is answered for them —
// they never choose to spend it — and the real decision moved one level up, to whether
// a slot is worth an insurance policy that fires once.
//
// It is **drawn like any other tech** (§8.3): quick play can deal it and a crate can
// hold it, so the §4.5 exception is measured in the runs players actually get rather
// than only in the ones that ask for it by name. That is the experiment (§14) — trial,
// measure, promote or reject — and the sim is what watches whether a run whose worst
// moment is survivable once has stopped taking capture seriously (§13.2/§13.4).
//
// **If it proves too strong, [`SAVER_USES`] is already at its floor.** The two levers
// are on the effect rather than the budget, and appendix 43 weighs them: leave the
// player **stunned for N turns** after the save (§8.3's eject stun — so being caught
// still costs the position it was caught in), or **stun the guard rather than take it
// down**, which turns one capture refunded into one capture deferred and gives up the
// free body with it.
const SAVER: Ability = Ability {
    id: AbilityId::Saver,
    mode: AbilityMode::Passive,
    uses: Some(SAVER_USES),
    behaviour: Behaviour::Effects(&[Effect::ReverseCapture]),
};

// Drone [START] (§8.1/§8.3/#273): the second **[`Behaviour::Coded`]** ability, and the
// one §8.1 names when it reserves the hatch (*"piloting a drone"*). What it does is not
// an effect on the player's body at all — it changes **who the keys are for** — so no
// arrangement of the effect vocabulary could express it without inventing a primitive
// for one row, which is exactly the DSL-rot §8.1 warns against.
//
// **One duration for both halves, and that is the design** (#273). Activating launches
// the drone and hands it the controls; pressing again hands them back, **free** (§4.4)
// — and the drone stays out there, hovering, feeding you its camera for whatever is
// left of the window. So the 40 turns are not "40 turns of flying": they are 40 turns
// of *machine*, and how much of that you spend flying versus watching is the whole
// decision. Two clocks would have made that a pair of numbers the player has to add up,
// and the bar can honestly show only one of them (§8.2's timing rule).
//
// **40 rather than the 30 it opened at**, retuned on the first play-through: 30 buys
// enough machine to fly somewhere *or* to leave a camera behind, and not enough to do
// both — which collapses the decision the single clock exists to create. The lockout
// absorbs the extra turns whole.
//
// **What it costs** (§2.3). Every turn you fly is a turn your body stands still in a
// patrolled building while you look somewhere else: the guard phase runs (§4.2), capture
// is contact (§4.5), and a patrol walking into your parked body ends the run while you
// are watching a corridor two rooms away. Scouting deep costs exactly as many turns of
// blind exposure as it buys of vision — which is why the tuning lever is the clock and
// never the drone's invulnerability. The good player's "when would I not use this" is
// therefore answered by geometry: you fly from somewhere nobody walks, and if you have
// nowhere like that, you do not fly.
//
// The lockout is 40 + 40 = **80**, comfortably the longest in the catalogue, because
// information is the thing §11.5a most deliberately withholds and a run that could
// re-scout every twenty turns would never have to plan under fog at all.
const DRONE: Ability = Ability {
    id: AbilityId::Drone,
    mode: activated(1, TargetingMode::Itself, 40, 40),
    uses: None,
    behaviour: Behaviour::Coded,
};

// False Call [START] (§7.7/§8.3, #504): the player's half of the radio. **Instant**
// (`duration: 0`), Confusion's and Pierce Wall's shape — it fires once from the cell it
// is pressed in and goes straight to its lockout, because what it does is send a
// message and a message is over the moment it is sent. There is no window to switch
// off and nothing to carry: what it bought runs on the responders' own legs.
//
// **What it costs, and when a good player declines it** (§2.3). The turn, the 30-turn
// lockout — and the cell. The call names where you *are*, so its whole value is in the
// turns after it: fire it and stand still and you have summoned a search onto your own
// feet, and a search flushes a hideout (§10.3/§7.6), so the cupboard in the corner is
// not the answer either. A good player declines it whenever they have nowhere to be
// next — pulling four guards into the room you are still crossing is strictly worse
// than the patrol you had. That the value is *elsewhere* is the design, and it is why
// the near line words it as a warning rather than as a confirmation.
//
// **30, against Confusion's 45.** It buys less than a daze does — the guards keep
// walking, keep looking, and arrive — so it is not priced like the panic-buy. It is
// still long enough that emptying a wing stays a plan rather than a habit, which is the
// §7.7 pressure the design says the difficulty lives in.
//
// Data-driven rather than [`Behaviour::Coded`], for Lockdown's reason: calling an
// area's guards is the same shape as freezing an area's guards and sealing an area's
// doors, so it is one more row in the vocabulary (§8.1) and not an escape hatch.
const FALSE_CALL: Ability = Ability {
    id: AbilityId::FalseCall,
    mode: activated(1, TargetingMode::Itself, 0, 30),
    uses: None,
    behaviour: Behaviour::Effects(&[Effect::FakeCall]),
};

// Guide [START] (§8.3/§11.5a, #505): the **second passive** after Vision — no
// activation, no turn, no cooldown, in effect for as long as it is held. Its whole price
// is the loadout slot (§8.2/#264), and unlike Vision it is not obviously worth one,
// which is the interesting part: it changes what you *know* rather than what you can
// *do*, and a run that always knows which way to walk still has to get there.
//
// **What it costs, and when a good player declines it** (§2.3). The slot, against a
// flight tool for the moment it all goes wrong — and a compass is worth least exactly
// when a run is going badly, because knowing the bearing to a console does not help a
// player being chased away from it. The other cost is subtler and is the one to watch:
// a bearing plus §11.5a's always-visible geometry may be enough to walk more or less
// straight to every console, which would delete the exploration the fog exists to
// create. If it does, the answer is a **range cap** — a compass that only wakes within N
// cells is a local tool rather than a global one — and not a nerf to the wash.
//
// **Deliberately not a pathfinder**, and the first bug report will say it points into a
// wall. That is the specification: §7.3's "nearest means the shortest walk" is control
// routing a guard, and this is a needle pointing. Nobody should "fix" it into a
// pathfind later ([`State::guide_bearing`] states it again at the seam).
//
// Data-driven rather than [`Behaviour::Coded`]: a standing wash over a derived cell is
// an effect like any other, and the layer already knows how to draw one.
const GUIDE: Ability = Ability {
    id: AbilityId::Guide,
    mode: AbilityMode::Passive,
    uses: None,
    behaviour: Behaviour::Effects(&[Effect::ObjectiveBearing]),
};

// Dart [START] (§7.2/§8.3/§8.4, #239): **on trial** — a takedown at *range*, and
// therefore a deliberate reopening of the ability that broke the old game (§2.3: *"the
// neutralise ability … unlimited range, no cooldown, and it did not consume a turn"*).
// It exists on trial, with every safeguard §2.3 asks for stacked on it at once, and the
// sim is what decides whether it survives (§14).
//
// **Aiming by facing is the safeguard the cursor was not.** The §2.3 failure was
// *auto-target-nearest-visible* — an ability that asked nothing at all of where the
// player stood. A cursor would have asked for two keypresses. This asks you to **be in
// the corridor, on the line, pointing the right way, and unseen**, which is paid for in
// movement, exposure and turns, on the board, where the guards can punish it. So the aim
// is [`TargetingMode::Direction`] — Decoy's mode, and the first use of it as a **ray**
// rather than as the single faced cell — and there is no target list anywhere in the
// implementation to snap to (§8.4/appendix 1).
//
// **Instant** (`duration: 0`) and **with no cooldown at all**, which is Pierce Wall's
// row exactly — and it is the one place this deliberately does not do what #239 asked
// for, so the reasoning is here rather than in the ticket.
//
// The ticket lists a *"very large cooldown, exaggerated on purpose"* among the §2.3
// safeguards, and then asks in the same breath that every cost be **shown to the
// player**, predicting the bar will read `(1)` and then `—`. Those two cannot both
// happen. With [`DART_USES`] at 1 the budget always bites first: [`Deck::state`] ranks a
// spent budget *above* a running cooldown — deliberately, because *"a cooldown on an
// ability that is never usable again is a countdown to nothing"* — so no `/60/` is ever
// drawn. Nor does one fit: the help panel's economy line has 34 cells and
// `1 turn · instant · 60 cooling · 1 a level` needs 41. Three surfaces say
// independently that there is one number here, and §8.2 agrees: the budget is the *one
// non-time axis*, and Pierce Wall's row already argued this exact case — *"adding a
// cooldown on top would only blur which number the player is actually managing."*
//
// **The safeguard is not dropped, it is met by something stronger.** Appendix 43 makes
// the argument for the Saver and it holds verbatim here: one use per facility *"is
// stricter than any cooldown can be: a lockout ends, and this does not."* A 60-turn
// lockout would let a 200-turn run fire three darts; `1/level` lets it fire one, for
// ever. So the clock is empty because the scarcity has moved somewhere a clock cannot
// reach — see appendix 54, which records this reversal so the next person to ask *"why
// has the most dangerous ability in the game no cooldown?"* does not have to re-derive it.
//
// - **1 turn**, like every activation (§4.4).
// - **No cooldown** — the budget is the whole economy.
// - **[`DART_USES`] = 1 per level** (§8.2/#302) — *"the one thing 'no charges' rules out
//   that the game needs, for an effect too strong to hand out on a cooldown alone."*
//   That sentence was written for exactly this ability; Pierce Wall's `3/level` is the
//   precedent and this is the floor beneath it.
//
// **[`Behaviour::Coded`]**, the third such ability, on Pierce Wall's own grounds rather
// than as a shortcut: a projectile that walks a line and takes a guard down at the end of
// it is not a primitive the effect vocabulary has, and it is a genuine one-off — no
// second ability would ever fire a dart — so it takes §8.1's escape hatch instead of
// widening the vocabulary for one row. The economy does not care: it reads only the
// numbers, so this steps through activation and its budget exactly as a data row does.
//
// **What it costs, and when a good player declines it** (§2.3). The turn, the level's
// only dart — and the **body**, which is the counterweight that does the real work
// (§7.3). A dart drops a guard *where it stood*, which is usually several cells away down
// a corridor you were not planning to walk: you often cannot reach it to stow it (§7.2),
// so what the shot buys is a guard gone and a find waiting to happen on a radio clock you
// cannot silence. A good player declines it whenever the guard was going to walk past
// anyway — which is most patrols — and spends it on the one watcher that genuinely cannot
// be gone round.
const DART: Ability = Ability {
    id: AbilityId::Dart,
    mode: activated(1, TargetingMode::Direction, 0, 0),
    uses: Some(DART_USES),
    behaviour: Behaviour::Coded,
};

/// How many darts one facility gives you — **[START]** (§7.2/§8.2/§8.3/#239).
///
/// **One**, which is §8.2's floor and the whole reason the ability is filable at all.
/// A ranged takedown is the ability §2.3 records as having *been* the old game, so the
/// bound is not a dial that happens to be low: it is the statement that a facility allows
/// exactly one guard to be removed at a distance, ever, and the rest of the building has
/// to be played.
///
/// **It is the ability's whole economy**, the row carrying no cooldown at all (see
/// [`DART`] and appendix 54), so this number is also the only thing between the player and
/// a second ranged takedown. §8.2's fence would permit up to nine; **two is where the
/// argument has to be made**, not where a tune quietly lands, because a second dart is a
/// second guard removed from a building that only ever had a handful — and with no clock in
/// the row the two could be fired on consecutive turns. Every value above one goes past the
/// sim first (§13.2), and past the kill-thresholds on
/// `docs/stats/abilities/dart.md`.
pub const DART_USES: u32 = 1;

/// How many captures one facility lets you walk away from — **[START]** (§4.5/§8.3/#243).
///
/// **One**, and the number is the design rather than a dial that happens to be low. Two
/// would make being caught a thing that *happens* on the way to somewhere, which is the
/// §2.3 failure ("being seen was free") transposed onto capture; one keeps a run's
/// single worst moment survivable exactly once and leaves the next one lethal. §8.2's
/// fence would allow up to nine, and every value above one should be argued for against
/// the sim before it is tried.
pub const SAVER_USES: u32 = 1;

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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
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
/// retuning a catalogue value moves the lockout with it and nothing here needs to
/// change (§8.2). For v1 the whole set is available from the start (#104): a fresh
/// deck is all [`Ready`](Slot::Ready).
///
/// [`activate`]: Deck::activate
/// [`deactivate`]: Deck::deactivate
/// [`tick`]: Deck::tick
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
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
    /// govern. Seeded from the catalogue when the deck is built — which is once, at
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
            uses[id.index()] = id.def().uses_per_level();
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
        let uses = self.uses[id.index()];
        if id.is_passive() {
            // A passive with a **budget** (#243) is still a passive — held is on — but
            // holding it is no longer the whole of its state: what it has left is the
            // fact the player is playing off, so it reads `(N)` and then `—`, never a
            // standing `(on)` that would promise a rescue already spent. The
            // parenthetical is the same shape either way, which is the point: both
            // numbers mean *not a timer*.
            return match uses {
                None => AbilityState::Passive,
                Some(0) => AbilityState::Exhausted,
                Some(uses) => AbilityState::Limited { uses },
            };
        }
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

    /// Whether `id`'s **per-level budget is short** of what the level granted
    /// (§8.2/#302) — the question a duplicate crate asks (#266): is there anything here
    /// worth taking, for a run that already holds this tech?
    ///
    /// `false` for an ability with no budget, and for one that has spent none of it:
    /// in both cases a second copy would restore nothing, and a bump that changes
    /// nothing must cost nothing (§4.4).
    pub(crate) fn uses_spent(&self, id: AbilityId) -> bool {
        matches!(
            (self.uses[id.index()], id.def().uses_per_level()),
            (Some(left), Some(granted)) if left < granted
        )
    }

    /// **Refill `id`'s per-level budget** from its own §8.3 row (§8.2/#302/#266) — what
    /// a crate holding tech the run already carries pays out.
    ///
    /// This is the one and only way a budget ever goes back up, and §8.2's fence is
    /// otherwise unmoved: nothing regenerates, nothing ticks it up, no console tops it
    /// up, and it is still a bound on the facility rather than a resource to manage.
    /// What it takes is **another copy of the tool itself**, found in a crate that is
    /// then spent — bounded by how many crates the building hides, which is the same
    /// scarcity the §14 v3 flavour already sets.
    ///
    /// Back to **full**, not up by one: the level's grant is the number the row states,
    /// and a partial refill would put a second number on the same axis for no gain.
    pub(crate) fn recharge(&mut self, id: AbilityId) {
        if let Some(granted) = id.def().uses_per_level() {
            self.uses[id.index()] = Some(granted);
        }
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
    /// slots and use budgets were seeded for the whole catalogue in
    /// [`new`](Deck::new) — every ability starts [`Ready`](Slot::Ready) with its budget
    /// full, held or not — so what was missing was never state, it was **permission**:
    /// [`state`](Deck::state) and [`activate`](Deck::activate) both refuse an ability the
    /// loadout does not contain. Granting it lifts exactly that, which is why this
    /// touches nothing else: a salvaged ability arrives ready, with its full per-level
    /// budget (§8.2/#302), and rebuilding the deck to achieve that would instead reset
    /// the clocks and budgets of everything the run was already carrying.
    ///
    /// Idempotent. Nothing here enforces the §8.3 held cap, and that is deliberate: the
    /// cap is kept at the **crate**, where a full run is offered the exchange instead
    /// (#266, [`Exchange`](crate::Exchange)) — that is the one moment the player can be
    /// told. A silent cap in here would drop a find nobody was warned about, which is
    /// the failure the exchange exists to avoid.
    pub(crate) fn grant(&mut self, id: AbilityId) {
        self.loadout = self.loadout.with(id);
    }

    /// **Trade `id` away** (§8.3/#266): drop it from the loadout, so the run stops
    /// holding it from this turn on — the exchange's half of [`grant`](Deck::grant).
    ///
    /// It **switches the ability off first**, and that is the whole subtlety here: an
    /// activated ability is *in effect* by its slot alone ([`in_effect`](Deck::in_effect)),
    /// not by loadout membership, so an ability dropped mid-window would keep running
    /// with nothing on the bar to say so. Ending it first is the same free toggle-off
    /// §8.2 already defines, and it refunds nothing — the cooldown it leaves behind
    /// simply belongs to an ability the run no longer has.
    ///
    /// The slot and the per-level budget are otherwise **left where they are**. Dropping
    /// an ability is not a way to launder its clocks: a run that trades tech away and
    /// finds the same tech again later picks it up exactly as cool — or as spent — as it
    /// put it down, which is what stops drop-and-refind being a free recharge.
    ///
    /// The state a *world* has to unwind with it — a decoy standing on the board, a
    /// lockdown's seals — is the turn loop's, not the deck's, and rides the same path
    /// an early toggle-off takes ([`State::step`](crate::State::step)).
    pub(crate) fn revoke(&mut self, id: AbilityId) {
        self.deactivate(id);
        self.loadout = self.loadout.without(id);
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
    ///
    /// "Always" for a passive means *while the level still allows it* (#243): a
    /// budgeted passive whose supply is spent is [`Exhausted`](AbilityState::Exhausted)
    /// on the bar, and an ability that reads as unusable must not still be quietly
    /// doing its work. So the same `Some(0)` that greys the entry is what stops the
    /// effect, and no caller has to remember to check both.
    fn in_effect(&self, id: AbilityId) -> bool {
        if id.is_passive() {
            return self.loadout.contains(id) && self.uses[id.index()] != Some(0);
        }
        // Deliberately *not* applied to an activated ability, on
        // [`state`](Deck::state)'s own ranking: an active window keeps running after
        // the use that bought it was the level's last, because the effect is what the
        // use was spent *on*. Only a passive — which has no window to be inside — is
        // switched off by an empty budget.
        matches!(self.slots[id.index()], Slot::Active { .. })
    }

    /// **Spend the effect the world just triggered** (§8.2/#302/#243): if any ability
    /// in effect declares `effect`, consume one of its per-level uses and report that
    /// it fired.
    ///
    /// The counterpart to [`activate`](Deck::activate) for a budget that is *not*
    /// spent by a press. A passive has no activation moment to charge (§8.2/#264), so
    /// for a budgeted one the moment its effect is actually called upon is the only
    /// honest place to take the use — and that moment belongs to the turn loop, not to
    /// the keyboard.
    ///
    /// Keyed on the **effect**, never on an identity, exactly as
    /// [`effect_active`](Deck::effect_active) is: the loop interprets §8.1's
    /// vocabulary, so a second ability that one day declares the same effect is
    /// charged for it on the same rules and the trigger site needs no new arm. An
    /// ability with no budget simply fires and is charged nothing, which is what an
    /// unbudgeted passive already means.
    pub(crate) fn spend_effect(&mut self, effect: Effect) -> bool {
        let Some(id) = AbilityId::ALL.into_iter().find(|id| {
            self.in_effect(*id)
                && matches!(id.def().behaviour(), Behaviour::Effects(effects) if effects.contains(&effect))
        }) else {
            return false;
        };
        if let Some(left) = &mut self.uses[id.index()] {
            *left -= 1; // `in_effect` refuses a spent budget, so this cannot underflow
        }
        true
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
