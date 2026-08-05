//! What a batch boots (§13.2/#256): the run config as an **input**, not a preset
//! compiled into the harness.
//!
//! Every run used to boot `LevelSeed::sim(seed)` — the baseline modifiers and the
//! bare innate-only loadout — so the sim could measure exactly one configuration of
//! the game. The toggle machinery it would need to vary already existed in core:
//! [`LevelModifiers`] (each field carrying a direction), [`IntelGate`], [`Loadout`],
//! all composed into a [`LevelSeed`] and carried by one shareable token (§12.4/#245).
//! What was missing was a way for a *batch* to reach it.
//!
//! [`RunConfig`] is that reach. It is the sim preset by default — [`RunConfig::sim`]
//! is derived from [`LevelSeed::sim`] rather than re-stated, so the two cannot drift
//! — and every field is a knob the CLI can turn:
//!
//! - the **facility recipe** ([`LevelConfig`], §10.2) — the guard count and the rest
//!   of the carve, already swept through `--guards`;
//! - the **modifiers** (#225) — the rules bending each run, including the intel gate;
//! - the **loadout** (§8.3/#244) — the tech every run in the batch holds.
//!
//! The seed is deliberately *not* here. A batch is a range of seeds over **one**
//! config, so the config is what stays fixed while the seed varies: `config.level(seed)`
//! is the [`LevelSeed`] a run boots from, and generation is seed-derived and
//! independent of modifiers and loadout (proven in core's `level_seed.rs`), so
//! varying the config never shifts the facility a seed carves. That is the property
//! a paired A/B (#257) rests on.

use intrusion_core::{
    AbilityId, AlertTuning, GuardCount, IntelCount, IntelGate, LayoutKnowledge, LevelConfig,
    LevelModifiers, LevelSeed, Loadout,
};

/// What every run in a batch boots from (§13.2): the facility recipe, the modifiers
/// bending the run, and the abilities it holds — everything but the seed.
///
/// [`Default`] is the **sim preset** (§13.3), so a batch given no config is the batch
/// the sim has always run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RunConfig {
    /// The §10.2 recipe the facility is carved from — v1 with the guard count as the
    /// knob the balance sweep drives.
    pub facility: LevelConfig,
    /// The level modifiers active on every run of the batch (#225).
    pub modifiers: LevelModifiers,
    /// The abilities every run of the batch holds (§8.3/#244).
    pub abilities: Loadout,
    /// The §7.3 alert ladder's **[START]** thresholds (#376) — how hard each rung is
    /// to reach. The sim preset is the shipped set, so a batch naming no knob measures
    /// the ladder the game ships.
    ///
    /// Deliberately *not* carried by [`level`](Self::level): no level-seed token can
    /// encode it (§12.4/#245), so a swept batch is an instrument reading rather than a
    /// run anyone could be handed — the same honest gap the facility recipe has.
    /// [`capture_one_with`](crate::capture_one_with) says so where it matters.
    pub alert: AlertTuning,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self::sim()
    }
}

impl RunConfig {
    /// The **sim preset** (§13.2/§13.3): the v1 recipe, the baseline modifiers (the
    /// intel gate at [`IntelGate::AtLeastOne`], which keeps the bot's outcome profile
    /// mixed) and the **innate-only** loadout — Run and nothing salvaged, so a batch
    /// measures the core stealth loop rather than what a lucky tech draw papers over.
    ///
    /// Read off [`LevelSeed::sim`] rather than re-stated, so there is exactly one
    /// definition of "the sim preset" and this cannot quietly drift from the core's.
    pub fn sim() -> Self {
        let preset = LevelSeed::sim(0);
        Self {
            facility: LevelConfig::V1,
            modifiers: preset.modifiers,
            abilities: preset.abilities,
            alert: AlertTuning::default(),
        }
    }

    /// The [`LevelSeed`] one run of this batch boots from — the config applied to
    /// `seed`. This is what replaces the harness's hardcoded `LevelSeed::sim(seed)`,
    /// and what a captured replay carries (#245), so `--emit-replay` under a
    /// non-default config round-trips to the run it actually played.
    pub fn level(&self, seed: u64) -> LevelSeed {
        LevelSeed {
            seed,
            modifiers: self.modifiers,
            abilities: self.abilities,
        }
    }

    /// Take the modifiers and loadout from a decoded level-seed token (#245/#333).
    ///
    /// The token's **seed is not taken here**: a batch runs a range of seeds, so the
    /// seed is the CLI's business (it stands in as the first seed unless `--seed`
    /// says otherwise) and the config's business is only what stays fixed across it.
    #[must_use]
    pub fn with_preset(self, preset: LevelSeed) -> Self {
        Self {
            modifiers: preset.modifiers,
            abilities: preset.abilities,
            ..self
        }
    }

    /// The same config with the guard count set — the §10.2 recipe knob `--guards`
    /// drives, holding the rest of v1.
    #[must_use]
    pub fn with_guards(self, guards: usize) -> Self {
        Self {
            facility: LevelConfig {
                guards,
                ..self.facility
            },
            ..self
        }
    }

    /// The same config with the exit's intel gate set (§4.5/§10.2).
    #[must_use]
    pub fn with_intel_gate(self, gate: IntelGate) -> Self {
        Self {
            modifiers: LevelModifiers {
                intel_to_exit: gate,
                ..self.modifiers
            },
            ..self
        }
    }

    /// The same config with the named modifier switched **on**, or the vocabulary as
    /// an error. Names are the [`LevelModifiers`] field names in kebab case, matched
    /// loosely ([`normalise`]) so `layout-knowledge-full`, `layout_knowledge_full` and
    /// `Layout Knowledge Full` all name the same field.
    ///
    /// Modifiers only ever go on: the baseline is every one of them off, so "off" is
    /// what not naming it already means. A **knob**'s ends are two names over one
    /// field (#232), so naming both leaves the one named last — the plain reading of a
    /// command line that asked for two things which cannot both be true.
    pub fn with_modifier(self, name: &str) -> Result<Self, String> {
        let wanted = normalise(name);
        let entry = MODIFIERS
            .iter()
            .find(|(known, _)| normalise(known) == wanted);
        let Some((_, set)) = entry else {
            return Err(format!(
                "unknown modifier {name}; known modifiers: {}",
                modifier_names(),
            ));
        };
        let mut modifiers = self.modifiers;
        set(&mut modifiers);
        Ok(Self { modifiers, ..self })
    }

    /// The same config with one §7.3 alert threshold set (#376), written
    /// `name=value` — the knob a sweep turns without a rebuild.
    ///
    /// Names are the [`AlertTuning`] field names in kebab case, matched loosely
    /// ([`normalise`]) like every other vocabulary here. The ladder is **not**
    /// validated per knob, deliberately: a dwell range is two knobs, and
    /// `dwell-turns-min=4 dwell-turns-max=6` would fail halfway through if each
    /// setting had to stand alone. [`validated`](Self::validated) checks the finished
    /// ladder, once, after every flag has been applied.
    pub fn with_alert(self, setting: &str) -> Result<Self, String> {
        let (name, value) = setting.split_once('=').ok_or_else(|| {
            format!(
                "{setting}: an alert knob is written name=value; known knobs: {}",
                alert_knob_names(),
            )
        })?;
        let wanted = normalise(name);
        let Some((_, set)) = ALERT_KNOBS
            .iter()
            .find(|(known, _)| normalise(known) == wanted)
        else {
            return Err(format!(
                "unknown alert knob {name}; known knobs: {}",
                alert_knob_names(),
            ));
        };
        let value: u32 = value
            .trim()
            .parse()
            .map_err(|_| format!("{name}: not a number: {value}"))?;
        let mut alert = self.alert;
        set(&mut alert, value);
        Ok(Self { alert, ..self })
    }

    /// This config, if it describes a game the design allows — or the rule it breaks.
    ///
    /// Run **once**, after every flag has been applied, because that is the only point
    /// at which a multi-knob setting (the alerted dwell range) is whole. A batch that
    /// would measure a game §7.3/§7.5 forbids — a dwell floor of 0, a sighting window
    /// too short to ever hold a sighting — is refused at the flag rather than run and
    /// reported: numbers from a game the design does not admit answer nothing (§13.2).
    pub fn validated(self) -> Result<Self, String> {
        self.alert.validate()?;
        self.holdable()
    }

    /// The same config holding **exactly** the tech in `list` (plus the innate set,
    /// which §8.3 makes unconditional), or an error naming the vocabulary.
    ///
    /// `list` is comma-separated ability names. The preset's own tech is dropped
    /// rather than added to: `--abilities decoy` means "a run holding Decoy", not
    /// "whatever the preset held, and Decoy" — which is the only reading that lets a
    /// command line state the loadout it is measuring.
    pub fn with_tech(self, list: &str) -> Result<Self, String> {
        let mut abilities = Loadout::innate();
        for name in split_list(list) {
            abilities = abilities.with(ability_named(name)?);
        }
        Self { abilities, ..self }.holdable()
    }

    /// The same config with the tech in `list` removed — the without-form, for asking
    /// what a loadout is worth by taking one verb out of it (#257).
    ///
    /// Naming an **innate** ability is an error rather than a silent no-op: §8.3 makes
    /// the innate set unconditional, and the level-seed token cannot even describe a
    /// run without it, so a batch that claimed to have dropped Run would be lying.
    pub fn without_tech(self, list: &str) -> Result<Self, String> {
        let mut abilities = Loadout::empty();
        let dropped: Vec<AbilityId> = split_list(list)
            .map(ability_named)
            .collect::<Result<_, _>>()?;
        if let Some(innate) = dropped.iter().find(|id| id.is_innate()) {
            return Err(format!(
                "{} is innate (§8.3) — every run holds it",
                innate.name(),
            ));
        }
        for id in self.abilities.iter().filter(|id| !dropped.contains(id)) {
            abilities = abilities.with(id);
        }
        Ok(Self { abilities, ..self })
    }

    /// This config, if it is one a **run can hold** — or the reason it is not.
    ///
    /// The §8.3 cap is [`AbilityId::MAX_TECH_HELD`], and a batch over it would be
    /// measuring a game nothing can produce: the ability bar is not sized for it
    /// (§11.4) and the level-seed token cannot carry it, so `--emit-replay` would
    /// print a replay that decodes to nothing. Refusing at the flag is the honest
    /// point to refuse.
    ///
    /// Errors here name the *fault*, never the flag — the caller knows which flag it
    /// is applying and prefixes accordingly, so a message never claims the wrong one.
    fn holdable(self) -> Result<Self, String> {
        let held = self.abilities.iter().filter(|id| !id.is_innate()).count();
        if held > AbilityId::MAX_TECH_HELD {
            return Err(format!(
                "{held} tech named, but a run holds at most {} (§8.3)",
                AbilityId::MAX_TECH_HELD,
            ));
        }
        Ok(self)
    }
}

/// The `--modifier` vocabulary: each name paired with the field it switches on.
///
/// The names are the [`LevelModifiers`] field names, spelled in kebab case — verbose,
/// and deliberately so: one concept, one spelling, and a reader of a command line can
/// find the field it names by searching for it.
type SetModifier = fn(&mut LevelModifiers);
const MODIFIERS: [(&str, SetModifier); 13] = [
    ("guards-always-search-hideouts", |m| {
        m.guards_always_search_hideouts = true
    }),
    ("sighting-lost-calls-a-guard", |m| {
        m.sighting_lost_calls_a_guard = true
    }),
    ("body-found-calls-two-guards", |m| {
        m.body_found_calls_two_guards = true
    }),
    ("always-show-vision-cones", |m| {
        m.always_show_vision_cones = true
    }),
    // The **layout knob**'s two ends (§11.5a/#307/#233), one name each, spelled off the
    // field as the guard knob's are — `full-layout-known` named a field that no longer
    // exists, and a name that cannot be searched for is the one thing this table's
    // convention forbids.
    ("layout-knowledge-full", |m| {
        m.layout_knowledge = LayoutKnowledge::Full
    }),
    // **The bot cannot honestly play this end** (`docs/bot-behaviour.md` §2): it is
    // granted geometry unconditionally, on the authority of the §11.5a rule this
    // modifier overrides, so with the layout hidden it still routes through walls it
    // has never seen. The name is here because a modifier the harness cannot even name
    // is one nobody can look at — `--inspect` and a replay still show what the *player*
    // would be shown — but a batch that names it measures the bot, not the game (§13.3).
    // Teaching the bot to explore unknown geometry is its own ticket.
    ("layout-knowledge-none", |m| {
        m.layout_knowledge = LayoutKnowledge::None
    }),
    // The one modifier that is read by **generation** rather than at runtime
    // (§12.6/#452): it decides what a doorway is, so a batch that names it carves a
    // different facility from the same seed. That is the point of measuring it.
    ("automatic-doors", |m| m.automatic_doors = true),
    // Read at the patrol-destination seam (§7.5/#319), so a batch that names it plays
    // the baseline's building with the same guards walking different legs — the
    // strongest frame a directional assertion can be stated in.
    ("guards-watch-consoles", |m| m.guards_watch_consoles = true),
    // The **guard-count knob**'s two ends (§10.2/#232), one name each — a knob is not
    // a toggle, so the name has to say which end. Read by placement rather than by the
    // carve, so a batch that names one plays the *same building* as the baseline with
    // one guard added or dropped: exactly the comparison appendix 26's `--guards`
    // sweep makes, now reachable as a level modifier rather than as a recipe override.
    ("guard-count-more", |m| m.guard_count = GuardCount::More),
    ("guard-count-fewer", |m| m.guard_count = GuardCount::Fewer),
    // The **intel-count knob**'s two ends (§10.2/#207) — the campaign map's reward axis,
    // reachable here for the same reason the guard knob is: it changes what placement
    // seats and nothing about the carve, so a batch that names one plays the baseline's
    // building with a console added or dropped. Worth sweeping on its own account under
    // `--intel-gate all`, where a console is an objective rather than loot.
    ("intel-count-more", |m| m.intel_count = IntelCount::More),
    ("intel-count-fewer", |m| m.intel_count = IntelCount::Fewer),
    // Purely a **render** modifier (§11.5/#224): it paints the §7.6 search areas and
    // bends no rule, so a batch that names it plays the baseline run exactly. It is
    // named here anyway because the bot reads the board through the player's own
    // channels (§13.2), so what the board shows is a legitimate thing to sweep on the
    // day the bot learns to route around a search — see `docs/bot-behaviour.md`.
    ("show-search-areas", |m| m.show_search_areas = true),
    // `calm-guards-detect-only-their-cone` is **not** here: slot 5 is retired (#442)
    // and its rule is the baseline, so a name that set it would offer the operator a
    // sweep that measures nothing. The destructure in the test below still names the
    // field, so the compiler keeps this decision deliberate rather than forgotten.
    //
    // The **cache count** is **not** here either (#209), for a sharper reason. The bot
    // has no cue for a crate: it would walk past every one, so a batch that named this
    // would measure a facility with a few cells of floor missing and a bot that cannot
    // see the thing under test — the §13.3 trap of measuring the bot rather than the
    // game. Caches are a **campaign** reward (§2.2) and the sim plays single facilities,
    // so there is nothing to sweep until the bot learns to salvage; the name lands with
    // that cue, not before it.
];

/// The `--alert` vocabulary (§7.3/#376): each threshold paired with the field it
/// sets. The names are the [`AlertTuning`] field names in kebab case, on the same
/// one-concept-one-spelling rule the modifiers follow.
type SetAlert = fn(&mut AlertTuning, u32);
const ALERT_KNOBS: [(&str, SetAlert); 8] = [
    ("sighting-contact-turns", |a, v| {
        a.sighting_contact_turns = v
    }),
    ("sighting-window-turns", |a, v| a.sighting_window_turns = v),
    ("sightings-for-second-rung", |a, v| {
        a.sightings_for_second_rung = v
    }),
    ("silent-posts-for-third-rung", |a, v| {
        a.silent_posts_for_third_rung = v
    }),
    ("dwell-turns-min", |a, v| a.dwell_turns_min = v),
    ("dwell-turns-max", |a, v| a.dwell_turns_max = v),
    ("rung-two-reinforcements", |a, v| {
        a.rung_two_reinforcements = v as usize
    }),
    ("rung-three-reinforcements", |a, v| {
        a.rung_three_reinforcements = v as usize
    }),
];

/// Every `--alert` knob name, read off the table above so a threshold added to §7.3
/// is spellable and documented the day it lands.
pub fn alert_knob_names() -> String {
    ALERT_KNOBS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every `--modifier` name, for the usage text and for an unknown name's error.
pub fn modifier_names() -> String {
    MODIFIERS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The `--intel-gate` vocabulary (§4.5): the three [`IntelGate`] values, named for
/// how much intel the exit asks for rather than for the variant that carries it.
const INTEL_GATES: [(&str, IntelGate); 3] = [
    ("none", IntelGate::None),
    ("one", IntelGate::AtLeastOne),
    ("all", IntelGate::All),
];

/// The intel gate a `--intel-gate` value names, or `None` for a value that names no
/// gate. Total over the vocabulary, so there is no "near enough" fallback.
pub fn intel_gate_named(name: &str) -> Option<IntelGate> {
    let wanted = normalise(name);
    INTEL_GATES
        .iter()
        .find(|(known, _)| normalise(known) == wanted)
        .map(|(_, gate)| *gate)
}

/// Every `--intel-gate` value, for the usage text and for a bad value's error.
pub fn intel_gate_names() -> String {
    INTEL_GATES
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The ability a name refers to, matched against the §8.3 catalog by
/// [`AbilityId::name`] — or the whole vocabulary as an error.
fn ability_named(name: &str) -> Result<AbilityId, String> {
    let wanted = normalise(name);
    AbilityId::ALL
        .into_iter()
        .find(|id| normalise(id.name()) == wanted)
        .ok_or_else(|| {
            format!(
                "unknown ability {name}; known abilities: {}",
                ability_names()
            )
        })
}

/// Every ability name, for the usage text and for an unknown name's error. Read off
/// the catalog rather than written down, so a new §8.1 row is spellable the day it
/// ships.
///
/// Spelled in kebab case (`pierce-wall`) to match how the modifier names read, not in
/// the normalised form the matcher compares — a vocabulary is for a person to type.
pub fn ability_names() -> String {
    AbilityId::ALL
        .into_iter()
        .map(|id| id.name().to_lowercase().replace(' ', "-"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Split a comma-separated list, dropping the empty pieces a trailing or doubled
/// comma leaves — `decoy,` and `decoy,,vision` are the list they obviously mean, and
/// an empty list is an empty set rather than an error.
fn split_list(list: &str) -> impl Iterator<Item = &str> {
    list.split(',')
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
}

/// A name reduced to what it *says*: lowercase, with spaces, hyphens and underscores
/// dropped. One spelling rule for every vocabulary here, so `Pierce Wall`,
/// `pierce-wall` and `piercewall` are the same ability and a field name can be typed
/// the way the source spells it or the way a flag does.
fn normalise(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default config **is** the sim preset, for every seed — the tie that stops
    /// `RunConfig::sim` and `LevelSeed::sim` drifting apart. A batch given no config
    /// boots exactly the level the harness used to hardcode.
    #[test]
    fn the_default_config_boots_the_sim_preset() {
        let config = RunConfig::default();
        assert_eq!(config.facility, LevelConfig::V1);
        for seed in [0, 1, 42, 8371] {
            assert_eq!(config.level(seed), LevelSeed::sim(seed));
        }
    }

    /// #376: every §7.3 alert threshold has a knob name, and each name sets **its
    /// own** field — so a sweep of one threshold moves one threshold.
    ///
    /// The destructure is the obligation: a new [`AlertTuning`] field will not compile
    /// here until somebody names it, and then the assertion fails until
    /// [`ALERT_KNOBS`] carries it.
    #[test]
    fn every_alert_threshold_has_a_knob_that_sets_it() {
        for (name, _) in ALERT_KNOBS {
            // 7 is a value no shipped threshold already holds, so "it changed" and
            // "it changed the right one" are the same assertion.
            let config = RunConfig::sim()
                .with_alert(&format!("{name}=7"))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            let AlertTuning {
                sighting_contact_turns,
                sighting_window_turns,
                sightings_for_second_rung,
                silent_posts_for_third_rung,
                dwell_turns_min,
                dwell_turns_max,
                rung_two_reinforcements,
                rung_three_reinforcements,
            } = config.alert;
            let moved = [
                sighting_contact_turns,
                sighting_window_turns,
                sightings_for_second_rung,
                silent_posts_for_third_rung,
                dwell_turns_min,
                dwell_turns_max,
                rung_two_reinforcements as u32,
                rung_three_reinforcements as u32,
            ]
            .iter()
            .filter(|&&v| v == 7)
            .count();
            assert_eq!(moved, 1, "{name} did not set exactly one threshold");
        }
        assert!(
            ALERT_KNOBS
                .iter()
                .all(|(name, _)| alert_knob_names().contains(name)),
            "a knob is unspellable from the usage text",
        );
    }

    /// The knob names are matched on what they *say*, like every other vocabulary
    /// here — and an unknown one is refused **with the vocabulary**, never applied to
    /// the nearest field. A batch swept on a knob that silently did nothing would
    /// report a flat curve for a threshold that never moved (§13.4).
    #[test]
    fn an_alert_knob_is_matched_loosely_and_refused_loudly() {
        for spelling in [
            "sighting-window-turns=14",
            "sighting_window_turns=14",
            "Sighting Window Turns = 14",
        ] {
            let config = RunConfig::sim().with_alert(spelling).expect("a known knob");
            assert_eq!(config.alert.sighting_window_turns, 14, "{spelling}");
        }

        let error = RunConfig::sim()
            .with_alert("window=4")
            .expect_err("no such knob");
        assert!(error.contains("window"), "{error}");
        assert!(error.contains("sighting-window-turns"), "{error}");

        let error = RunConfig::sim()
            .with_alert("sighting-window-turns=soon")
            .expect_err("not a number");
        assert!(error.contains("not a number"), "{error}");

        let error = RunConfig::sim()
            .with_alert("sighting-window-turns")
            .expect_err("no value");
        assert!(error.contains("name=value"), "{error}");
    }

    /// §7.3/§7.5: a ladder the design forbids is refused at the flag rather than
    /// measured — but only once the **whole** config is in, so a dwell range spelled
    /// across two knobs is not rejected halfway through being written.
    #[test]
    fn an_illegal_ladder_is_refused_once_the_config_is_whole() {
        let half_written = RunConfig::sim()
            .with_alert("dwell-turns-min=4")
            .expect("a known knob");
        assert!(
            half_written.validated().is_err(),
            "a floor above the ceiling is not a ladder",
        );
        assert_eq!(
            half_written
                .with_alert("dwell-turns-max=6")
                .expect("a known knob")
                .validated()
                .map(|c| (c.alert.dwell_turns_min, c.alert.dwell_turns_max)),
            Ok((4, 6)),
            "…and the finished range is legal",
        );

        let error = RunConfig::sim()
            .with_alert("dwell-turns-min=0")
            .expect("a known knob")
            .validated()
            .expect_err("the §7.5 floor");
        assert!(error.contains("never removed"), "{error}");
    }

    /// Every modifier has a name, and each name flips **its own** field.
    ///
    /// The destructure is the obligation: a new [`LevelModifiers`] field will not
    /// compile here until somebody names it, and then the assertion fails until
    /// [`MODIFIERS`] carries it. Two fields are excluded on purpose — the intel gate,
    /// a bounded knob with its own flag rather than a toggle, and the **retired** slot
    /// 5 (#442), whose rule is the baseline and which therefore has no name to offer.
    /// The guard-count knob (#232) is in, with one name per end.
    #[test]
    fn every_modifier_toggle_has_a_name_that_flips_it() {
        // One name at a time: exactly one modifier goes active (§12.6's `active` set
        // is derived from the fields, so "exactly one" is checked against the core).
        for (name, _) in MODIFIERS {
            let config = RunConfig::sim()
                .with_modifier(name)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                config.modifiers.active().len(),
                1,
                "{name} did not flip exactly one modifier",
            );
        }

        // Every name at once: every toggle is on, named field by field.
        let mut all = RunConfig::sim();
        for (name, _) in MODIFIERS {
            all = all.with_modifier(name).expect("a known modifier");
        }
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
            intel_to_exit,
        } = all.modifiers;
        assert!(guards_always_search_hideouts);
        assert!(sighting_lost_calls_a_guard);
        assert!(body_found_calls_two_guards);
        assert!(always_show_vision_cones);
        assert!(automatic_doors);
        assert!(guards_watch_consoles);
        assert!(show_search_areas);
        // The knob's two ends are two names over one field, so naming both leaves the
        // one named last rather than accumulating — see [`RunConfig::with_modifier`].
        assert_eq!(guard_count, GuardCount::Fewer);
        assert_eq!(intel_count, IntelCount::Fewer);
        assert_eq!(layout_knowledge, LayoutKnowledge::None);
        assert!(
            !calm_guards_detect_only_their_cone,
            "the retired slot has no name, so naming every modifier must not set it",
        );
        assert_eq!(
            caches,
            intrusion_core::CacheCount::None,
            "the cache knob has no name until the bot has a cue for it (#209), so naming \
             every modifier must not plant one",
        );
        assert_eq!(
            intel_to_exit,
            RunConfig::sim().modifiers.intel_to_exit,
            "a toggle must not move the gate — that is --intel-gate's job",
        );
    }

    /// A name nothing answers to is refused **with the vocabulary**, never dropped:
    /// a batch silently running the baseline while its command line claims a modifier
    /// is the failure this error exists to prevent (§13.2 attribution).
    #[test]
    fn an_unknown_name_is_refused_with_the_vocabulary() {
        let error = RunConfig::sim()
            .with_modifier("deafen-the-guards")
            .expect_err("no such modifier");
        assert!(error.contains("deafen-the-guards"), "{error}");
        assert!(error.contains("layout-knowledge-full"), "{error}");

        let error = RunConfig::sim()
            .with_tech("smoke-grenade")
            .expect_err("no such ability");
        assert!(error.contains("smoke-grenade"), "{error}");
        assert!(error.contains("camouflage"), "{error}");

        assert_eq!(intel_gate_named("some"), None);
    }

    /// Names are matched by what they *say*: case, spaces, hyphens and underscores
    /// are noise, so a field name can be typed the way the source spells it.
    #[test]
    fn a_name_is_matched_loosely() {
        let held = |config: Result<RunConfig, String>| {
            config
                .expect("a known ability")
                .abilities
                .iter()
                .collect::<Vec<_>>()
        };
        let expected = vec![AbilityId::Run, AbilityId::PierceWall];
        for spelling in ["pierce-wall", "Pierce Wall", "piercewall", "PIERCE_WALL"] {
            assert_eq!(
                held(RunConfig::sim().with_tech(spelling)),
                expected,
                "{spelling}"
            );
        }
        for spelling in [
            "layout-knowledge-full",
            "layout_knowledge_full",
            "Layout Knowledge Full",
        ] {
            assert_eq!(
                RunConfig::sim()
                    .with_modifier(spelling)
                    .expect("a known modifier")
                    .modifiers
                    .layout_knowledge,
                LayoutKnowledge::Full,
                "{spelling}",
            );
        }
    }

    /// `--abilities` states the **whole** tech set, so it replaces the preset's rather
    /// than adding to it; the innate set survives regardless (§8.3).
    #[test]
    fn the_named_tech_is_the_whole_loadout() {
        let config = RunConfig::sim()
            .with_tech("camouflage,decoy")
            .expect("known abilities")
            .with_tech("vision")
            .expect("known abilities");
        assert_eq!(
            config.abilities.iter().collect::<Vec<_>>(),
            vec![AbilityId::Run, AbilityId::Vision],
            "the second list replaced the first",
        );
        // An empty list is a bare run, not an error — the sim's own baseline.
        assert_eq!(
            RunConfig::sim()
                .with_tech("")
                .expect("an empty list")
                .abilities,
            Loadout::innate(),
        );
    }

    /// The without-form drops exactly what it names, and refuses to pretend it can
    /// drop an innate ability (§8.3 — the token cannot even describe its absence).
    #[test]
    fn the_without_form_drops_tech_but_never_the_innate_set() {
        let config = RunConfig::sim()
            .with_tech("camouflage,decoy,vision")
            .expect("known abilities")
            .without_tech("decoy")
            .expect("known abilities");
        assert_eq!(
            config.abilities.iter().collect::<Vec<_>>(),
            vec![AbilityId::Run, AbilityId::Camouflage, AbilityId::Vision],
        );
        let error = config.without_tech("run").expect_err("Run is innate");
        assert!(error.contains("innate"), "{error}");
    }

    /// A loadout over the §8.3 cap is refused at the flag rather than run: the bar is
    /// not sized for it (§11.4) and the level-seed token cannot carry it, so
    /// `--emit-replay` would print a replay that decodes to nothing.
    #[test]
    fn a_loadout_over_the_cap_is_refused() {
        let error = RunConfig::sim()
            .with_tech("camouflage,decoy,vision,phase-out")
            .expect_err("four tech is over the cap of three");
        assert!(error.contains("at most 3"), "{error}");
        // The cap itself is fine, and encodes — which is what `--emit-replay` needs.
        let at_cap = RunConfig::sim()
            .with_tech("camouflage,decoy,vision")
            .expect("three tech is holdable");
        assert!(at_cap.level(42).encode().is_some());
    }

    /// A config round-trips through the level-seed token (#245): the preset a token
    /// carries is the preset a batch runs, which is what makes `--config` reproduce a
    /// shared run rather than approximate it.
    #[test]
    fn a_config_round_trips_through_a_token() {
        let config = RunConfig::sim()
            .with_intel_gate(IntelGate::All)
            .with_modifier("guards-always-search-hideouts")
            .expect("a known modifier")
            .with_tech("camouflage,vision")
            .expect("known abilities");
        let token = config.level(8371).encode().expect("a holdable config");
        let decoded = LevelSeed::decode(&token).expect("its own token");
        assert_eq!(RunConfig::sim().with_preset(decoded), config);
        assert_eq!(decoded.seed, 8371, "the token still names its seed");
    }

    /// The recipe knob is part of the config, and nothing but the recipe: `--guards`
    /// must not disturb the modifiers or the loadout it shares a struct with.
    #[test]
    fn the_guard_count_moves_only_the_recipe() {
        let config = RunConfig::sim().with_guards(9);
        assert_eq!(config.facility.guards, 9);
        assert_eq!(
            RunConfig {
                facility: LevelConfig::V1,
                ..config
            },
            RunConfig::sim(),
        );
    }
}
