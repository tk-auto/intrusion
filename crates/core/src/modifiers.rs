//! Level modifiers — the §12.6 seam.
//!
//! A **level modifier** is a named toggle or bounded knob that shifts a
//! facility's *baseline* difficulty or rules *before the level begins*. Each one
//! flips a rule an existing system already owns — the §7.6 search, the §11.5
//! danger overlay — rather than adding a parallel one. The whole point is that
//! the coupling stays **visible** (§12.3): every modifier is a field on one plain
//! [`LevelModifiers`] value, resolved once at facility start, and every system
//! reads *that* value — never a global bool queried in ten places.
//!
//! # The source → modifier → config flow
//!
//! The *mechanism* here is shared; the *sources* that turn modifiers on are
//! separate and stack on top of it (§14 v3). There are three intended sources,
//! kept deliberately distinct:
//!
//! - **Choice** (exogenous) — the player's chosen or seeded baseline. The only
//!   source this crate ships; it is what a mode preset (#244) and a shared token
//!   (#245) set.
//! - **Alert** (endogenous) — the campaign alert (#210): a loud raid alerts the ground
//!   ahead of it, and an alerted facility is drawn a harder rule from the same pool the
//!   difficulty axis draws from ([`draw_from_pool`]). It lands in
//!   [`ModifierSources::alert`]; outside a campaign there is no such layer and it is
//!   `None`.
//! - **Flavour** (per-node) — a facility's own character on the campaign map (#207):
//!   an [`Outpost`](crate::Flavour::Outpost) is thin and thinly guarded, a
//!   [`Vault`](crate::Flavour::Vault) rich and watched. It is what makes a branch on
//!   the map a decision rather than a coin flip, and it is *only* the modifier set it
//!   contributes here — there is no second vocabulary for what a facility is.
//!
//! They all resolve into the *same* [`LevelModifiers`] the systems read
//! ([`ModifierSources::resolve`]), so a new source is a new field and a line in
//! `resolve`, never a new difficulty path. Determinism (§12.4) is preserved
//! because the resolved set is plain [`Copy`] data threaded through the same seed
//! and inputs: same seed + same modifiers + same inputs → identical run.
//!
//! # Debug modifiers are a different thing
//!
//! [`DebugModifiers`] lives here as the *contrast*, not as a fourth source: a
//! playtest-only view switch that no rule and no generation seam may read, and that
//! never travels in a level-seed token. See its own documentation for why the two
//! are deliberately kept apart.

use crate::rng::Rng;

/// The exit's **intel gate** (§4.5/§10.2/#244): how much intel a run must hold
/// before the exit will let the player leave.
///
/// A **mode knob**, not a difficulty toggle — the three modes want three
/// different objectives over the *same* generated facility, so the gate is a
/// modifier value rather than one global rule (this is what reconciles the old
/// §4.5-vs-`place.rs` discrepancy). Ordered by the pressure it puts on a run:
/// [`All`](IntelGate::All) is the hardest (the longest exposure), [`None`] the
/// easiest, so the sources compose it *harder-ward* ([`IntelGate::harder_of`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IntelGate {
    /// **Easier.** The exit opens immediately — no intel required. Reserved for
    /// campaign (§14 v3), where intel is currency (§2.2), not an exit key.
    None,
    /// The §4.5 **[START]** baseline and the headless-sim preset (§13.2/§13.3):
    /// the exit opens once **at least one** intel is in hand. One objective is a
    /// complete run; pressing on for more is the aggressive style's trade, not a
    /// requirement. Keeps the sim bot's outcome profile mixed (§13.3) — the
    /// all-intel march pinned it in the facility long enough to be caught nearly
    /// every seed. This is [`Default`], so a hand-built state and the sim play the
    /// unchanged §4.5 game.
    #[default]
    AtLeastOne,
    /// **Harder.** Every objective must be taken before the exit opens — the
    /// complete objective **quick play** (#244) sets: gather all the intel, then
    /// get out (§10.2). The longest exposure, hence the hardest gate.
    All,
}

impl IntelGate {
    /// Compose two gates *harder-ward* — the further-along-the-exposure gate wins,
    /// so sources add pressure and never cancel ([`LevelModifiers::union`]). The
    /// order is `All` > `AtLeastOne` > `None`.
    #[must_use]
    pub fn harder_of(self, other: Self) -> Self {
        self.max(other)
    }

    /// The gate's rank along the exposure axis — higher is harder. Private; the
    /// only comparison callers need is [`harder_of`](Self::harder_of).
    fn rank(self) -> u8 {
        match self {
            IntelGate::None => 0,
            IntelGate::AtLeastOne => 1,
            IntelGate::All => 2,
        }
    }
}

impl PartialOrd for IntelGate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IntelGate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank().cmp(&other.rank())
    }
}

/// The facility's **guard count** (§10.2/#232) — one step either side of the
/// recipe's own number, the two ends of a single bounded knob.
///
/// The §10.2 [START] baseline is four guards, and the `--guards` sweep is roughly
/// linear at 8–10 points of bare win rate per guard with no cliff (appendix 26), so
/// one guard is exactly one difficulty step: [`More`](GuardCount::More) is harder,
/// [`Fewer`](GuardCount::Fewer) easier, and neither is a threshold that turns the
/// game into a different one. The **±1 reach is the whole knob** — the envelope it
/// may move within is [`LevelConfig::GUARDS_MIN`]…[`LevelConfig::GUARDS_MAX`]
/// (`crate::LevelConfig`), and the arithmetic lives there with the recipe rather
/// than here, because how many guards a carve can *seat* is a fact about generation
/// (§10.6), not about the modifier seam.
///
/// **The one knob whose baseline sits in the middle.** [`IntelGate`] is ordered along
/// a single exposure axis with quick play already at its hard end, so composing it
/// harder-ward is all the seam ever needs. This knob's baseline is a *neutral* middle
/// with a departure on each side, which is what lets both ends live in the §12.6
/// directed pool — see [`harder_of`](GuardCount::harder_of) for the composition rule
/// that follows from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GuardCount {
    /// **Easier.** One guard fewer than the recipe asks for — never below
    /// [`LevelConfig::GUARDS_MIN`](crate::LevelConfig::GUARDS_MIN), the floor that
    /// keeps a facility patrolled rather than empty.
    Fewer,
    /// The recipe's own count, unchanged — §10.2's four **[START]** for
    /// [`LevelConfig::V1`](crate::LevelConfig::V1). [`Default`], so a hand-built
    /// state and the sim play the unchanged §10.2 game.
    #[default]
    Baseline,
    /// **Harder.** One guard more — never above
    /// [`LevelConfig::GUARDS_MAX`](crate::LevelConfig::GUARDS_MAX), the cap that
    /// keeps the screen-bound board (§11.4) from being crowded.
    More,
}

impl GuardCount {
    /// Compose two contributions, the [`LevelModifiers::union`] rule for this knob:
    /// **the end that departs from the baseline wins, and pressure breaks a tie.**
    ///
    /// It is the same promise [`IntelGate::harder_of`] makes — *sources add pressure
    /// and never cancel each other* — read against a knob whose baseline is a neutral
    /// middle rather than one end of an axis. Taking the plain maximum would have a
    /// source that asked for **nothing** silently overrule one that asked for fewer
    /// guards, which is not a source adding pressure; it is a source that stayed
    /// quiet being counted as one that objected.
    ///
    /// So: a baseline contribution yields to whichever end its partner names, and
    /// [`More`](Self::More) beats [`Fewer`](Self::Fewer) when two sources genuinely
    /// disagree. The invariant that matters is preserved exactly — **no contribution
    /// can relieve pressure another one asked for**, so the campaign alert (#210)
    /// cannot be talked out of its extra guard by a choice the player made.
    #[must_use]
    pub fn harder_of(self, other: Self) -> Self {
        match (self, other) {
            (Self::Baseline, end) | (end, Self::Baseline) => end,
            (Self::More, _) | (_, Self::More) => Self::More,
            (Self::Fewer, Self::Fewer) => Self::Fewer,
        }
    }
}

/// The facility's **intel count** (§10.2/#207) — one step either side of the recipe's
/// own number, the second bounded knob of [`GuardCount`]'s shape and the campaign's
/// **reward** axis.
///
/// It exists because the facility map (#207) offers a choice between facilities and a
/// choice needs two axes to be a choice: a successor that is only ever *harder* is one
/// no player picks, and one that is only ever *easier* is one they always do. Guards
/// are the risk axis and this is the reward — a [`Vault`](crate::Flavour::Vault) holds
/// one console more than the recipe asks for and posts one more guard over it, an
/// [`Outpost`](crate::Flavour::Outpost) one fewer of each.
///
/// **Which end is "harder" is the opposite of the guard knob's**, and follows from
/// what intel *is* rather than from arithmetic. Under the campaign's
/// [`IntelGate::None`] (§4.5) intel is currency (§2.2), so fewer consoles is a thinner
/// run: [`Fewer`](Self::Fewer) is the harder end and composes as one
/// ([`harder_of`](Self::harder_of)). Under quick play's [`IntelGate::All`] the same
/// step would move the **win condition** instead — which is exactly why this knob is
/// not in the §12.6 directed pool (see [`POOL`]): the difficulty draw would be
/// deciding how much of the game you have to do, under a change that is not about
/// quick play at all.
///
/// The ±1 envelope is [`LevelConfig::INTEL_MIN`]…[`LevelConfig::INTEL_MAX`]
/// (`crate::LevelConfig`), and lives there with the recipe for the same reason the
/// guard envelope does: how many consoles a carve can *seat* is a fact about
/// generation (§10.6), not about this seam.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IntelCount {
    /// **Harder.** One console fewer than the recipe asks for — never below
    /// [`LevelConfig::INTEL_MIN`](crate::LevelConfig::INTEL_MIN), the floor that keeps
    /// a facility worth raiding at all.
    Fewer,
    /// The recipe's own count, unchanged — §10.2's three **[START]** for
    /// [`LevelConfig::V1`](crate::LevelConfig::V1). [`Default`], so a hand-built state,
    /// quick play and the sim all play the unchanged §10.2 game.
    #[default]
    Baseline,
    /// **Easier.** One console more — never above
    /// [`LevelConfig::INTEL_MAX`](crate::LevelConfig::INTEL_MAX), the cap that keeps
    /// the intel pool inside what a 40×40 carve reliably seats.
    More,
}

impl IntelCount {
    /// Compose two contributions **harder-ward**, the [`LevelModifiers::union`] rule
    /// for this knob — [`GuardCount::harder_of`]'s rule with the ends swapped, because
    /// here the *scarce* end is the hard one.
    ///
    /// The invariant is the one that matters everywhere in §12.6: **no contribution can
    /// relieve pressure another one asked for**. A baseline source stayed quiet and
    /// yields to whichever end its partner names; when two sources genuinely disagree,
    /// [`Fewer`](Self::Fewer) wins.
    #[must_use]
    pub fn harder_of(self, other: Self) -> Self {
        match (self, other) {
            (Self::Baseline, end) | (end, Self::Baseline) => end,
            (Self::Fewer, _) | (_, Self::Fewer) => Self::Fewer,
            (Self::More, Self::More) => Self::More,
        }
    }
}

/// How much of the building the player is given **before setting foot in it**
/// (§11.5a/§12.6, #307/#233) — the third bounded knob of [`GuardCount`]'s shape, and
/// the one whose axis is *knowledge* rather than a count.
///
/// §11.5a's baseline hands over the **plans**: the load-bearing fabric of ground you
/// have never had eyes on draws as the schematic `□`, the floor space between it
/// blank, and walking somewhere resolves it into the real building permanently. The
/// knob's two ends move that line in opposite directions — [`Full`](Self::Full) draws
/// the real building where the schematic would stand, [`None`](Self::None) draws
/// nothing at all — so both are departures from one middle, which is why they are one
/// knob rather than two toggles that could contradict each other.
///
/// **The [`None`] end deliberately overrides a [SETTLED] rule**, and that is stated
/// rather than smuggled: §11.5a settles that geometry is *"always visible, from turn
/// one. Never fogged."* precisely so a player can **plan an escape route before being
/// spotted** (§7.6) — *"a player who is chased and improvising in unknown geometry is
/// not playing a stealth game, they're rolling dice."* This end removes that pillar's
/// support on purpose. It is why the rule may only ever be bent by a **modifier**,
/// never by the base game, and why the end sits outside the difficulty draw (see
/// [`POOL`]): it does not add a step of pressure, it hands back a different game.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutKnowledge {
    /// **Easier.** The **full layout** (§11.5a): geometry the player has never had
    /// eyes on draws as the real building rather than as the schematic `□`, so
    /// doorways, duct mouths and furniture are all on the map from turn one. Exactly
    /// the picture the game gave everyone before the schematic landed (#307), which is
    /// what makes this end easy to state and easy to price.
    ///
    /// It reveals the **layout and nothing else**: a console and a cupboard are
    /// contents, still hidden until seen (§11.5a), so this never shortcuts the
    /// scouting that finds the objectives — it removes the *architectural* unknown
    /// only. The knowledge state on the seam stays truthful either way: an unexplored
    /// cell still reports itself unexplored, it is simply drawn in full.
    ///
    /// **The duct mouth is the one content on the plans**, and stays one (#450). It is
    /// a recess cut into the fabric and reads off a drawing the way a doorway does,
    /// which is why this end has always handed it over. #450 made a mouth *remembered*
    /// once scouted — a change to what finding one is worth — and left that alone
    /// deliberately: quietly narrowing an easier-direction modifier is a difficulty
    /// change wearing a render fix's clothes.
    ///
    /// **It has to be paid for.** Route-planning through unscouted wings is a real
    /// advantage, so this end sits on the *easier* side of the §12.6 directed pool:
    /// under the difficulty draw it spends budget that must be found by taking a
    /// harder rule elsewhere. A modifier that only ever gave would not be a modifier.
    Full,
    /// The §11.5a **[SETTLED]** baseline: the building's **plans**, drawn as the
    /// schematic until explored. [`Default`], so a hand-built state, quick play and
    /// the sim all play the unchanged §11.5a game.
    #[default]
    Plans,
    /// **Harder.** No layout at all (#233): a cell the player has never had eyes on
    /// draws as **blank**, and only what has actually been seen is on the board.
    /// Route-planning stops being a first-class activity from turn one and becomes
    /// something exploration has to earn — the schematic's *"you are never lost, never
    /// mapping"* traded away.
    ///
    /// **The fog is total, and it has to be.** §11.5a's masking rule — *everything
    /// unexplored collapses to exactly the same appearance, glyph channel and colour
    /// channel both* — is what stops the fog leaking what it hides, so this end masks
    /// every unexplored cell to bare floor rather than dimming the schematic further.
    /// One appearance, no ink, nothing to read.
    ///
    /// **The exit is the one exception**, on the same footing it keeps everywhere else
    /// (§4.5/§11.5a): the tunnel the player dug and came in by is theirs, drawn as
    /// itself from turn one. It is also the whole reason the run stays playable — with
    /// the building gone, `E` is the one fixed point every escape plan can still be
    /// hung on (§7.6).
    ///
    /// **The §9 guard sense is untouched, and reads oddly on purpose.** A sensed guard
    /// is still a bare position through walls — the sense was never line of sight — so
    /// with this on the player gets a dot floating in blank space and does not know
    /// what stands between them and it. That is the honest picture rather than an
    /// oversight: special-casing the sense here would invent a second knowledge rule
    /// for one channel to paper over the first one's consequences.
    None,
}

impl LayoutKnowledge {
    /// Compose two contributions **harder-ward**, the [`LevelModifiers::union`] rule
    /// for this knob — [`GuardCount::harder_of`]'s rule over a knob whose baseline is
    /// likewise a neutral middle.
    ///
    /// The invariant is §12.6's: **no contribution can relieve pressure another one
    /// asked for.** A source resting at [`Plans`](Self::Plans) stayed quiet and yields
    /// to whichever end its partner names; when two sources genuinely disagree,
    /// [`None`](Self::None) wins, because a source that asked for the layout to be
    /// hidden cannot be talked out of it by one that offered to hand it over.
    #[must_use]
    pub fn harder_of(self, other: Self) -> Self {
        match (self, other) {
            (Self::Plans, end) | (end, Self::Plans) => end,
            (Self::None, _) | (_, Self::None) => Self::None,
            (Self::Full, Self::Full) => Self::Full,
        }
    }
}

/// How many **equipment caches** a facility hides (§2.2/§14 v3/#209) — the campaign's
/// *second* reward axis, and the one the run's power curve is made of.
///
/// A count rather than a toggle because the map's flavours differ in **how much** tech
/// they hold, not merely in whether they hold any: a Depot has one crate, a
/// [`Workshop`](crate::Flavour::Workshop) two, a [`Vault`](crate::Flavour::Vault)
/// three. Bounded at three so the knob is a small enum like its neighbours — a value
/// per rung, each with its own permanent token slot and its own help-card caption —
/// rather than an open number the format would have to find room for.
///
/// **Its baseline is its zero end, and that is the difference from the other two
/// knobs.** [`GuardCount`] and [`IntelCount`] sit at a neutral middle with a departure
/// either side; a facility with no crate in it is not a middle, it is *none*. That
/// makes the §12.6 pressure rule read differently here — see
/// [`most_of`](Self::most_of).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum CacheCount {
    /// No crate at all — every quick-play level and every hand-built state.
    /// [`Default`], because a single facility has no *rest of the run* to accumulate
    /// into (§2.2), and because the game plays as it always did without one.
    #[default]
    None,
    /// One crate: the [`Depot`](crate::Flavour::Depot)'s, and the smallest find a
    /// facility can be worth.
    One,
    /// Two: the [`Workshop`](crate::Flavour::Workshop)'s, which is what that flavour
    /// gives up a console for.
    Two,
    /// Three: the [`Vault`](crate::Flavour::Vault)'s — the richest facility on the map
    /// is richest in both currencies at once, and posts a guard over them.
    Three,
}

impl CacheCount {
    /// The number of crates, as placement counts them.
    pub const fn crates(self) -> usize {
        match self {
            CacheCount::None => 0,
            CacheCount::One => 1,
            CacheCount::Two => 2,
            CacheCount::Three => 3,
        }
    }

    /// The most crates any facility hides — the bound placement is written against and
    /// the ceiling the token's slots cover.
    pub const MAX: usize = 3;

    /// The rung that stands for `crates`, saturating at [`MAX`](Self::MAX).
    ///
    /// The inverse of [`crates`](Self::crates), and it exists for one caller: the boot
    /// path narrows a facility's count to the number of crates the draw could actually
    /// fill ([`start_level_with`](crate::start_level_with)), so what the state carries is
    /// what the building has rather than what its flavour asked for.
    pub const fn for_crates(crates: usize) -> Self {
        match crates {
            0 => CacheCount::None,
            1 => CacheCount::One,
            2 => CacheCount::Two,
            _ => CacheCount::Three,
        }
    }

    /// Compose two contributions: **a source that stayed quiet yields, and the larger
    /// count wins.** [`LevelModifiers::union`]'s rule for this knob.
    ///
    /// It is *not* [`GuardCount::harder_of`]'s rule wearing different words, and the
    /// difference is worth stating because it is the one place §12.6's composition
    /// invariant does not simply apply. That invariant — *no contribution can relieve
    /// pressure another one asked for* — is about **pressure**, and this knob has none
    /// to relieve: a crate is a pure reward, and asking for fewer of them is not a
    /// source adding difficulty, it is a source declining to give.
    ///
    /// Composing the other way would also be unworkable rather than merely odd. This
    /// knob's baseline is its own zero, so *every* source that never mentions caches
    /// names the scarce end — and a harder-ward rule would let the player's own
    /// [`chosen`](ModifierSources::chosen) set, which is silent about crates by
    /// construction, wipe the flavour the map just offered. The rule that actually
    /// holds the §2.3 promise here is the one the map needs: **what a facility was
    /// advertised as holding, it holds.**
    #[must_use]
    pub fn most_of(self, other: Self) -> Self {
        self.max(other)
    }
}

/// The set of level modifiers active for a facility — resolved once at facility
/// start (§12.3) into the one value guards, vision, and render branch on.
///
/// Plain, heterogeneous data: a toggle is a `bool`, a bounded knob is a small
/// enum ([`IntelGate`]). Adding a modifier is adding a field, and the compiler
/// then enumerates every read site that must handle it (§12.2). Every field
/// carries a documented **direction** — *harder* raises pressure, *easier* lowers
/// it — so a directional assertion (§2.3, the anti-facade guard) can prove it
/// bites.
///
/// [`Default`] is the **baseline**: every modifier off and the intel gate at its
/// §4.5 [START] value, the game exactly as it plays without the system. Quick
/// play (#244) is a *named preset* over this — it flips [`intel_to_exit`] to
/// [`IntelGate::All`] and is carried alongside its ability loadout in the
/// shareable level-seed token ([`LevelSeed`](crate::LevelSeed), #245).
///
/// [`intel_to_exit`]: LevelModifiers::intel_to_exit
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelModifiers {
    /// **Harder.** Force the §7.6 search to flush an occupied hideout within its
    /// disc unconditionally — not only a *body* search (§10.3/#219). Baseline, a
    /// guard that merely lost a chase leaves the cupboard the safe wait-out it is
    /// (the "hold still, watch the cone sweep past" payoff, §7.6); with this on,
    /// any active search that sweeps over the cupboard you dived into flushes it.
    pub guards_always_search_hideouts: bool,
    /// **Harder.** A guard that had the player in the **certain** zone (§7.6,
    /// `CERTAIN_RANGE`) and then loses sight **calls it in** (§7.7): one other
    /// guard converges on the last-known cell and searches it. Baseline, breaking
    /// contact leaves you with only the guard you broke it from — with this on,
    /// someone who was never chasing you starts combing the ground you vanished
    /// into. This is the §7.7 net, and the reason a lone tail is allowed to be
    /// escapable (§7.6 fix 4). A **glimpse**-zone contact never calls anyone, and
    /// the losing guard searches on its own either way — the modifier adds the
    /// calling of others, nothing else.
    pub sighting_lost_calls_a_guard: bool,
    /// **Harder.** A guard that finds a body (§7.2) **calls it in** (§7.7): two
    /// other guards converge on the body's cell and search it. Finding a body is
    /// the loudest event in the game, and "louder than a sighting" is expressed as
    /// *how many come* — not a longer reach or a priority system — so this is the
    /// same call as [`sighting_lost_calls_a_guard`] with a bigger count. The finder
    /// reacts on its own either way (the harder alert and its own search, §7.2);
    /// the modifier adds only the calling of others.
    ///
    /// [`sighting_lost_calls_a_guard`]: LevelModifiers::sighting_lost_calls_a_guard
    pub body_found_calls_two_guards: bool,
    /// **Easier.** Paint the §11.5 danger overlay in full — the cone of *every*
    /// guard, not only the ones you can currently see. This only ever **widens**
    /// what is revealed; it never hides the red detection set, so the §11.5
    /// [SETTLED] contract ("if your cell isn't red, no guard detects you") is
    /// kept and, if anything, strengthened.
    pub always_show_vision_cones: bool,
    /// How much of the building the player is given before setting foot in it
    /// (§11.5a/#307/#233) — [`Full`](LayoutKnowledge::Full) hands the real
    /// architecture over (easier), [`None`](LayoutKnowledge::None) hides it
    /// altogether (harder). Baseline [`LayoutKnowledge::Plans`]: §11.5a's schematic,
    /// untouched.
    ///
    /// **One knob rather than two toggles**, because the two ends are two answers to
    /// one question and a pair of bools could be asked both at once with nothing to
    /// say which won. The knob has the same shape as [`GuardCount`] — a neutral middle
    /// with a departure either side — and composes by the same rule
    /// ([`LayoutKnowledge::harder_of`]).
    ///
    /// **Its hard end is the one modifier that overrides a [SETTLED] rule** (§11.5a,
    /// geometry is never fogged). That is why the bending lives here and only here:
    /// the base game keeps the visible layout, and a run that gives it up says so on
    /// its card. See [`LayoutKnowledge`] for what each end costs.
    ///
    /// **Both ends are in the directed pool since #518.** The hard end used to be kept
    /// out on the grounds that it is not a *difficulty step* — the ±N axis promises the
    /// same game under more or less pressure, and this end arguably hands back a
    /// different one. That reading is unchanged and the entry is admitted anyway, with
    /// the cost stated: a `+N` quick-play run can be dealt a fogged-geometry facility the
    /// player never named, and the Level info card's *"Layout unknown"* is their only
    /// warning. It is the largest player-facing cost in that change, and appendix 47 is
    /// where it is argued.
    pub layout_knowledge: LayoutKnowledge,
    /// **Retired — slot 5, frozen (#442).** This was the `calm_guards_detect_only_
    /// their_cone` experiment (#410): a **Calm** guard detecting exactly its ~90° cone,
    /// its flank cells (§6.2 tier 3) dropping out of detection along with the three at
    /// its back, while any guard that is *not* Calm watches its sides as it always did.
    ///
    /// **It is now the rule** (§6.1/§6.2/§7.2), so this toggle asks for nothing that is
    /// not already true. Appendix 28 has what it measured and why the condition on the
    /// mood is the design rather than a tuning fudge; the adoption itself was the feel
    /// call that appendix left open.
    ///
    /// **The field stays because the slot must.** Its position is a permanent slot
    /// number the level-seed token encodes *by index*
    /// ([`docs/level-seed-token.md`](../../docs/level-seed-token.md) §3): deleting it
    /// and closing the gap would silently re-point every token ever shared at a
    /// different modifier — the #286 break, with nothing to notice it by. So it is a
    /// tombstone, not a hole: it still round-trips through
    /// [`modifier_slots`](crate::level_seed) so old tokens decode exactly, and nothing
    /// reads it. It contributes no [`CAPTIONS`] row, because a caption announcing a
    /// rule the level plays regardless would be noise.
    ///
    /// Restoring the *harder* arm — guards that watch their flanks even while calm —
    /// would be a **new** entry appended to the end of the slot list, never a revival
    /// of this one.
    pub calm_guards_detect_only_their_cone: bool,
    /// **Harder.** Every door on the level is **automatic** (§10.4/#147/#452):
    /// frameless spans that shut themselves a few turns after the doorway is last
    /// vacated. Baseline, every door is **manual** and hinged — a handle to shut by
    /// hand, and a Calm guard that closes one behind itself (#146).
    ///
    /// **All or nothing, and that is the change.** Automatic doors used to be a
    /// `[START]` fraction of every facility drawn per doorway, so a run met both
    /// vocabularies mixed together and which door was which was a coin flip you
    /// discovered by walking up to it. A run now speaks **one** door vocabulary, and
    /// which one is a stated property of the run rather than a per-doorway draw.
    ///
    /// **Marked `Harder`, and the sim does not confirm it.** This is the honest state
    /// of the question rather than a claim, so it is written down here:
    ///
    /// The case *for* harder is structural. The hand-close goes away — you can no
    /// longer bump a hinge to break a sightline when you choose to, only wait out a
    /// timer you do not control — and an all-automatic level has systematically
    /// **wider throats** (a 3–6 panel passable span where a manual door is two solid
    /// hinges around 1–4), which is a longer sightline through every doorway. That
    /// much is measurable, and it is what `the_modifier_widens_the_facility_s_throats`
    /// holds as §2.3's anti-facade guard.
    ///
    /// The case against is the batch. Over 100 seeds × four temperaments the **win
    /// rate does not move** (net one win in four hundred), and detections per turn
    /// fall in *all four* — the doors re-closing themselves restores cover that the
    /// baseline's guard traffic props open for good. What does move is the run's
    /// character: `alert_peak_mean` rises in three of four, `diversity` falls in three
    /// of four, and pacing splits by temperament — the avoidant profiles get slower,
    /// the striking ones faster.
    ///
    /// So on the evidence this is a **feel** modifier, not a difficulty one, and what it
    /// really wants is the difficulty/feeling split (#258). `Harder` is the safer of the
    /// two labels until that split exists: the structural change is the
    /// harder-direction one, and it is the direction the anti-facade assertion can
    /// actually hold.
    ///
    /// **It is in the directed pool since #518**, feel/difficulty question and all — and
    /// it is the one entry there that reaches the **carve**, so it is the exception the
    /// pool's same-building guarantee is now stated with rather than without
    /// ([`draw_from_pool`]). A `+N` draw that picks it hands back a different facility
    /// from the same seed, which no other entry does.
    ///
    /// **The one modifier that reaches generation.** Every other field here is read
    /// at runtime or by the renderer; this one is consumed by
    /// [`generate_level`](crate::generate_level) before a single cell is stamped, so
    /// it is threaded in as a parameter rather than consulted from a global — see
    /// that function's own note.
    pub automatic_doors: bool,
    /// **Harder.** A **Calm** guard prefers a **console** as its next patrol
    /// destination (§7.5/#319): every other patrol leg heads for a patrollable cell
    /// beside an intel console `$` or the comms console `Ψ` its beat touches, and it
    /// cycles them, so every console in a beat is stood beside within a bounded number
    /// of turns rather than by luck.
    ///
    /// Baseline, a console is just floor the farthest-uninspected sweep may or may not
    /// wander past: the room holding the intel is no better watched than an empty
    /// corridor, so *where the player must go* has no bearing on *where the guards
    /// are*. With this on, both of the run's fixed errands — take the intel (§4.5) and
    /// silence the net (§7.3) — have to be timed against a patrol that comes back.
    ///
    /// **It bends destination choice and nothing else.** The guard still walks 1
    /// cell/turn (§7.1 **[SETTLED]**), still takes the ordinary §7.5 dwell on arrival
    /// and then leaves, and drops the whole thing the instant it turns reactive — so it
    /// raises how *often* a console is looked at, never how *long* a guard holds it
    /// (§7.6's anti-tracking-turret rule). The player's own tunnel is deliberately not
    /// in the set: no guard knows where you came in (§1/§4.5), and adding the exit
    /// would invent facility knowledge the fiction denies.
    ///
    /// **A silenced net switches it off with the beats** (§7.3). The cycle is over the
    /// consoles a guard's *beat* touches, and killing the net leaves no partition to
    /// divide the building — every Calm guard takes the whole level and draws at random
    /// ([`PatrolStyle::Wander`](crate::guard::PatrolStyle)). There is then no "its
    /// consoles" left to cycle, so the console watch goes the way the beat does. That is
    /// one more thing the comms console buys, priced by the same detour (§7.3).
    ///
    /// **`Harder`, and what the sim does and does not support** (appendix 39). The §2.3
    /// assertion it is held to is *turns with a guard's cone on a console*, which rises
    /// on every seed of the sweep and never falls. Over 120 bot seeds per profile the
    /// **cautious** profile — the one that lingers near an objective waiting for a gap —
    /// loses ten points of win rate; balanced and aggressive move inside the noise, and
    /// diversity does not move at all. **Detections per run fall in all three**, because
    /// guard time is finite and legs spent on the consoles are legs not spent sweeping
    /// the rest of the beat. That is the shape of the modifier rather than a hole in it:
    /// the pressure is concentrated on the two errands a run cannot skip, not spread
    /// over the level.
    pub guards_watch_consoles: bool,
    /// **Easier.** Paint every live §7.6 **investigation area** (§11.5/#224): the
    /// `SEARCH_RADIUS` box around each searching guard's focus washes orange, so *where*
    /// a search is sweeping is on the board rather than inferred from a wandering cone.
    ///
    /// Baseline, a search is legible only in **time** — the near line says one starts
    /// and says when it is called off (§11.7) — and that is the half a hidden player
    /// needs most, since the guard doing the searching is typically one they cannot see.
    /// This adds the half a message cannot carry: *is hiding **here** risky?* It is the
    /// literal flush zone, the same box [`checks_hideout_at`] tests a cupboard against,
    /// so a player who can see the area can see the mistake #219 made available — diving
    /// into the sweep a body of your own leaving started.
    ///
    /// **It is a separate, advisory layer and never the red one** (§11.5 **[SETTLED]**).
    /// Red means *a guard detects you here*; this orange means only *a guard's attention
    /// is on this area*, and where the two overlap red paints last and wins. It widens
    /// what is drawn and hides nothing, so the overlay's one non-negotiable promise —
    /// if your cell isn't red, no guard you can see will detect you — is untouched.
    ///
    /// **Every live search projects one, seen or not.** The §11.5 "never a guess" rule
    /// binds the detection set, not an advisory layer, and a see-only rule would go dark
    /// exactly when the area is worth most: in a cupboard, watching a guard you cannot
    /// see decide whether to open it. That is the same shape
    /// [`always_show_vision_cones`] takes, and it is why this sits on the *easier* side
    /// of the §12.6 directed pool — knowing which ground is being combed is a real
    /// advantage, and it has to be paid for by taking a harder rule elsewhere.
    ///
    /// [`checks_hideout_at`]: crate::Guard::checks_hideout_at
    /// [`always_show_vision_cones`]: LevelModifiers::always_show_vision_cones
    pub show_search_areas: bool,
    /// How many guards patrol the facility (§10.2/#232) — one step either side of the
    /// recipe's count, [`More`](GuardCount::More) harder and
    /// [`Fewer`](GuardCount::Fewer) easier. Baseline
    /// [`GuardCount::Baseline`]: the recipe's own number, untouched.
    ///
    /// **The second modifier that reaches generation**, and it reaches a different
    /// part of it than [`automatic_doors`](LevelModifiers::automatic_doors) does. The
    /// doors modifier changes what a doorway *is*, so the two settings **carve
    /// different buildings** from one seed. This one is read by placement (§10.1.9)
    /// and leaves the carve alone: the pieces are drawn from the same stream in the
    /// same order, and the guards come off one shuffled pool by `take(n)` — so from a
    /// single seed the three settings put the **same player, exit and intel** in the
    /// **same building**, and their guard sets are strictly **nested**. `Fewer` is the
    /// baseline's guards minus its last, `More` is them plus one more.
    ///
    /// That nesting is what the §2.3 directional assertion is stated on, and it is a
    /// much stronger claim than the distributional one the doors modifier has to
    /// settle for: on one seed, more guards watch a superset of the cells fewer guards
    /// watch. Everything after placement does shift — the comms console is drawn from
    /// a pool the guards are excluded from, and each guard draws a radio clock — so
    /// the two settings are the same *level* played out differently, not the same run.
    pub guard_count: GuardCount,
    /// How many intel consoles the facility holds (§10.2/#207) — one step either side
    /// of the recipe's count, [`Fewer`](IntelCount::Fewer) harder and
    /// [`More`](IntelCount::More) easier. Baseline [`IntelCount::Baseline`]: the
    /// recipe's own number, untouched.
    ///
    /// **The third modifier that reaches generation**, and it reaches the same part of
    /// it the guard count does — placement (§10.1.9), never the carve. The pieces come
    /// off the same shuffled pool by `take(n)` from the same stream, so from one seed
    /// the three settings put the **same player, exit and guards** in the **same
    /// building** and their console sets are strictly **nested**: `Fewer` is the
    /// baseline's consoles minus its last, `More` is them plus one more.
    ///
    /// That nesting is what makes the flavour honest (§2.3): a [`Vault`] really does
    /// hold a console an [`Outpost`] does not, on the same ground, rather than being a
    /// differently-worded label on the same facility.
    ///
    /// [`Vault`]: crate::Flavour::Vault
    /// [`Outpost`]: crate::Flavour::Outpost
    pub intel_count: IntelCount,
    /// How many **equipment caches** the facility hides (§2.2/§14 v3/#209) — crates of
    /// salvaged tech, planted in non-start rooms, each of whose bump unlocks a §8.3
    /// tech ability for the rest of the run. Baseline [`CacheCount::None`]: a facility
    /// holds none, which is every quick-play level.
    ///
    /// **The fourth modifier that reaches generation**, and it reaches placement
    /// (§10.1.8) exactly as the two count knobs do — never the carve. The crates are
    /// drawn from the same shuffled pool as the comms console, immediately after it, so
    /// the settings put the **same player, exit and intel** in the **same building**;
    /// what differs from there on is how many crates stand in it and a guard pool that
    /// many cells smaller.
    ///
    /// **Easier, and paid for on the map rather than in this field.** What a crate gives
    /// is permanent and compounding — an ability the *rest of the run* carries (§2.2) —
    /// so no reading of it makes the facility harder. The price is the flavour that
    /// plants them: the richer the crate count, the more the offer costs in guards or in
    /// consoles ([`Flavour::modifiers`](crate::Flavour::modifiers)), which is where the
    /// §2.3 trade actually lives.
    ///
    /// **What the crates hold is not here**, deliberately. This says how many a facility
    /// has; which abilities they hold is drawn from the run's own unheld tech at boot
    /// ([`cache_contents`](crate::cache_contents)), because a crate offering a fifth
    /// Dephase is a reward that is not one — and that draw needs the loadout, which a
    /// level modifier has no business carrying. It is also what makes the count a
    /// **ceiling** rather than a promise: a facility plants only as many crates as there
    /// is tech left in the world for this run to find.
    pub caches: CacheCount,
    /// **Harder.** The doors of the one room holding the facility's **prize** are
    /// **locked**, and every guard carries a key (§10.4/#236). Baseline, §10.4's
    /// **[START]** rule stands untouched: anyone can operate any door, no keys, no
    /// locks.
    ///
    /// **Which room.** A room hiding an **equipment cache** if the facility hides one
    /// (§10.2/#209), otherwise a room holding an **intel console**. So what the lock
    /// gates depends on the run: in quick play, where there are no crates and the exit
    /// wants every console (`IntelGate::All`), it is a **hard gate** on the win and the
    /// modifier's whole promise lands — you cannot leave without committing a takedown.
    /// In a campaign facility rich enough to hide crates it gates **loot**, which is the
    /// same rule reading as a choice rather than a toll. Among the candidates a room no
    /// §10.7 duct opens into is preferred, so a shortcut cannot walk round the lock and
    /// leave the modifier a caption.
    ///
    /// **Why the key is on every guard, and not on one of them.** The §7.2 takedown is
    /// the price, and it is already a steep one — a permanent body on the §7.3 radio
    /// clock, evidence to hide, an alert if it is found. Hanging the key on one *named*
    /// guard would add a search on top of that price and turn the modifier into a hunt
    /// for a particular `g` the player has no way to pick out; with the key on all of
    /// them the cost is exactly the takedown, which is the cost §2.3 asks the modifier
    /// to charge. It goes **straight to hand**, not onto the body: the body is the cost,
    /// and a key lying on the floor would be a second errand and a second thing to lose.
    ///
    /// **The doors are automatic, and that is what makes the lock hold** (§10.4/#147).
    /// Guards carry keys, so they walk through as they always did — a key lock that let
    /// the door stand open would last until the first patrol came past and never again.
    /// Frameless and self-closing, the doorway shuts a few turns after it is last
    /// vacated, and those turns are the modifier's one bypass: a player standing beside
    /// a door a guard has just opened can **slip in without a key**, at the price of
    /// standing next to that guard. Thin, and a decision rather than a lottery.
    ///
    /// **The lock refuses entry, never exit.** From inside the room the door always
    /// opens. A slip-in that could seal a player in a locked room with no key would be a
    /// run ended by a mechanic they were invited to gamble on — §2.2/§7.2's soft-lock
    /// class, which the design does not allow to be merely unlikely.
    ///
    /// **The fifth modifier that reaches generation**, and it reaches *past* placement
    /// rather than into it. The room cannot be chosen until the crates and consoles are
    /// seated, so the lock is applied after [`place`](crate::place) on the finished
    /// board — and it draws nothing at all, so a seed carves and places the **same
    /// building** either way and the two settings differ in exactly the cells this rule
    /// touches. That is the strongest frame a §2.3 directional assertion has been stated
    /// in here: on one seed, the prize is reachable at baseline and not reachable behind
    /// the locks (`the_lock_puts_the_prize_out_of_reach`).
    ///
    /// **In the directed pool since #518** (§12.6/[`POOL`]), and it was held out of it
    /// until then because the §13.2 bot has no cue for buying a key: it knows a locked
    /// door is not a way through and opens the room the moment a takedown hands it the
    /// key, but no plan of its says *the thing I need is behind that door, so go and buy
    /// the key*. Under the sim preset's `--intel-gate one` that costs it a little and the
    /// modifier reads harder in the documented direction (over 100 balanced seeds: win
    /// rate 35% → 24%, detections 848 → 1,086, diversity 0.60 → 0.54); under
    /// `--intel-gate all`, where the locked console is required, the win rate goes to
    /// **zero**, which is a fact about the bot rather than about the game (§13.3).
    ///
    /// That was never a reason to withhold the modifier from **players**, which is what
    /// keeping it out of the pool did (§13.1/§13.4: the sim is a smoke detector, not a
    /// judge — see [`POOL`]). It stays a reason not to cite a bot batch that draws it:
    /// until #517 teaches the policy to buy a key, those numbers measure the bot (§13.3)
    /// and do not belong in a balance argument.
    ///
    /// Appendix 46 has the argument: why the key is on every guard rather than on one,
    /// why the gated doors have to shut themselves, why the lock refuses entry and never
    /// exit, and what the §10.6 guarantee had to grow.
    pub prize_room_locked: bool,
    /// The exit's intel gate (§4.5/§10.2) — how much intel the run must hold to
    /// leave. Baseline [`IntelGate::AtLeastOne`]; quick play (#244) sets
    /// [`IntelGate::All`], campaign (§14 v3) [`IntelGate::None`]. Read at runtime
    /// by [`State::exit_ready`](crate::State::exit_ready).
    pub intel_to_exit: IntelGate,
}

/// Which way a level modifier bends a run's difficulty (§12.6) — the *harder* /
/// *easier* marker each [`LevelModifiers`] field documents, promoted from doc-only
/// prose to real data the help panel (#248) reads, colours, and asserts against.
/// *Harder* raises the pressure on a run, *easier* lowers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModifierDirection {
    /// Raises the pressure on the run — the help card cues it in the §11.2 Warning
    /// colour (a threat is hunting), the same orange a harder rule reads as.
    Harder,
    /// Lowers it, a rule bent in the player's favour — cued in the §11.2 Owned
    /// colour (yours), the calm blue that reads apart from every threat shade.
    Easier,
}

/// One **active** level modifier, described for the help panel (#248): a
/// human-readable name, the direction it bends difficulty, and — for a bounded
/// knob — the value it currently sits at. Produced by [`LevelModifiers::active`]
/// so the card is **derived**, never hand-copied (§11.3): a new modifier field
/// surfaces here on its own and cannot be silently omitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveModifier {
    /// The modifier's name, as the player reads it on the card.
    pub name: &'static str,
    /// Which way it bends the run — the card's colour cue.
    pub direction: ModifierDirection,
    /// A bounded knob's current value (e.g. `"all of it"`), or `None` for a plain
    /// on/off toggle whose name says all there is. Shaped for the knobs that land
    /// with #232/#244; only toggles and the intel gate produce one today.
    pub detail: Option<&'static str>,
}

/// What the help card puts between a modifier's name and its value — the `": "` in
/// `Intel to exit: all of it`. Shared by the renderer and by
/// [`ActiveModifier::caption_len`] so the measured width and the drawn one cannot
/// drift apart.
pub(crate) const CAPTION_SEPARATOR: &str = ": ";

impl ActiveModifier {
    /// The width, in cells, of this modifier's caption as the help card draws it
    /// (#248) — `name` alone, or `name: detail` for a knob. `const` on purpose:
    /// it is what the card's compile-time width bound measures against, so a
    /// caption that would clip on the v1 board fails the build rather than being
    /// discovered as a truncated line in a screenshot.
    pub(crate) const fn caption_len(&self) -> usize {
        match self.detail {
            Some(detail) => self.name.len() + CAPTION_SEPARATOR.len() + detail.len(),
            None => self.name.len(),
        }
    }
}

/// **Every caption the help card can draw** (#248) — the one place a modifier's
/// display text is written, so the width bound in
/// [`render::help`](crate::render) can measure the complete set at compile time.
/// [`LevelModifiers::active`] returns entries from here rather than building
/// literals inline: a caption that is not in this table is a caption nothing
/// checks, which is exactly the drift the bound exists to stop.
///
/// A bounded knob contributes **one entry per non-baseline value**, since each is a
/// different caption with a different width.
pub(crate) const CAPTIONS: [ActiveModifier; 19] = [
    LOCKED_PRIZE_ROOM,
    SEARCHES_HIDEOUTS,
    CALLS_IN_SIGHTINGS,
    CALLS_IN_BODIES,
    SHOWS_ALL_CONES,
    KNOWS_FULL_LAYOUT,
    LAYOUT_UNKNOWN,
    ALL_DOORS_AUTOMATIC,
    WATCHES_CONSOLES,
    GUARDS_MORE,
    GUARDS_FEWER,
    CONSOLES_MORE,
    CONSOLES_FEWER,
    ONE_CACHE,
    TWO_CACHES,
    THREE_CACHES,
    SHOWS_SEARCH_AREAS,
    INTEL_GATE_ALL,
    INTEL_GATE_NONE,
];

const SEARCHES_HIDEOUTS: ActiveModifier = ActiveModifier {
    name: "Guards search hideouts",
    direction: ModifierDirection::Harder,
    detail: None,
};

const CALLS_IN_SIGHTINGS: ActiveModifier = ActiveModifier {
    name: "Sightings called in",
    direction: ModifierDirection::Harder,
    detail: Some("one guard"),
};

const CALLS_IN_BODIES: ActiveModifier = ActiveModifier {
    name: "Bodies called in",
    direction: ModifierDirection::Harder,
    detail: Some("two guards"),
};

const SHOWS_ALL_CONES: ActiveModifier = ActiveModifier {
    name: "All vision cones shown",
    direction: ModifierDirection::Easier,
    detail: None,
};

const KNOWS_FULL_LAYOUT: ActiveModifier = ActiveModifier {
    name: "Full layout known",
    direction: ModifierDirection::Easier,
    detail: None,
};

/// The same knob's hard end (#233), and the caption a player most needs to read
/// **before** turn one: with this on the board opens nearly empty, and a card that did
/// not say so would leave them wondering whether the render had broken.
///
/// Not styled as `Layout: fully known` / `Layout: unknown` even though the two are now
/// one knob's ends. The easier end shipped first and *"Full layout known"* is the
/// caption players have already read; restyling a caption nothing else needs changed
/// would be churn on the card for tidiness in the source.
const LAYOUT_UNKNOWN: ActiveModifier = ActiveModifier {
    name: "Layout unknown",
    direction: ModifierDirection::Harder,
    detail: None,
};

const ALL_DOORS_AUTOMATIC: ActiveModifier = ActiveModifier {
    name: "Doors",
    direction: ModifierDirection::Harder,
    detail: Some("all automatic"),
};

/// Named for the **ground**, not for the rule that bends: what a player can act on is
/// that the consoles are patrolled, not that a destination pick is biased. It is the
/// same reading the other harder captions take — say what the guards do.
const WATCHES_CONSOLES: ActiveModifier = ActiveModifier {
    name: "Guards watch consoles",
    direction: ModifierDirection::Harder,
    detail: None,
};

/// Named for what the **board** gains, like its easier neighbours above (#224): the
/// player reads that the searched ground is drawn, not that a guard has a focus disc.
/// "Search areas" rather than "investigation areas" because *search* is the word §7.6
/// and the near line already use for the same thing — one name for one mechanic
/// (§11.8).
const SHOWS_SEARCH_AREAS: ActiveModifier = ActiveModifier {
    name: "Search areas shown",
    direction: ModifierDirection::Easier,
    detail: None,
};

/// The knob's two ends read as a **count**, not as a rule — "one more" says what the
/// facility holds, which is the only thing about it a player can act on. The absolute
/// number is deliberately not in the caption: the card describes how a run departs
/// from the baseline, and a bare "5" would need the baseline beside it to mean
/// anything.
const GUARDS_MORE: ActiveModifier = ActiveModifier {
    name: "Guards",
    direction: ModifierDirection::Harder,
    detail: Some("one more"),
};

const GUARDS_FEWER: ActiveModifier = ActiveModifier {
    name: "Guards",
    direction: ModifierDirection::Easier,
    detail: Some("one fewer"),
};

/// The intel knob's two ends, worded as the guard knob's are — a **count**, said as a
/// departure from the recipe rather than as an absolute the card would have to print
/// the baseline beside. "Intel" alone would read as the gate one row down, so the
/// caption names the thing counted: the consoles standing in the building.
const CONSOLES_MORE: ActiveModifier = ActiveModifier {
    name: "Consoles",
    direction: ModifierDirection::Easier,
    detail: Some("one more"),
};

const CONSOLES_FEWER: ActiveModifier = ActiveModifier {
    name: "Consoles",
    direction: ModifierDirection::Harder,
    detail: Some("one fewer"),
};

/// The cache knob's three rungs. Each names the **crates**, not the abilities in them:
/// which tech a run finds is drawn from what it does not already hold (#209), so a card
/// printing a name here would be promising a specific prize the facility has not
/// decided on yet — and spoiling the find if it had.
///
/// A count, said as a count, because unlike the guard and console knobs there is no
/// baseline behind it to depart from: a facility hides these many crates, full stop.
const ONE_CACHE: ActiveModifier = ActiveModifier {
    name: "Equipment caches",
    direction: ModifierDirection::Easier,
    detail: Some("one hidden"),
};

const TWO_CACHES: ActiveModifier = ActiveModifier {
    name: "Equipment caches",
    direction: ModifierDirection::Easier,
    detail: Some("two hidden"),
};

const THREE_CACHES: ActiveModifier = ActiveModifier {
    name: "Equipment caches",
    direction: ModifierDirection::Easier,
    detail: Some("three hidden"),
};

/// Named for the **ground and the way through it** (#236), which is the pair a player
/// can act on: one room is shut, and the guards are what open it. Naming the rule
/// instead ("Doors: keyed") would say which system bends without saying the one thing
/// worth knowing before turn one — that the way in is a takedown.
const LOCKED_PRIZE_ROOM: ActiveModifier = ActiveModifier {
    name: "Locked room",
    direction: ModifierDirection::Harder,
    detail: Some("guards hold the key"),
};

const INTEL_GATE_ALL: ActiveModifier = ActiveModifier {
    name: "Intel to exit",
    direction: ModifierDirection::Harder,
    detail: Some("all of it"),
};

const INTEL_GATE_NONE: ActiveModifier = ActiveModifier {
    name: "Intel to exit",
    direction: ModifierDirection::Easier,
    detail: Some("none required"),
};

/// One entry of the §12.6 directed pool, as the difficulty draw sees it (#297): the
/// caption that already names it and states its direction, plus the change it makes.
///
/// The direction is declared **once**, on the caption, and the draw reads it from
/// there — there is no second hand-kept list of "the harder ones" that could come to
/// disagree with the field's own documentation.
pub(crate) struct PoolEntry {
    /// The caption this entry contributes to [`LevelModifiers::active`], and so —
    /// through [`ActiveModifier::direction`] — the way it bends the run.
    pub(crate) caption: ActiveModifier,
    /// Apply this entry. A `fn` pointer rather than a field offset, so the table
    /// below stays plain data that a `const` can hold.
    pub(crate) set: fn(&mut LevelModifiers),
}

/// **The directed pool** (§12.6/#297): every modifier the difficulty draw may pick,
/// listed in the permanent slot order [`modifier_slots`](crate::level_seed) encodes.
/// A new modifier joins the pool by taking a row here beside the caption that
/// declares its direction.
///
/// **What is still out, and why.** The **retired** slot 5 (#442) is not a modifier any
/// more — it asks for the rule the level already plays, so drawing it would spend a
/// pick on nothing. The **intel gate** is a knob the pool cannot reach: quick play
/// already sets it to [`IntelGate::All`], and [`LevelModifiers::union`] composes it
/// *harder-ward*, so an easier draw could not relax it without the draw learning to
/// replace a knob rather than compose with it. Relaxing the gate is therefore a
/// decision the pool does not quietly make.
///
/// **The intel count is out for a sharper reason** (#207). It is symmetric like
/// [`GuardCount`], so the mechanical objection above does not apply — but under quick
/// play's [`IntelGate::All`] it moves the **win condition**, not the pressure on it, and
/// a difficulty draw that quietly decided how many consoles a run must clear would be
/// tuning quick play from inside a campaign ticket. It is driven by node flavour
/// ([`Flavour`](crate::Flavour)) and by nothing else until someone measures it.
///
/// **The cache count is out**, on the intel count's reasoning taken one step further
/// (#209). It is not a difficulty knob at all: what it hands over is a §8.3 ability the
/// *rest of the campaign run* carries, so a quick-play difficulty draw that picked it
/// would be granting a permanent reward inside a single facility — meta-progression by
/// accident, and the one thing §2.2 forbids outright. It is driven by node flavour
/// ([`Flavour`](crate::Flavour)) and by nothing else.
///
/// **A symmetric knob is a different case, and both its ends are in** (#232,
/// appendix 30). [`GuardCount`]'s baseline is a neutral middle rather than one end of
/// an axis quick play has already walked to, so
/// [`harder_of`](GuardCount::harder_of) leaves an easier pick standing instead of
/// overruling it with a base that asked for nothing — the exact mechanical objection
/// that keeps the gate out does not arise.
///
/// # What decides membership, and what does not (#518)
///
/// A modifier's job is to **modify the level** — that is the whole of what §12.6 says
/// one is. So two questions decide whether it belongs here, and only two:
///
/// 1. Is it a **difficulty** change rather than a change of subject? The ±N axis
///    promises the same game under more or less pressure.
/// 2. Does it bend in a **documented direction** ([`ModifierDirection`])? That is what
///    makes §2.3's directional assertion true by construction rather than by review.
///
/// **Whether the §13.2 bot can weigh it is not one of them.** That reading crept in and
/// this is where it stops: the sim is *"a smoke detector, not a judge"* (§13.4), and
/// **"you play and rule — fun is a human judgement"** (§13.1). Some modifiers are light
/// and sweep cleanly; others will only ever be judged by playing them. Withholding the
/// second kind from players because the harness cannot score it is letting the smoke
/// detector decide what the game contains, which is the §13.3 failure the sim exists to
/// avoid, pointed the other way.
///
/// Bot-blindness is a real fact and it stays a **reporting** concern: a batch that draws
/// a modifier the policy cannot use measures the bot, and its numbers do not belong in a
/// balance argument. It has never been a small problem here either — three of the four
/// *easier* entries above are already bot-blind (the policy builds its danger set from
/// seen guards only, is granted geometry unconditionally, and reads no search area), so
/// a `−N` bot batch has long been drawing no-ops. That is an argument for teaching the
/// bot (#498, #517) and for labelling the batch, never for a thinner game.
///
/// # The three admitted by #518, and what each one costs
///
/// **`automatic_doors`** (#452) is the entry this table had never explained. Its
/// exclusion was referred to here — *"the mechanical objection that keeps
/// `automatic_doors` out"* — and stated only on the field, which is exactly the drift a
/// "why is this missing?" reader falls into. The objection is real and unchanged: **it
/// reaches the carve**, so a draw that picks it and a draw that does not are two
/// *different facilities* from one seed, and the ±N arms stop being the same building.
/// That guarantee is now stated with this one exception rather than flatly
/// ([`draw_from_pool`]) and pinned by
/// `a_difficulty_draw_moves_the_building_only_where_it_is_meant_to`. Its other objection —
/// that the sim measured **feel** rather than difficulty — is question 1 above, and it
/// is genuinely open; `Harder` stays the safer label until #258's split can answer it.
///
/// **[`LayoutKnowledge::None`]** (#233) is the only entry that overrides a **[SETTLED]**
/// rule (§11.5a: geometry is never fogged), and it is the one whose question 1 is
/// closest to *no*: §11.5a's escape-route planning stops existing, which is arguably a
/// different game rather than a harder one. It is admitted with that stated: a `+N` run
/// can be dealt a fogged-geometry facility the player never named, and the Level info
/// card's *"Layout unknown"* is their only warning.
///
/// **[`prize_room_locked`](LevelModifiers::prize_room_locked)** (#236) is the clean case,
/// and the one that exposed the bad criterion. It is a plain harder toggle whose two
/// settings are the same building down to the cell; nothing about it failed either
/// question. It was out solely because the bot has no cue for buying a key — which, by
/// the rule above, was never a reason at all.
pub(crate) const POOL: [PoolEntry; 12] = [
    PoolEntry {
        caption: SEARCHES_HIDEOUTS,
        set: |m| m.guards_always_search_hideouts = true,
    },
    PoolEntry {
        caption: CALLS_IN_SIGHTINGS,
        set: |m| m.sighting_lost_calls_a_guard = true,
    },
    PoolEntry {
        caption: CALLS_IN_BODIES,
        set: |m| m.body_found_calls_two_guards = true,
    },
    PoolEntry {
        caption: SHOWS_ALL_CONES,
        set: |m| m.always_show_vision_cones = true,
    },
    PoolEntry {
        caption: KNOWS_FULL_LAYOUT,
        set: |m| m.layout_knowledge = LayoutKnowledge::Full,
    },
    // The knob's two ends, one on each side of the pool's filter (#232). They are
    // listed in slot order like everything else, so the two rows sit together even
    // though no single draw can ever pick both — a `+N` draw sees only the first and
    // a `−N` draw only the second.
    PoolEntry {
        caption: GUARDS_MORE,
        set: |m| m.guard_count = GuardCount::More,
    },
    PoolEntry {
        caption: GUARDS_FEWER,
        set: |m| m.guard_count = GuardCount::Fewer,
    },
    // Slot 11, appended (#319) — a runtime rule like every other entry above the
    // guard knob, so the ±N arms of a comparison that draws it are the same board
    // down to the last radio clock.
    PoolEntry {
        caption: WATCHES_CONSOLES,
        set: |m| m.guards_watch_consoles = true,
    },
    // Slot 15, appended (#224) — the easier side's fourth entry, which is what takes
    // −2 off the two-of-three draw appendix 29 flagged and this one leaves at four.
    PoolEntry {
        caption: SHOWS_SEARCH_AREAS,
        set: |m| m.show_search_areas = true,
    },
    // The three admitted together by #518, listed in the slot order the rest of the
    // table keeps. Each was held back for its own reason and each of those reasons is
    // now either superseded or deliberately accepted — appendix 47 has the trade.
    //
    // Slot 6 (#452). **The one entry that reaches the carve**, and the reason the
    // `same_building` guarantee below is now stated with an exception rather than
    // flatly (see [`draw_from_pool`]).
    PoolEntry {
        caption: ALL_DOORS_AUTOMATIC,
        set: |m| m.automatic_doors = true,
    },
    // Slot 16 (#233) — the layout knob's harder end, and the only pool entry that
    // overrides a **[SETTLED]** rule (§11.5a: geometry is never fogged). A `+N` draw
    // can therefore hand a player a fogged-geometry run they did not name, which is the
    // largest player-facing cost in this change and is said plainly in §11.5a rather
    // than left as a contradiction between the doc and the table.
    PoolEntry {
        caption: LAYOUT_UNKNOWN,
        set: |m| m.layout_knowledge = LayoutKnowledge::None,
    },
    // Slot 17 (#236).
    PoolEntry {
        caption: LOCKED_PRIZE_ROOM,
        set: |m| m.prize_room_locked = true,
    },
];

/// **Draw `picks` modifiers from the directed pool** and switch them on over `base` —
/// the one draw over [`POOL`], shared by everything that asks the pool for a rule
/// (§12.6).
///
/// Two callers today, and they are the two *sources* that pick rather than state:
/// the quick-play difficulty axis ([`Difficulty::draw`](crate::Difficulty::draw), #297)
/// and the campaign alert (#210). Sharing the draw is what keeps §2.3's directional
/// guarantee true **by construction** for both — the pool is filtered on
/// [`ModifierDirection`] itself, so a `direction`-ward draw cannot hand back a rule that
/// bends the other way, whichever source asked for it.
///
/// A pure function of `(base, direction, picks, seed)` (§12.4). The caller salts its own
/// seed: the difficulty axis and the alert must not share a stream position, and which
/// salt separates them is the caller's business rather than the pool's.
///
/// **The same-building guarantee, and its one exception** (#518). The salted sub-stream
/// buys a strong property: a seed's *facility* is byte-identical whatever this draw
/// returns, so the ±N arms of a comparison differ in their **rules** and in nothing else
/// — which is what makes comparing them worth anything. That held flatly while every
/// entry was read at runtime or by placement. It now holds for every entry **but
/// [`ALL_DOORS_AUTOMATIC`]**, which is consumed by the carve (§12.6): a draw that picks
/// it produces a different building from the same seed, and a comparison that lands on
/// it is comparing two facilities rather than two rulesets.
///
/// It is graded rather than binary, and **pinned by a test**
/// (`a_difficulty_draw_moves_the_building_only_where_it_is_meant_to`) rather than left
/// as a sentence here: the doors may move the building (a duct mouth relocates on seed
/// 42), [`LOCKED_PRIZE_ROOM`] may only re-role cells inside doorways the carve already
/// cut, and every other entry must leave the grid byte-identical. If a new entry ever
/// reaches generation, that test fails and this paragraph is what has to be rewritten.
///
/// **`base` is a parameter because the two callers need different ones.** Quick play
/// draws over [`LevelModifiers::default`], the game's baseline; a *contributing* source
/// like the alert must draw over [`LevelModifiers::neutral`], or the contribution would
/// silently ask for an intel gate one rung tighter than the campaign's and lock an exit
/// it never mentioned (see [`neutral`](LevelModifiers::neutral)).
///
/// Takes what exists when `picks` outruns the directed pool, rather than looping to fill
/// the quota — the rule [`Difficulty::picks`](crate::Difficulty::picks) states, held here
/// too so no caller can ask for more than the pool holds and panic for it.
pub(crate) fn draw_from_pool(
    base: LevelModifiers,
    direction: ModifierDirection,
    picks: usize,
    seed: u64,
) -> LevelModifiers {
    let mut drawn = base;
    let mut pool: Vec<&PoolEntry> = POOL
        .iter()
        .filter(|entry| entry.caption.direction == direction)
        .collect();
    // A partial Fisher–Yates over the directed pool, the same idiom the quick-play tech
    // grant draws its subset with.
    let mut rng = Rng::new(seed);
    for i in 0..picks.min(pool.len()) {
        let j = i + rng.below((pool.len() - i) as u32) as usize;
        pool.swap(i, j);
        (pool[i].set)(&mut drawn);
    }
    drawn
}

/// How many pool entries bend a run `direction`-wards — the size the draw
/// takes what it can from when it is asked for more picks than exist.
pub(crate) const fn pool_size(direction: ModifierDirection) -> usize {
    let mut size = 0;
    let mut i = 0;
    while i < POOL.len() {
        // `PartialEq` is not `const`; the enum is fieldless, so pattern-match instead.
        let same = matches!(
            (POOL[i].caption.direction, direction),
            (ModifierDirection::Harder, ModifierDirection::Harder)
                | (ModifierDirection::Easier, ModifierDirection::Easier)
        );
        if same {
            size += 1;
        }
        i += 1;
    }
    size
}

impl LevelModifiers {
    /// The modifiers **active** for this run, each described for display (#248):
    /// every field sitting off its baseline, in reading order. The baseline
    /// ([`LevelModifiers::default`]) yields an empty list — the help panel reads
    /// that as *none active* — and a knob resting at its baseline value
    /// ([`IntelGate::AtLeastOne`]) is likewise not "active".
    ///
    /// The set is **derived**, never a hand-copied table (§11.3): the fields are
    /// destructured, so adding a modifier is a compile error here until it is
    /// described — this is the single place the active set is enumerated, mirroring
    /// the derived glyph legend. Direction is real data ([`ModifierDirection`]), so
    /// the card's colour cue can never disagree with a field's documented sense.
    #[must_use]
    pub fn active(&self) -> Vec<ActiveModifier> {
        // Destructure, don't field-access: a new modifier field fails to compile
        // here until it is given a row, the compile-time half of the §11.3 rule.
        let LevelModifiers {
            guards_always_search_hideouts,
            sighting_lost_calls_a_guard,
            body_found_calls_two_guards,
            always_show_vision_cones,
            layout_knowledge,
            calm_guards_detect_only_their_cone,
            automatic_doors,
            guards_watch_consoles,
            show_search_areas,
            guard_count,
            intel_count,
            caches,
            prize_room_locked,
            intel_to_exit,
        } = *self;
        let mut active = Vec::new();
        // Every caption comes from [`CAPTIONS`] rather than a literal built here, so
        // the help card's compile-time width bound measures exactly what is drawn.
        if guards_always_search_hideouts {
            active.push(SEARCHES_HIDEOUTS);
        }
        if sighting_lost_calls_a_guard {
            active.push(CALLS_IN_SIGHTINGS);
        }
        if body_found_calls_two_guards {
            active.push(CALLS_IN_BODIES);
        }
        if always_show_vision_cones {
            active.push(SHOWS_ALL_CONES);
        }
        // The layout knob surfaces one caption per end, like the count knobs below
        // (§11.5a/#307/#233) — its baseline is the schematic every other run plays, so
        // there is nothing there to announce.
        match layout_knowledge {
            LayoutKnowledge::Plans => {} // §11.5a's own picture — nothing to surface
            LayoutKnowledge::Full => active.push(KNOWS_FULL_LAYOUT),
            LayoutKnowledge::None => active.push(LAYOUT_UNKNOWN),
        }
        if automatic_doors {
            active.push(ALL_DOORS_AUTOMATIC);
        }
        if guards_watch_consoles {
            active.push(WATCHES_CONSOLES);
        }
        // The reward knob surfaces one caption per rung, like the two count knobs
        // surface one per end (§10.2/#209).
        match caches {
            CacheCount::None => {} // nothing hidden — nothing to announce
            CacheCount::One => active.push(ONE_CACHE),
            CacheCount::Two => active.push(TWO_CACHES),
            CacheCount::Three => active.push(THREE_CACHES),
        }
        if show_search_areas {
            active.push(SHOWS_SEARCH_AREAS);
        }
        // Read before turn one or not at all (#236): with this on, one room's doorways
        // will not open to a bump, and a player who was not told would read that as the
        // game being broken rather than as the rule it is.
        if prize_room_locked {
            active.push(LOCKED_PRIZE_ROOM);
        }
        // Slot 5 is **retired** (#442) — see the field's own note. A run that
        // decodes a token with the bit set gets no caption, because there is nothing
        // to announce: what the bit asked for is the rule the level is already
        // playing.
        let _ = calm_guards_detect_only_their_cone;
        // A bounded knob surfaces only when it has left its baseline, and each end
        // carries its own direction (§10.2/#232).
        match guard_count {
            GuardCount::Baseline => {} // the recipe's own count — nothing to surface
            GuardCount::More => active.push(GUARDS_MORE),
            GuardCount::Fewer => active.push(GUARDS_FEWER),
        }
        // The reward end of the same shape (§10.2/#207) — surfaced beside the guard
        // count, so a Vault's card reads as the one trade it is: one more console,
        // one more guard over it.
        match intel_count {
            IntelCount::Baseline => {} // the recipe's own count — nothing to surface
            IntelCount::More => active.push(CONSOLES_MORE),
            IntelCount::Fewer => active.push(CONSOLES_FEWER),
        }
        // The intel gate is a bounded knob (§4.5/§10.2): only its non-baseline
        // settings are "active", each with the direction its exposure rank implies.
        match intel_to_exit {
            IntelGate::AtLeastOne => {} // the §4.5 baseline — nothing to surface
            IntelGate::All => active.push(INTEL_GATE_ALL),
            IntelGate::None => active.push(INTEL_GATE_NONE),
        }
        active
    }

    /// **The identity for [`union`](Self::union)** — a contribution that asks for
    /// nothing: every toggle off, and every knob at the value that adds no pressure.
    ///
    /// **This is not [`default`](Self::default), and the difference is a trap worth
    /// naming** (#207). The default is the *game's* baseline — §4.5's
    /// [`IntelGate::AtLeastOne`], the middle of the exposure axis — and union composes
    /// the gate *harder-ward*, so a source built from the default silently asks for a
    /// gate one rung tighter than [`None`](IntelGate::None). A campaign facility, whose
    /// chosen set says `None` because intel is currency (§2.2), would then have its exit
    /// locked by a *flavour* that never mentioned the exit at all.
    ///
    /// So every **contributing source** ([`ModifierSources`]) starts from this, and only
    /// the `chosen` source — the one that speaks for the whole run — starts from the
    /// default. A source is a set of *departures*; this is the empty one.
    #[must_use]
    pub fn neutral() -> Self {
        Self {
            intel_to_exit: IntelGate::None,
            ..Self::default()
        }
    }

    /// Compose two contributions into one active set. A toggle is active if
    /// **any** source requests it (field-wise OR) — sources add pressure, they do
    /// not cancel each other. When bounded knobs arrive they compose
    /// *harder-ward* (take the value further in its documented direction); add
    /// that here field by field as each knob lands, so the rule stays one place.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            guards_always_search_hideouts: self.guards_always_search_hideouts
                || other.guards_always_search_hideouts,
            sighting_lost_calls_a_guard: self.sighting_lost_calls_a_guard
                || other.sighting_lost_calls_a_guard,
            body_found_calls_two_guards: self.body_found_calls_two_guards
                || other.body_found_calls_two_guards,
            always_show_vision_cones: self.always_show_vision_cones
                || other.always_show_vision_cones,
            // A knob whose baseline is a neutral middle, composed like the guard count
            // (#233): a source resting on §11.5a's schematic stayed quiet and yields,
            // and when two sources disagree the *hidden* end wins — an easier source
            // cannot hand back a layout a harder one asked to take away.
            layout_knowledge: self.layout_knowledge.harder_of(other.layout_knowledge),
            // Retired (#442): composed like any other toggle so the slot keeps
            // round-tripping, but nothing reads the result.
            calm_guards_detect_only_their_cone: self.calm_guards_detect_only_their_cone
                || other.calm_guards_detect_only_their_cone,
            automatic_doors: self.automatic_doors || other.automatic_doors,
            guards_watch_consoles: self.guards_watch_consoles || other.guards_watch_consoles,
            show_search_areas: self.show_search_areas || other.show_search_areas,
            // A bounded knob composes *harder-ward* (§12.6): take the value further
            // in its documented direction, so sources add pressure, never cancel.
            // For the guard count that reads as "the end that departs from the
            // baseline wins, pressure breaking a tie" — the same promise over a knob
            // whose baseline is a neutral middle (#232).
            guard_count: self.guard_count.harder_of(other.guard_count),
            // The same rule with the ends swapped (#207): scarcity is this knob's
            // hard direction, so a flavour asking for fewer consoles cannot be talked
            // out of it by a source that asked for more.
            intel_count: self.intel_count.harder_of(other.intel_count),
            // The reward knob (#209): a quiet source yields and the larger count wins,
            // so what the map advertised, the facility holds. See `CacheCount::most_of`
            // for why this one does not compose harder-ward like its neighbours.
            caches: self.caches.most_of(other.caches),
            // A plain toggle like the ones above (#236): one source asking for the lock
            // is enough, and no source can talk another out of it.
            prize_room_locked: self.prize_room_locked || other.prize_room_locked,
            intel_to_exit: self.intel_to_exit.harder_of(other.intel_to_exit),
        }
    }
}

/// The independent *sources* that contribute modifiers to a facility, composed
/// into one resolved [`LevelModifiers`] at facility start.
///
/// This is the **activation hook** (§12.6): the campaign alert (#210) owns the
/// *mapping* from alert level to a modifier contribution and drops it into
/// [`alert`](Self::alert), and node flavour (#207) drops its own into
/// [`flavour`](Self::flavour). Each stays a distinct source — alert is endogenous,
/// choice is exogenous, flavour is per-node — and [`resolve`](Self::resolve) is the
/// single place they merge, so no source grows a private knob set the seam should own.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModifierSources {
    /// **Choice** — the player's chosen or seeded baseline (this crate's source).
    pub chosen: LevelModifiers,
    /// **Alert** — the campaign-alert contribution (#210), or `None` when no
    /// campaign layer is driving difficulty (all of v1 quick play).
    pub alert: Option<LevelModifiers>,
    /// **Flavour** — what the map node you chose makes of the facility (#207), or
    /// `None` outside a campaign. A [`Flavour`](crate::Flavour)'s whole mechanical
    /// existence is the set it puts here: the offer on the map screen and the facility
    /// you walk into are then the same statement, one drawn and one played.
    pub flavour: Option<LevelModifiers>,
}

impl ModifierSources {
    /// The choice source alone — the v1 quick-play path: a chosen baseline, no
    /// alert, no flavour. `ModifierSources::chosen(LevelModifiers::default())` is
    /// the plain default game.
    #[must_use]
    pub fn chosen(chosen: LevelModifiers) -> Self {
        Self {
            chosen,
            alert: None,
            flavour: None,
        }
    }

    /// Resolve every source into the one active set the systems read. Sources
    /// compose by [`LevelModifiers::union`]; a new source is composed in here.
    #[must_use]
    pub fn resolve(self) -> LevelModifiers {
        let mut active = self.chosen;
        if let Some(alert) = self.alert {
            active = active.union(alert);
        }
        if let Some(flavour) = self.flavour {
            active = active.union(flavour);
        }
        active
    }
}

/// **Debug modifiers** — playtest-only switches over what is *drawn*, deliberately
/// kept apart from [`LevelModifiers`].
///
/// A level modifier bends the **rules** and is part of a level's identity: it is
/// resolved from sources at facility start, some are read at the generation seam
/// (§12.6), and every one of them travels in the shareable level-seed token
/// ([`LevelSeed`](crate::LevelSeed), #245). A debug modifier is none of that. It bends
/// only what the **player perceives** — never the facility, the guards, or the seed's
/// stream — so a run under one plays exactly the run it plays without one, and the
/// only thing that differs is how much of it you get to watch. It is never encoded
/// into a [`LevelSeed`](crate::LevelSeed), so no shared level, typed token or `?seed=`
/// link can turn it on: the only way to get one is to bake it into a build (the
/// artifact-build skill's `assemble.py --debug reveal`), which is what makes it safe
/// to be as blunt as it is.
///
/// The separation is the whole point. As a `LevelModifiers` field the reveal would
/// need a bit in the token, so "try this level" could quietly hand someone a game
/// with the fog lifted; it would join the set the generation seam reads; and the
/// compile-time enumeration of modifier read sites (§12.2) would start listing a
/// switch that no rule may ever consult. Two types, two rules: **a level modifier
/// changes the game, a debug modifier changes only what you get to see of it.**
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugModifiers {
    /// See the **whole level**: the player's field of view (§6) becomes every cell of
    /// the facility, so a playtest build can be watched instead of played blind.
    ///
    /// It is stated as *sight*, not as a drawing rule — the sight phase substitutes a
    /// full [`VisibleSet`](crate::vision::VisibleSet) and everything downstream
    /// follows on its own, with no special case anywhere else. So the fog lifts
    /// (§11.5a: contents draw, live and in their ordinary colours — there is no dim
    /// second layer to read), every guard reads as **Seen** (§9.2) and therefore draws
    /// its full state-coloured `g`, and the §11.5 danger overlay paints every one of
    /// their cones. The picture is exactly the picture a player standing there with
    /// impossible eyes would get.
    ///
    /// What it does **not** touch is the facility: guards look with their own cones,
    /// detect what they would have detected and walk the same beats, so the run plays
    /// identically — which is the whole reason watching one is worth anything. Seeing
    /// everything is not being everywhere.
    pub reveal_whole_level: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_baseline_default_has_every_modifier_off() {
        let baseline = LevelModifiers::default();
        // A knob's baseline is the recipe's own count — the §10.2 game, untouched.
        assert_eq!(baseline.guard_count, GuardCount::Baseline);
        assert!(!baseline.guards_always_search_hideouts);
        assert!(!baseline.always_show_vision_cones);
        // A debug modifier is off by default too — the fog is on unless a build
        // deliberately baked the reveal in.
        assert!(!DebugModifiers::default().reveal_whole_level);
        // The intel gate's baseline is the §4.5 [START] "at least one" — the game
        // exactly as it plays without the modifier system.
        assert_eq!(baseline.intel_to_exit, IntelGate::AtLeastOne);
    }

    /// The gate composes *harder-ward* (§12.6): the further-along-the-exposure
    /// value wins, so the quick-play `All` survives beside a campaign `None` rather
    /// than either cancelling the other. `All` > `AtLeastOne` > `None`.
    #[test]
    fn the_intel_gate_composes_harder_ward() {
        assert_eq!(IntelGate::All.harder_of(IntelGate::None), IntelGate::All);
        assert_eq!(
            IntelGate::None.harder_of(IntelGate::AtLeastOne),
            IntelGate::AtLeastOne
        );
        assert_eq!(
            IntelGate::AtLeastOne.harder_of(IntelGate::AtLeastOne),
            IntelGate::AtLeastOne
        );
        assert!(IntelGate::All > IntelGate::AtLeastOne);
        assert!(IntelGate::AtLeastOne > IntelGate::None);
    }

    #[test]
    fn the_choice_source_resolves_to_exactly_its_chosen_set() {
        let chosen = LevelModifiers {
            guards_always_search_hideouts: true,
            ..LevelModifiers::default()
        };
        // No alert, no flavour: the resolved set is the choice, untouched.
        assert_eq!(ModifierSources::chosen(chosen).resolve(), chosen);
    }

    #[test]
    fn the_alert_hook_composes_on_top_of_the_chosen_set() {
        // #210 drives this path: the player chose the easier overlay, and a loud
        // raid switches on the harder search for the next facility. Both survive
        // the merge — sources add, they do not cancel.
        let chosen = LevelModifiers {
            always_show_vision_cones: true,
            ..LevelModifiers::default()
        };
        let alert = LevelModifiers {
            guards_always_search_hideouts: true,
            ..LevelModifiers::default()
        };
        let flavour = LevelModifiers {
            intel_count: IntelCount::More,
            ..LevelModifiers::default()
        };
        let resolved = ModifierSources {
            chosen,
            alert: Some(alert),
            flavour: Some(flavour),
        }
        .resolve();
        assert!(resolved.always_show_vision_cones);
        assert!(resolved.guards_always_search_hideouts);
    }

    /// #248: the active set is **derived** from the fields, each described with its
    /// documented direction — so flipping any single modifier surfaces exactly it on
    /// the help card, and the baseline surfaces nothing. This mirrors the derived
    /// glyph legend: the display cannot drift from the resolved value, and adding a
    /// field is a compile error in `active` until it is described.
    #[test]
    fn the_active_set_describes_each_modifier_by_direction() {
        // Baseline: nothing is active — the card reads "none active".
        assert!(LevelModifiers::default().active().is_empty());

        // A harder toggle, alone, surfaces exactly itself in the Harder direction.
        let harder = LevelModifiers {
            guards_always_search_hideouts: true,
            ..LevelModifiers::default()
        };
        assert_eq!(
            harder.active(),
            vec![ActiveModifier {
                name: "Guards search hideouts",
                direction: ModifierDirection::Harder,
                detail: None,
            }]
        );

        // An easier toggle carries the Easier direction, its colour cue on the card.
        let easier = LevelModifiers {
            always_show_vision_cones: true,
            ..LevelModifiers::default()
        };
        assert_eq!(
            easier.active(),
            vec![ActiveModifier {
                name: "All vision cones shown",
                direction: ModifierDirection::Easier,
                detail: None,
            }]
        );

        // The intel gate is a bounded knob: its two non-baseline settings each carry
        // a direction *and* a rendered value; resting at the baseline surfaces nothing.
        let all = LevelModifiers {
            intel_to_exit: IntelGate::All,
            ..LevelModifiers::default()
        };
        assert_eq!(
            all.active(),
            vec![ActiveModifier {
                name: "Intel to exit",
                direction: ModifierDirection::Harder,
                detail: Some("all of it"),
            }]
        );
        let none = LevelModifiers {
            intel_to_exit: IntelGate::None,
            ..LevelModifiers::default()
        };
        assert_eq!(none.active()[0].direction, ModifierDirection::Easier);
        assert_eq!(none.active()[0].detail, Some("none required"));

        // The guard count is the second bounded knob, and its two ends carry opposite
        // directions off the one field.
        let more = LevelModifiers {
            guard_count: GuardCount::More,
            ..LevelModifiers::default()
        };
        assert_eq!(
            more.active(),
            vec![ActiveModifier {
                name: "Guards",
                direction: ModifierDirection::Harder,
                detail: Some("one more"),
            }]
        );
        let fewer = LevelModifiers {
            guard_count: GuardCount::Fewer,
            ..LevelModifiers::default()
        };
        assert_eq!(fewer.active()[0].direction, ModifierDirection::Easier);
        assert_eq!(fewer.active()[0].detail, Some("one fewer"));

        // Several sources at once: every active field is listed, in reading order.
        // **Twelve, not thirteen, with every field set** — `calm_guards_detect_only_their_cone`
        // is the retired slot 5 (#442), and a retired toggle announces nothing: what it
        // asked for is the rule the level plays regardless, so a caption for it would
        // tell the player about a difference that no longer exists.
        let stacked = LevelModifiers {
            guards_always_search_hideouts: true,
            sighting_lost_calls_a_guard: true,
            body_found_calls_two_guards: true,
            always_show_vision_cones: true,
            layout_knowledge: LayoutKnowledge::Full,
            calm_guards_detect_only_their_cone: true,
            automatic_doors: true,
            guards_watch_consoles: true,
            show_search_areas: true,
            guard_count: GuardCount::More,
            intel_count: IntelCount::Fewer,
            caches: CacheCount::Three,
            prize_room_locked: true,
            intel_to_exit: IntelGate::All,
        };
        assert_eq!(stacked.active().len(), 13);
        assert!(
            !stacked
                .active()
                .iter()
                .any(|m| m.name.contains("cone only") || m.detail == Some("flanks blind")),
            "the retired slot must never surface a caption",
        );
    }

    /// The §12.6 directed pool (#297) and [`LevelModifiers::active`] describe the
    /// **same** modifiers: switching a pool entry on surfaces exactly that entry's
    /// caption and nothing else. This is what keeps the pool from becoming the second
    /// hand-kept list the ticket set out to avoid — a caption that drifted from the
    /// field its `set` writes fails here rather than in a draw nobody reads.
    #[test]
    fn every_pool_entry_flips_exactly_the_modifier_its_caption_names() {
        for entry in &POOL {
            let mut modifiers = LevelModifiers::default();
            (entry.set)(&mut modifiers);
            assert_eq!(
                modifiers.active(),
                vec![entry.caption],
                "{} set something else",
                entry.caption.name,
            );
        }
        // The pool covers every live toggle — the fields, less the retired slot 5,
        // which asks for the rule the level plays regardless (#442) — plus the guard
        // knob's two ends, one on each side (#232) and the layout knob's two (#233/#518).
        //
        // **The sides are no longer the same depth, and that is the shape of #518.** The
        // three modifiers admitted there are all `Harder`, so the harder side went from
        // five to **eight** while the easier side stayed at **four**. Nothing lies about
        // it — [`Difficulty::blurb`](crate::Difficulty::blurb) counts *picks*, not pool
        // depth, so both ±2 stops still promise exactly the two rules they will bend —
        // but a `+N` run now has meaningfully more variety than a `−N` one, and the
        // easier side is the one to grow next.
        assert_eq!(POOL.len(), 12);
        assert_eq!(pool_size(ModifierDirection::Harder), 8);
        assert_eq!(pool_size(ModifierDirection::Easier), 4);
        assert_eq!(
            pool_size(ModifierDirection::Harder) + pool_size(ModifierDirection::Easier),
            POOL.len(),
            "every pool entry has a direction the draw can ask for",
        );
        // The intel gate stays out of the pool — a knob the draw cannot relax, and no
        // entry may claim its caption (see [`POOL`]).
        assert!(
            !POOL.iter().any(|t| t.caption.name == INTEL_GATE_ALL.name),
            "the intel gate is not a pool entry",
        );
    }

    #[test]
    fn union_is_a_field_wise_or() {
        let a = LevelModifiers {
            guards_always_search_hideouts: true,
            sighting_lost_calls_a_guard: true,
            body_found_calls_two_guards: true,
            always_show_vision_cones: false,
            layout_knowledge: LayoutKnowledge::Plans,
            calm_guards_detect_only_their_cone: false,
            automatic_doors: false,
            guards_watch_consoles: false,
            show_search_areas: false,
            guard_count: GuardCount::Baseline,
            intel_count: IntelCount::Baseline,
            caches: CacheCount::Two,
            prize_room_locked: true,
            intel_to_exit: IntelGate::All,
        };
        let b = LevelModifiers {
            guards_always_search_hideouts: false,
            sighting_lost_calls_a_guard: false,
            body_found_calls_two_guards: false,
            always_show_vision_cones: true,
            layout_knowledge: LayoutKnowledge::Full,
            calm_guards_detect_only_their_cone: true,
            automatic_doors: true,
            guards_watch_consoles: true,
            show_search_areas: true,
            guard_count: GuardCount::Fewer,
            intel_count: IntelCount::More,
            caches: CacheCount::None,
            prize_room_locked: false,
            intel_to_exit: IntelGate::None,
        };
        let both = a.union(b);
        assert!(both.guards_always_search_hideouts);
        assert!(both.always_show_vision_cones);
        // The bounded gate composes harder-ward, not by OR: `All` wins over `None`.
        assert_eq!(both.intel_to_exit, IntelGate::All);
        // The guard knob is the case a plain maximum would get wrong: `a` asked for
        // nothing, `b` asked for fewer guards, and the source that stayed quiet does
        // not get to overrule the one that spoke.
        assert_eq!(both.guard_count, GuardCount::Fewer);
        // And the intel knob the same way, with its ends the other way up: `b` asked
        // for one more console and nothing objected, so one more it is (#207).
        assert_eq!(both.intel_count, IntelCount::More);
        // The reward knob (#209): `a` stocks two crates and `b` asked for none, so two
        // it is — a source that stayed quiet cannot take away a reward another offered.
        assert_eq!(both.caches, CacheCount::Two);
        // Union with the baseline changes nothing.
        assert_eq!(a.union(LevelModifiers::default()), a);
    }

    /// The guard knob's composition rule (#232): the **end that departs from the
    /// baseline wins, and pressure breaks a tie**. The invariant it exists to keep is
    /// the §12.6 one — no contribution can relieve pressure another one asked for —
    /// stated over a knob whose baseline is a neutral middle rather than one end of an
    /// axis, which is why a plain maximum is not the rule here.
    #[test]
    fn the_guard_knob_composes_without_a_quiet_source_overruling_a_loud_one() {
        use GuardCount::{Baseline, Fewer, More};
        // A source that asks for nothing yields to one that names an end — either end.
        assert_eq!(Baseline.harder_of(Fewer), Fewer);
        assert_eq!(Fewer.harder_of(Baseline), Fewer);
        assert_eq!(Baseline.harder_of(More), More);
        assert_eq!(More.harder_of(Baseline), More);
        // A genuine disagreement resolves *harder-ward*: the campaign alert (#210)
        // cannot be talked out of its extra guard by the player's easier choice.
        assert_eq!(More.harder_of(Fewer), More);
        assert_eq!(Fewer.harder_of(More), More);
        // Agreement composes to itself, and the baseline is the identity.
        assert_eq!(Fewer.harder_of(Fewer), Fewer);
        assert_eq!(More.harder_of(More), More);
        assert_eq!(Baseline.harder_of(Baseline), Baseline);
        // Commutative over the whole knob — sources compose in any order.
        for a in [Fewer, Baseline, More] {
            for b in [Fewer, Baseline, More] {
                assert_eq!(a.harder_of(b), b.harder_of(a), "{a:?} / {b:?}");
            }
        }
        // And the whole rule, once more through the seam the systems actually read:
        // an alert asking for one more guard survives a choice asking for one fewer.
        let resolved = ModifierSources {
            chosen: LevelModifiers {
                guard_count: Fewer,
                ..LevelModifiers::default()
            },
            flavour: None,
            alert: Some(LevelModifiers {
                guard_count: More,
                ..LevelModifiers::default()
            }),
        }
        .resolve();
        assert_eq!(resolved.guard_count, More);
    }

    /// The layout knob's composition (#233), the guard knob's rule over the knowledge
    /// axis: a source resting on §11.5a's plans yields to either end, and when two
    /// sources genuinely disagree the **hidden** end wins — an easier source may not
    /// hand back a building a harder one asked to take away.
    #[test]
    fn the_layout_knob_hides_when_two_sources_disagree() {
        use LayoutKnowledge::{Full, None, Plans};
        assert_eq!(Plans.harder_of(Full), Full);
        assert_eq!(Full.harder_of(Plans), Full);
        assert_eq!(Plans.harder_of(None), None);
        assert_eq!(None.harder_of(Plans), None);
        // The disagreement, which is the case the knob exists to make answerable at
        // all: two bools could be asked for both at once with nothing to say which won.
        assert_eq!(Full.harder_of(None), None);
        assert_eq!(None.harder_of(Full), None);
        // Agreement composes to itself, the baseline is the identity, and the whole
        // knob is commutative — sources compose in any order.
        assert_eq!(Full.harder_of(Full), Full);
        assert_eq!(None.harder_of(None), None);
        assert_eq!(Plans.harder_of(Plans), Plans);
        for a in [Full, Plans, None] {
            for b in [Full, Plans, None] {
                assert_eq!(a.harder_of(b), b.harder_of(a), "{a:?} / {b:?}");
            }
        }
        // Through the seam the renderer actually reads: a campaign alert that hid the
        // layout survives a choice that asked for it.
        let resolved = ModifierSources {
            chosen: LevelModifiers {
                layout_knowledge: Full,
                ..LevelModifiers::default()
            },
            flavour: Option::None,
            alert: Some(LevelModifiers {
                layout_knowledge: None,
                ..LevelModifiers::neutral()
            }),
        }
        .resolve();
        assert_eq!(resolved.layout_knowledge, None);
    }

    /// Each end of the layout knob surfaces its own caption with its own direction
    /// (#248/#233), and the baseline announces nothing — §11.5a's schematic is the
    /// picture every other run plays, so there is no departure to state.
    ///
    /// The harder end is also the one caption a player most needs *before* turn one:
    /// the board opens nearly empty, and a card that did not say so would read as a
    /// broken render.
    #[test]
    fn each_end_of_the_layout_knob_announces_itself() {
        assert!(LevelModifiers::default().active().is_empty());
        let with = |knowledge| {
            LevelModifiers {
                layout_knowledge: knowledge,
                ..LevelModifiers::default()
            }
            .active()
        };
        assert_eq!(with(LayoutKnowledge::Plans), vec![]);
        assert_eq!(with(LayoutKnowledge::Full), vec![KNOWS_FULL_LAYOUT]);
        assert_eq!(with(LayoutKnowledge::None), vec![LAYOUT_UNKNOWN]);
        assert_eq!(KNOWS_FULL_LAYOUT.direction, ModifierDirection::Easier);
        assert_eq!(LAYOUT_UNKNOWN.direction, ModifierDirection::Harder);
        // Both captions are in the table the help card's width bound measures, or the
        // card could clip a caption nothing checks.
        assert!(CAPTIONS.contains(&KNOWS_FULL_LAYOUT));
        assert!(CAPTIONS.contains(&LAYOUT_UNKNOWN));
    }

    /// **Every modifier that is not in the pool is out for a stated reason** (#518), and
    /// this pins the list so that "why is this missing?" always has an answer in
    /// [`POOL`]'s own doc rather than only on a field somewhere.
    ///
    /// The list shrank when the three held-back entries were admitted, and the criterion
    /// is what shrank it: membership asks whether an entry is a *difficulty* change that
    /// bends in a documented direction, and **not** whether the §13.2 bot can weigh it
    /// (§13.4 — the sim is a smoke detector, not a judge). What is left out is left out
    /// on mechanics, not on measurability.
    #[test]
    fn what_is_out_of_the_pool_is_out_on_mechanics_and_not_on_measurability() {
        let drawable = |c: ActiveModifier| POOL.iter().any(|e| e.caption.name == c.name);

        // The intel gate: quick play already sets `All` and union composes harder-ward,
        // so an easier draw could not relax it — the pool cannot reach a knob it can
        // only ever tighten.
        assert!(!drawable(INTEL_GATE_ALL) && !drawable(INTEL_GATE_NONE));
        // The reward knobs: both move what a run *wins*, not the pressure on it, and the
        // cache count would hand a campaign a permanent ability from inside one facility.
        assert!(!drawable(CONSOLES_MORE) && !drawable(CONSOLES_FEWER));
        assert!(!drawable(ONE_CACHE) && !drawable(TWO_CACHES) && !drawable(THREE_CACHES));

        // …and the three admitted by #518 really are reachable now, at every stop that
        // draws in their direction. All three are `Harder`, so a `−N` must never find one.
        for caption in [ALL_DOORS_AUTOMATIC, LAYOUT_UNKNOWN, LOCKED_PRIZE_ROOM] {
            assert!(drawable(caption), "{} is not in the pool", caption.name);
            assert_eq!(caption.direction, ModifierDirection::Harder);
        }
        let harder_can_draw = |pick: fn(&LevelModifiers) -> bool| {
            (0..400u64).any(|seed| pick(&crate::Difficulty::MuchHarder.draw(seed)))
        };
        assert!(harder_can_draw(|m| m.automatic_doors));
        assert!(harder_can_draw(
            |m| m.layout_knowledge == LayoutKnowledge::None
        ));
        assert!(harder_can_draw(|m| m.prize_room_locked));
        for difficulty in [crate::Difficulty::Easier, crate::Difficulty::MuchEasier] {
            for seed in [0, 7, 4242, u64::MAX] {
                let drawn = difficulty.draw(seed);
                assert!(
                    !drawn.automatic_doors
                        && drawn.layout_knowledge != LayoutKnowledge::None
                        && !drawn.prize_room_locked,
                    "a {:?} draw on seed {seed} bent a harder rule",
                    difficulty.label(),
                );
            }
        }
    }

    /// The layout knob is in the pool at **both** ends now (#233/#518), one caption each,
    /// and they are on opposite sides of the direction filter — so no single draw can
    /// ever name both, and the codec's "a knob holds one value" rejection is never asked
    /// to arbitrate a set the draw produced.
    #[test]
    fn the_layout_knob_is_in_the_pool_at_both_ends_and_never_at_once() {
        assert_eq!(KNOWS_FULL_LAYOUT.direction, ModifierDirection::Easier);
        assert_eq!(LAYOUT_UNKNOWN.direction, ModifierDirection::Harder);
        for difficulty in crate::Difficulty::ALL {
            for seed in [0, 7, 4242, u64::MAX] {
                // A draw resolves the knob to exactly one value by construction; this is
                // the assertion that the two rows cannot conspire to ask for both.
                let drawn = difficulty.draw(seed).layout_knowledge;
                assert!(matches!(
                    drawn,
                    LayoutKnowledge::Plans | LayoutKnowledge::Full | LayoutKnowledge::None
                ));
                if difficulty.direction() == Some(ModifierDirection::Easier) {
                    assert_ne!(drawn, LayoutKnowledge::None);
                }
                if difficulty.direction() == Some(ModifierDirection::Harder) {
                    assert_ne!(drawn, LayoutKnowledge::Full);
                }
            }
        }
    }
}
