//! The **level-seed string** (§12.4 / §13.1 / #244 / #245): a run's whole
//! reproducible starting config — the seed, the active modifiers, and the ability
//! loadout — as one compact, shareable token.
//!
//! # Why the seed alone stopped being enough
//!
//! §12.4 [SETTLED] made a run `(seed, [inputs])`: the seed fixed the facility, so
//! "try seed 8371" reproduced a level exactly (§13.1). Level modifiers (#225) and
//! quick play's seeded ability grant (#244) broke that — the same seed under a
//! *different* modifier set or loadout is a *different* level. The reproducible
//! unit is now `(seed, modifiers, abilities)` and, with the replay stream, a run is
//! `(config, [inputs])`. This module is that config: [`LevelSeed`] bundles the three
//! pieces, and [`encode`](LevelSeed::encode)/[`decode`](LevelSeed::decode) carry
//! them in one string so a handed-around link reproduces the run *exactly*, not just
//! its geometry.
//!
//! # The three pieces compose to one token
//!
//! - a **seed** (`u64`) — the §12.4 source the facility is carved from;
//! - the active **[`LevelModifiers`]** (#225) — the rules bending this run;
//! - the ability **[`Loadout`]** (#244) — the economy abilities the run holds.
//!
//! The string is versioned (`L1-…`) so it can grow — campaign state (§14 v3) is a
//! later field — and **old links degrade predictably** rather than mis-parsing. It
//! is deliberately compact and URL-safe (unreserved characters only), so it drops
//! straight into the #110 seed surface and the #197 replay carrier without a second
//! scheme, and it is **backward compatible**: a bare decimal seed still decodes, as
//! the default preset — quick play (#244).
//!
//! # One boot path
//!
//! [`start_level`] turns a [`LevelSeed`] into a running [`State`] — the single boot
//! the web shell, the replay viewer, and the headless sim all call, so a seed here
//! is the very facility any of them plays (§13.2). A run's identity lives in one
//! place, and there is no second "almost the same" boot to drift from it.

use crate::ability::{AbilityId, Loadout};
use crate::cell::Direction;
use crate::generate::{generate_level, GenError};
use crate::modifiers::{IntelGate, LevelModifiers};
use crate::place::LevelConfig;
use crate::rng::Rng;
use crate::state::State;

/// The number of salvaged-tech abilities quick play grants at start (#244) — the
/// `starting_abilities` count knob. [`AbilityId::TECH`] has outgrown the grant, so
/// it is a **seeded draw of three of the pool** (§8.3) — a run holds a subset of the
/// tech, not all of it. The pool's size is deliberately not written down here: it
/// grows every time a tech ships, and a number in this comment would be wrong within
/// a ticket of being written (it already was, twice).
///
/// It *is* [`AbilityId::MAX_TECH_HELD`] rather than a second three: the cap is what
/// the ability bar is sized against (§11.4), so a grant that outgrew it would have
/// to answer to the bar's compile-time width bound.
const QUICK_PLAY_TECH_GRANT: usize = AbilityId::MAX_TECH_HELD;

/// A fixed transform applied to the run seed before drawing the quick-play ability
/// loadout, so the draw takes from a sub-stream **independent** of the generation
/// stream (§12.4). This is what keeps granting a *subset* of tech from ever shifting
/// the facility a seed carves — the two streams never share a position — while the
/// whole run still derives from the one seed (§12.4 rule 1).
const LOADOUT_STREAM_SALT: u64 = 0x_10AD_0000_10AD_0000;

/// The current level-seed string format version. Bumped when the token's fields
/// change; an older or unknown tag decodes to `None` (a graceful fall to a fresh
/// run, #110/#197) rather than mis-reading new fields as old.
const FORMAT_TAG: &str = "L1";

/// A run's whole reproducible starting config (§12.4/#245): the three pieces that
/// compose to a shareable [level-seed string](self) — the seed, the active
/// modifiers, and the ability loadout.
///
/// Everything random in a run derives from `seed` (§12.4); `modifiers` and
/// `abilities` are the config that now also shapes it (#225/#244). A [`LevelSeed`]
/// plus a replay's `[inputs]` reproduces a run byte-for-byte.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LevelSeed {
    /// The run's random seed (§12.4) — the facility is carved from it.
    pub seed: u64,
    /// The level modifiers active for the run (#225) — the rules bending it.
    pub modifiers: LevelModifiers,
    /// The economy abilities the run holds (#244) — its loadout.
    pub abilities: Loadout,
}

impl LevelSeed {
    /// The **quick-play** preset for `seed` (#244) — v1's default mode and the
    /// default a bare seed decodes to. A named preset = a modifier bundle over the
    /// base rules: the intel gate at [`IntelGate::All`] (gather all the intel, then
    /// get out, §10.2), plus the innate set and a seeded draw of
    /// [`QUICK_PLAY_TECH_GRANT`] tech (§8.3). Fully seed-derived, so the same seed
    /// always yields the same quick-play run — which is why a bare seed can stand in
    /// for the whole token.
    pub fn quick_play(seed: u64) -> Self {
        Self {
            seed,
            modifiers: LevelModifiers {
                intel_to_exit: IntelGate::All,
                ..LevelModifiers::default()
            },
            abilities: quick_play_loadout(seed),
        }
    }

    /// The **headless-sim** preset for `seed` (§13.2/§13.3): the baseline rules
    /// (the intel gate at [`IntelGate::AtLeastOne`], which keeps the bot's outcome
    /// profile mixed) and the **innate-only** loadout — Run and nothing salvaged.
    /// The sim baseline is deliberately *bare*: a level must be winnable with no
    /// tech (§8.3), so the bot's win rate measures the game's core stealth loop, not
    /// what a lucky tech draw papers over. Runs that want to weigh a specific tech
    /// add it back explicitly. The web shell boots quick play (§10.2's seeded tech
    /// grant); the sim boots this — same facility from the same seed, a different
    /// objective and no tech.
    pub fn sim(seed: u64) -> Self {
        Self {
            seed,
            modifiers: LevelModifiers::default(),
            abilities: Loadout::innate(),
        }
    }

    /// Encode to the compact, URL-safe [level-seed string](self).
    ///
    /// A config that is *exactly* what [`quick_play`](Self::quick_play) resolves for
    /// its seed emits the **bare decimal seed** — the maximally compact form, and the
    /// one existing `?seed=N` links and the seed box already use (a bare seed decodes
    /// straight back to the same quick-play run, so nothing is lost). Any other
    /// config emits the versioned `L1-<seed>-<mods>-<abils>` form.
    pub fn encode(&self) -> String {
        if *self == Self::quick_play(self.seed) {
            return self.seed.to_string();
        }
        self.encode_full()
    }

    /// Encode to the **versioned** `L1-<seed>-<mods>-<abils>` form, always — the
    /// same token as [`encode`](Self::encode) except that a default-preset run
    /// spells its config out instead of collapsing to the bare seed.
    ///
    /// This is for a surface that must *show what the run is*, not just carry it:
    /// the help panel's Level info tab (#272) reads a token the player can compare
    /// against the run in front of them, and a bare `8371` says nothing about the
    /// modifiers or loadout it implies. Links keep using [`encode`](Self::encode)
    /// — a bare seed is shorter and decodes to exactly the same run — so this
    /// changes what is *displayed*, never what is carried. Both forms decode
    /// (§12.4), and to the same [`LevelSeed`].
    pub fn encode_full(&self) -> String {
        let mods = to_base36(modifier_bits(self.modifiers));
        let abils: String = self.abilities.iter().map(|id| id.hotkey()).collect();
        format!("{FORMAT_TAG}-{}-{mods}-{abils}", self.seed)
    }

    /// Decode a [level-seed string](self), or `None` if it is not one.
    ///
    /// Two shapes, matching [`encode`](Self::encode): a **bare decimal seed** →
    /// the quick-play preset (#244), the backward-compatible path every existing
    /// `?seed=N` link and typed seed takes; or the versioned `L1-…` form → its exact
    /// `(seed, modifiers, abilities)`. Anything else — an unknown version tag, a
    /// malformed field — is `None`, which the seed surface and the replay carrier
    /// turn into a graceful fall to live play, never a bricked page (#110/#197).
    pub fn decode(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if let Ok(seed) = raw.parse::<u64>() {
            return Some(Self::quick_play(seed));
        }
        let mut fields = raw.split('-');
        if fields.next()? != FORMAT_TAG {
            return None; // an unknown or older version tag degrades to None
        }
        let seed = fields.next()?.parse::<u64>().ok()?;
        let modifiers = modifiers_from_bits(from_base36(fields.next()?)?)?;
        let abilities = loadout_from_codes(fields.next()?)?;
        if fields.next().is_some() {
            return None; // trailing junk: a malformed token, not a valid one
        }
        Some(Self {
            seed,
            modifiers,
            abilities,
        })
    }
}

/// Draw quick play's ability loadout from `seed` (#244): the innate set plus
/// [`QUICK_PLAY_TECH_GRANT`] tech chosen from [`AbilityId::TECH`]. Seeded off a
/// sub-stream independent of generation ([`LOADOUT_STREAM_SALT`]), so the draw is
/// deterministic yet never perturbs the facility. When the grant meets or exceeds
/// the pool every tech is granted and no randomness is drawn at all; while the pool
/// outgrows the grant, the partial draw below runs and picks a subset.
fn quick_play_loadout(seed: u64) -> Loadout {
    let mut pool = AbilityId::TECH;
    let grant = QUICK_PLAY_TECH_GRANT.min(pool.len());
    let mut loadout = Loadout::innate();
    if grant == pool.len() {
        // Grant the whole pool — a partial shuffle would draw the same set, so skip
        // it and keep the stream (and the v1 facility) untouched.
        for id in pool {
            loadout = loadout.with(id);
        }
        return loadout;
    }
    // A partial Fisher–Yates over the tech pool: draw `grant` distinct tech.
    let mut rng = Rng::new(seed ^ LOADOUT_STREAM_SALT);
    for i in 0..grant {
        let j = i + rng.below((pool.len() - i) as u32) as usize;
        pool.swap(i, j);
        loadout = loadout.with(pool[i]);
    }
    loadout
}

/// Boot a running [`State`] from a [`LevelSeed`] — the one boot path (§13.2).
///
/// `Rng::new(seed)` → [`generate_level`] with [`LevelConfig::V1`] → [`State::new`]
/// facing north, then the seed's modifiers and loadout threaded in. The generation
/// stream continues into the turn loop (§12.4/#146), exactly as before loadouts —
/// the loadout draw takes its own sub-stream, so a seed's facility is byte-identical
/// whatever the loadout. The web shell, the replay viewer, and the sim all call
/// this, so a level-seed string reproduces the *same* run everywhere.
pub fn start_level(level: &LevelSeed) -> Result<State, GenError> {
    start_level_with(&LevelConfig::V1, level)
}

/// Boot a running [`State`] from a [`LevelSeed`] under an explicit [`LevelConfig`]
/// — the same boot as [`start_level`], with the facility recipe (its size and piece
/// counts) opened up as a knob.
///
/// [`start_level`] is this called with [`LevelConfig::V1`], the one tuned v1 recipe
/// the web shell plays. The headless sim (§13.2) drives *this* to **sweep** the
/// recipe — most usefully the guard count — measuring how a knob moves the balance
/// numbers, exactly the "the knobs are data so the sim can sweep them" the config is
/// declared for (§10.2). The [`LevelSeed`] still fixes the seed, modifiers and
/// loadout; only the recipe the facility carves from differs.
pub fn start_level_with(config: &LevelConfig, level: &LevelSeed) -> Result<State, GenError> {
    let mut rng = Rng::new(level.seed);
    let (layout, placement) = generate_level(config, &mut rng)?;
    let guards = placement.guards(&layout);
    Ok(State::new(
        layout,
        placement.player(),
        Direction::North,
        guards,
        placement.intel().iter().copied(),
        placement.exit(),
    )
    .with_rng(rng)
    .with_level(*level))
}

/// Pack a [`LevelModifiers`] into a small bitfield for the token. A struct
/// destructure names every field, so a new modifier will not compile until it is
/// given a bit here (§12.2 — the compiler enumerates the encode sites).
fn modifier_bits(m: LevelModifiers) -> u32 {
    let LevelModifiers {
        guards_always_search_hideouts,
        sighting_lost_calls_a_guard,
        body_found_calls_two_guards,
        always_show_vision_cones,
        intel_to_exit,
    } = m;
    u32::from(guards_always_search_hideouts)
        | u32::from(always_show_vision_cones) << 1
        | gate_bits(intel_to_exit) << 2
        | u32::from(sighting_lost_calls_a_guard) << 4
        | u32::from(body_found_calls_two_guards) << 5
}

/// Unpack a bitfield back into a [`LevelModifiers`], or `None` if a field holds a
/// value this version has no meaning for (a token from a newer format).
fn modifiers_from_bits(bits: u32) -> Option<LevelModifiers> {
    Some(LevelModifiers {
        guards_always_search_hideouts: bits & 0b1 != 0,
        always_show_vision_cones: bits & 0b10 != 0,
        intel_to_exit: gate_from_bits((bits >> 2) & 0b11)?,
        sighting_lost_calls_a_guard: bits & 0b1_0000 != 0,
        body_found_calls_two_guards: bits & 0b10_0000 != 0,
    })
}

/// The two-bit encoding of the intel gate.
fn gate_bits(gate: IntelGate) -> u32 {
    match gate {
        IntelGate::None => 0,
        IntelGate::AtLeastOne => 1,
        IntelGate::All => 2,
    }
}

/// The intel gate for a two-bit code, or `None` for an unused code (`3`).
fn gate_from_bits(bits: u32) -> Option<IntelGate> {
    match bits {
        0 => Some(IntelGate::None),
        1 => Some(IntelGate::AtLeastOne),
        2 => Some(IntelGate::All),
        _ => None,
    }
}

/// A loadout from its ability-code string — the [`AbilityId::hotkey`] letters of the
/// held abilities, in any order. Built up from the empty loadout, so the round-trip
/// carries exactly the codes named. `None` if a letter is not an ability key or is
/// repeated, so a malformed field never silently drops or doubles an ability.
fn loadout_from_codes(codes: &str) -> Option<Loadout> {
    let mut loadout = Loadout::empty();
    for ch in codes.chars() {
        let id = AbilityId::ALL.into_iter().find(|id| id.hotkey() == ch)?;
        if loadout.contains(id) {
            return None; // a repeated ability code is malformed
        }
        loadout = loadout.with(id);
    }
    Some(loadout)
}

/// Base-36 of a small integer — compact and URL-safe (lowercase digits + letters).
fn to_base36(mut n: u32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).expect("base-36 digits are ASCII")
}

/// Parse a base-36 field back to an integer, or `None` if it is empty or holds a
/// non-base-36 character (a malformed token).
fn from_base36(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    let mut n: u32 = 0;
    for ch in s.chars() {
        let digit = ch.to_digit(36)?;
        n = n.checked_mul(36)?.checked_add(digit)?;
    }
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Outcome;

    /// The two encodings are two views of one config (#272): [`encode`] is the
    /// **link** form — bare for the default preset, so `?seed=8371` stays short —
    /// and [`encode_full`] is the **display** form, always versioned, so a surface
    /// that shows what the run *is* spells its modifiers and loadout out. Both
    /// decode back to the same [`LevelSeed`], which is what makes the choice purely
    /// cosmetic.
    ///
    /// [`encode`]: LevelSeed::encode
    /// [`encode_full`]: LevelSeed::encode_full
    #[test]
    fn the_link_form_is_compact_and_the_display_form_is_always_full() {
        let quick = LevelSeed::quick_play(8371);
        assert_eq!(quick.encode(), "8371", "the link form collapses");
        assert!(
            quick.encode_full().starts_with("L1-8371-"),
            "the display form spells the preset out: {}",
            quick.encode_full()
        );
        assert_ne!(quick.encode(), quick.encode_full());
        // Both decode to the same run — the display form loses nothing and adds no
        // way to boot something else.
        assert_eq!(LevelSeed::decode(&quick.encode()), Some(quick));
        assert_eq!(LevelSeed::decode(&quick.encode_full()), Some(quick));

        // A non-default config already emitted the full form, and still does — the
        // two agree wherever the preset is not the default.
        let custom = LevelSeed::sim(8371);
        assert_eq!(custom.encode(), custom.encode_full());
        assert_eq!(LevelSeed::decode(&custom.encode_full()), Some(custom));
    }

    /// The booted run **carries the config that booted it** (#245/#272): `start_level`
    /// records the whole [`LevelSeed`] on the state, so the help panel's token
    /// ([`LevelSeed::encode`]) reproduces this very run — and the two halves it
    /// applies, the modifiers and the loadout, agree with the recorded config by
    /// construction. A hand-built state carries none.
    #[test]
    fn a_booted_run_carries_the_level_that_booted_it() {
        for level in [
            LevelSeed::quick_play(8371),
            LevelSeed::sim(8371),
            LevelSeed {
                seed: 4242,
                modifiers: LevelModifiers {
                    always_show_vision_cones: true,
                    ..LevelModifiers::default()
                },
                abilities: Loadout::innate(),
            },
        ] {
            let state = start_level(&level).expect("the v1 recipe places");
            assert_eq!(state.level(), Some(level), "the config is recorded");
            assert_eq!(state.modifiers(), level.modifiers, "…and applied");
            assert_eq!(state.loadout(), level.abilities, "…both halves of it");
            // What the panel would show boots this run again.
            assert_eq!(
                LevelSeed::decode(&state.level().expect("a booted run").encode()),
                Some(level),
            );
        }
    }

    /// Quick play (#244): the intel gate at [`IntelGate::All`], the innate set, and a
    /// seeded draw of [`QUICK_PLAY_TECH_GRANT`] tech. The pool outgrows the grant, so
    /// the draw bites (§8.3): the run holds every innate ability plus exactly
    /// [`QUICK_PLAY_TECH_GRANT`] of the tech — a strict subset of the full loadout.
    #[test]
    fn quick_play_is_all_intel_and_the_tech_grant() {
        let level = LevelSeed::quick_play(8371);
        assert_eq!(level.modifiers.intel_to_exit, IntelGate::All);
        // Every innate ability is always granted.
        for id in AbilityId::ALL.into_iter().filter(|id| id.is_innate()) {
            assert!(level.abilities.contains(id), "{} is innate", id.name());
        }
        // Exactly QUICK_PLAY_TECH_GRANT of the tech pool are granted.
        let tech_held = AbilityId::TECH
            .into_iter()
            .filter(|&id| level.abilities.contains(id))
            .count();
        assert_eq!(
            tech_held, QUICK_PLAY_TECH_GRANT,
            "the grant is a strict subset of the pool"
        );
        // The pool now outgrows the grant, so the loadout is a strict subset.
        assert_ne!(level.abilities, Loadout::full(), "not every tech is held");
    }

    /// The sim preset (§13.3): the baseline gate ([`IntelGate::AtLeastOne`]) and the
    /// **innate-only** loadout — the bare, no-tech baseline (§8.3), so the bot's win
    /// rate measures the core stealth loop rather than a lucky tech draw.
    #[test]
    fn the_sim_preset_is_the_baseline_gate_and_the_innate_loadout() {
        let level = LevelSeed::sim(42);
        assert_eq!(level.modifiers.intel_to_exit, IntelGate::AtLeastOne);
        assert_eq!(level.abilities, Loadout::innate());
        // Bare means bare: not one salvaged-tech ability is held.
        for tech in AbilityId::TECH {
            assert!(!level.abilities.contains(tech), "{} is tech", tech.name());
        }
        assert_eq!(level.modifiers, LevelModifiers::default());
    }

    /// A **bare decimal seed** decodes to the quick-play preset — the backward-
    /// compatible path every existing `?seed=N` link and typed seed takes (#245).
    #[test]
    fn a_bare_seed_decodes_to_quick_play() {
        assert_eq!(LevelSeed::decode("8371"), Some(LevelSeed::quick_play(8371)));
        assert_eq!(LevelSeed::decode("  42 "), Some(LevelSeed::quick_play(42)));
        assert_eq!(LevelSeed::decode("0"), Some(LevelSeed::quick_play(0)));
    }

    /// Quick play encodes to the bare seed — the maximally compact form, since a
    /// bare seed decodes straight back to the same quick-play run.
    #[test]
    fn quick_play_encodes_to_the_bare_seed() {
        assert_eq!(LevelSeed::quick_play(8371).encode(), "8371");
        // And that bare form round-trips.
        let level = LevelSeed::quick_play(8371);
        assert_eq!(LevelSeed::decode(&level.encode()), Some(level));
    }

    /// A config that is *not* quick play encodes to the versioned `L1-…` form and
    /// round-trips through it exactly — seed, every modifier, and the loadout.
    #[test]
    fn a_non_default_config_round_trips_through_the_structured_form() {
        let level = LevelSeed {
            seed: 999,
            modifiers: LevelModifiers {
                guards_always_search_hideouts: true,
                sighting_lost_calls_a_guard: true,
                body_found_calls_two_guards: true,
                always_show_vision_cones: true,
                intel_to_exit: IntelGate::None,
            },
            abilities: Loadout::innate().with(AbilityId::Dephase),
        };
        let token = level.encode();
        assert!(token.starts_with("L1-"), "structured form: {token}");
        assert_eq!(LevelSeed::decode(&token), Some(level));
    }

    /// Every combination of the modifier fields and a spread of loadouts survives
    /// the round-trip — the encode/decode is total over the config space, so no
    /// shared level can silently mutate in transit (#245).
    #[test]
    fn every_config_round_trips() {
        let gates = [IntelGate::None, IntelGate::AtLeastOne, IntelGate::All];
        let loadouts = [
            Loadout::empty(),
            Loadout::innate(),
            Loadout::full(),
            Loadout::innate().with(AbilityId::Camouflage),
            Loadout::empty()
                .with(AbilityId::Decoy)
                .with(AbilityId::Dephase),
        ];
        for search in [false, true] {
            for cones in [false, true] {
                for called in [false, true] {
                    for gate in gates {
                        for abilities in loadouts {
                            let level = LevelSeed {
                                seed: 12345,
                                modifiers: LevelModifiers {
                                    guards_always_search_hideouts: search,
                                    sighting_lost_calls_a_guard: called,
                                    body_found_calls_two_guards: called,
                                    always_show_vision_cones: cones,
                                    intel_to_exit: gate,
                                },
                                abilities,
                            };
                            assert_eq!(
                                LevelSeed::decode(&level.encode()),
                                Some(level),
                                "round-trip failed for {level:?}",
                            );
                        }
                    }
                }
            }
        }
    }

    /// A malformed or unknown token decodes to `None` — the graceful fall to a fresh
    /// run the seed surface and the replay carrier depend on (#110/#197): a bad token
    /// must never brick the page, and an older/newer version must degrade, not
    /// mis-parse.
    #[test]
    fn a_malformed_token_decodes_to_none() {
        assert_eq!(LevelSeed::decode(""), None);
        assert_eq!(
            LevelSeed::decode("L2-1-3-rcdx"),
            None,
            "unknown version tag"
        );
        assert_eq!(LevelSeed::decode("L1-1-3"), None, "too few fields");
        assert_eq!(
            LevelSeed::decode("L1-1-3-rcdx-extra"),
            None,
            "trailing junk"
        );
        assert_eq!(LevelSeed::decode("L1-notaseed-3-r"), None, "bad seed");
        assert_eq!(LevelSeed::decode("L1-1--r"), None, "empty modifier field");
        assert_eq!(LevelSeed::decode("L1-1-3-rr"), None, "a repeated ability");
        assert_eq!(LevelSeed::decode("L1-1-3-rq"), None, "q is no ability key");
        assert_eq!(LevelSeed::decode("-1.5-"), None, "not a token at all");
    }

    /// The token is compact and URL-safe: only unreserved characters (digits,
    /// lowercase letters, and the `-` separator), so it drops into a `?seed=` field
    /// or a `#seed=` hash with no escaping.
    #[test]
    fn the_token_is_url_safe_and_short() {
        let token = LevelSeed {
            seed: u64::MAX,
            modifiers: LevelModifiers {
                guards_always_search_hideouts: true,
                sighting_lost_calls_a_guard: true,
                body_found_calls_two_guards: true,
                always_show_vision_cones: true,
                intel_to_exit: IntelGate::All,
            },
            abilities: Loadout::full(),
        }
        .encode();
        assert!(
            token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "url-safe: {token}",
        );
        // Even the widest config stays well under a tweet's worth of characters.
        assert!(
            token.len() < 40,
            "compact: {token} is {} chars",
            token.len()
        );
    }

    /// Determinism (§12.4): a level-seed string reproduces the **exact** run — the
    /// same facility, modifiers, and loadout — every boot. A golden pin: decode a
    /// token, boot twice, and assert the rendered frames are byte-identical, and
    /// that booting from the decoded config matches booting from the config directly.
    #[test]
    fn a_token_reproduces_the_exact_run() {
        let level = LevelSeed::quick_play(2026);
        let token = level.encode();
        let decoded = LevelSeed::decode(&token).expect("its own token decodes");
        assert_eq!(decoded, level);

        let a = start_level(&decoded).expect("the v1 config boots");
        let b = start_level(&level).expect("the v1 config boots");
        assert_eq!(
            crate::render(&a),
            crate::render(&b),
            "the token boots the same frame as the config",
        );
        // The resolved config is threaded into the running state — the loadout the
        // token carries (a partial tech draw now, no longer the full set).
        assert_eq!(a.modifiers().intel_to_exit, IntelGate::All);
        assert_eq!(a.loadout(), level.abilities);
        assert_eq!(a.outcome(), Outcome::Playing);
    }

    /// The quick-play ability draw is deterministic and independent of the
    /// generation stream (§12.4): the same seed always draws the same loadout, and
    /// resolving the loadout never disturbs the facility a seed carves — the two boot
    /// to byte-identical frames whether or not the loadout is the full set.
    #[test]
    fn the_loadout_draw_never_perturbs_the_facility() {
        for seed in [0, 7, 2026] {
            // Same seed, same drawn loadout.
            assert_eq!(quick_play_loadout(seed), quick_play_loadout(seed));
            // Booting quick play (which draws a loadout) and the sim (which does not)
            // from the same seed carves the identical facility — the draw takes its
            // own sub-stream, so it cannot shift generation. The rendered board is
            // identical (the loadout shapes the ability line, not the map); the two
            // presets differ only in the intel gate they carry.
            let quick = start_level(&LevelSeed::quick_play(seed)).expect("boots");
            let sim = start_level(&LevelSeed::sim(seed)).expect("boots");
            assert_eq!(
                crate::render(&quick),
                crate::render(&sim),
                "seed {seed}: the loadout draw shifted the facility",
            );
        }
    }
}
