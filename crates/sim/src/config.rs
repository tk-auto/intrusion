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

use intrusion_core::{AbilityId, IntelGate, LevelConfig, LevelModifiers, LevelSeed, Loadout};

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
    /// loosely ([`normalise`]) so `full-layout-known`, `full_layout_known` and
    /// `Full Layout Known` all name the same field.
    ///
    /// Modifiers only ever go on: the baseline is every one of them off, so "off" is
    /// what not naming it already means.
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
const MODIFIERS: [(&str, SetModifier); 5] = [
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
    ("full-layout-known", |m| m.full_layout_known = true),
];

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

    /// Every modifier has a name, and each name flips **its own** field.
    ///
    /// The destructure is the obligation: a new [`LevelModifiers`] field will not
    /// compile here until somebody names it, and then the assertion fails until
    /// [`MODIFIERS`] carries it. The gate is excluded on purpose — it is a bounded
    /// knob with its own flag, not a toggle.
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
            full_layout_known,
            intel_to_exit,
        } = all.modifiers;
        assert!(guards_always_search_hideouts);
        assert!(sighting_lost_calls_a_guard);
        assert!(body_found_calls_two_guards);
        assert!(always_show_vision_cones);
        assert!(full_layout_known);
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
        assert!(error.contains("full-layout-known"), "{error}");

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
            "full-layout-known",
            "full_layout_known",
            "Full Layout Known",
        ] {
            assert!(
                RunConfig::sim()
                    .with_modifier(spelling)
                    .expect("a known modifier")
                    .modifiers
                    .full_layout_known,
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
            .with_tech("camouflage,decoy,vision,dephase")
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
