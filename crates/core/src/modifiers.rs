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
//! - **Alert** (endogenous) — the campaign alert (#210): a loud raid raises the
//!   alert, and a higher alert switches on harder modifiers for later facilities.
//!   [`ModifierSources`] exposes the hook it plugs into; it is `None` here.
//! - **Flavour** (per-node) — a facility's own character (#207), a third future
//!   source.
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
    /// **Easier.** Hand over the **full layout** (§11.5a): geometry the player has
    /// never had eyes on draws as the real building rather than as the schematic
    /// `≈`/`~`, so doorways, duct mouths and furniture are all on the map from turn
    /// one. Exactly the picture the game gave everyone before the schematic landed
    /// (#307), which is what makes the modifier easy to state and easy to price.
    ///
    /// It reveals the **layout and nothing else**: a console and a cupboard are
    /// contents, still hidden until seen (§11.5a), so this never shortcuts the
    /// scouting that finds the objectives — it removes the *architectural* unknown
    /// only. The knowledge state on the seam stays truthful either way: an
    /// unexplored cell still reports itself unexplored, it is simply drawn in full.
    ///
    /// **It has to be paid for.** Route-planning through unscouted wings is a real
    /// advantage, so this sits on the *easier* side of the §12.6 directed pool: under
    /// the difficulty draw it spends budget that must be found by taking a harder
    /// rule elsewhere. A modifier that only ever gave would not be a modifier.
    pub full_layout_known: bool,
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
pub(crate) const CAPTIONS: [ActiveModifier; 7] = [
    SEARCHES_HIDEOUTS,
    CALLS_IN_SIGHTINGS,
    CALLS_IN_BODIES,
    SHOWS_ALL_CONES,
    KNOWS_FULL_LAYOUT,
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
            full_layout_known,
            calm_guards_detect_only_their_cone,
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
        if full_layout_known {
            active.push(KNOWS_FULL_LAYOUT);
        }
        // Slot 5 is **retired** (#442) — see the field's own note. A run that
        // decodes a token with the bit set gets no caption, because there is nothing
        // to announce: what the bit asked for is the rule the level is already
        // playing.
        let _ = calm_guards_detect_only_their_cone;
        // The intel gate is a bounded knob (§4.5/§10.2): only its non-baseline
        // settings are "active", each with the direction its exposure rank implies.
        match intel_to_exit {
            IntelGate::AtLeastOne => {} // the §4.5 baseline — nothing to surface
            IntelGate::All => active.push(INTEL_GATE_ALL),
            IntelGate::None => active.push(INTEL_GATE_NONE),
        }
        active
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
            full_layout_known: self.full_layout_known || other.full_layout_known,
            // Retired (#442): composed like any other toggle so the slot keeps
            // round-tripping, but nothing reads the result.
            calm_guards_detect_only_their_cone: self.calm_guards_detect_only_their_cone
                || other.calm_guards_detect_only_their_cone,
            // A bounded knob composes *harder-ward* (§12.6): take the value further
            // in its documented direction, so sources add pressure, never cancel.
            intel_to_exit: self.intel_to_exit.harder_of(other.intel_to_exit),
        }
    }
}

/// The independent *sources* that contribute modifiers to a facility, composed
/// into one resolved [`LevelModifiers`] at facility start.
///
/// This is the **activation hook** (§12.6): the campaign alert (#210) owns the
/// *mapping* from alert level to a modifier contribution and drops it into
/// [`alert`](Self::alert); node flavour (#207) is a future field. Each stays a
/// distinct source — alert is endogenous, choice is exogenous, flavour is
/// per-node — and [`resolve`](Self::resolve) is the single place they merge, so
/// no source grows a private knob set the seam should own.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModifierSources {
    /// **Choice** — the player's chosen or seeded baseline (this crate's source).
    pub chosen: LevelModifiers,
    /// **Alert** — the campaign-alert contribution (#210), or `None` when no
    /// campaign layer is driving difficulty (all of v1 quick play).
    pub alert: Option<LevelModifiers>,
    // Flavour (#207) is the third source: `pub flavour: Option<LevelModifiers>`.
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
        let resolved = ModifierSources {
            chosen,
            alert: Some(alert),
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

        // Several sources at once: every active field is listed, in reading order.
        // **Six, not seven, with every field set** — `calm_guards_detect_only_their_cone`
        // is the retired slot 5 (#442), and a retired toggle announces nothing: what it
        // asked for is the rule the level plays regardless, so a caption for it would
        // tell the player about a difference that no longer exists.
        let stacked = LevelModifiers {
            guards_always_search_hideouts: true,
            sighting_lost_calls_a_guard: true,
            body_found_calls_two_guards: true,
            always_show_vision_cones: true,
            full_layout_known: true,
            calm_guards_detect_only_their_cone: true,
            intel_to_exit: IntelGate::All,
        };
        assert_eq!(stacked.active().len(), 6);
        assert!(
            !stacked
                .active()
                .iter()
                .any(|m| m.name.contains("cone only") || m.detail == Some("flanks blind")),
            "the retired slot must never surface a caption",
        );
    }

    #[test]
    fn union_is_a_field_wise_or() {
        let a = LevelModifiers {
            guards_always_search_hideouts: true,
            sighting_lost_calls_a_guard: true,
            body_found_calls_two_guards: true,
            always_show_vision_cones: false,
            full_layout_known: false,
            calm_guards_detect_only_their_cone: false,
            intel_to_exit: IntelGate::All,
        };
        let b = LevelModifiers {
            guards_always_search_hideouts: false,
            sighting_lost_calls_a_guard: false,
            body_found_calls_two_guards: false,
            always_show_vision_cones: true,
            full_layout_known: true,
            calm_guards_detect_only_their_cone: true,
            intel_to_exit: IntelGate::None,
        };
        let both = a.union(b);
        assert!(both.guards_always_search_hideouts);
        assert!(both.always_show_vision_cones);
        // The bounded gate composes harder-ward, not by OR: `All` wins over `None`.
        assert_eq!(both.intel_to_exit, IntelGate::All);
        // Union with the baseline changes nothing.
        assert_eq!(a.union(LevelModifiers::default()), a);
    }
}
