//! The **level-seed token** (§12.4 / §13.1 / #244 / #245): a run's whole
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
//! # The token (#333)
//!
//! Twelve characters, `a`–`z`, nothing else: `xtrzghtfqmvd`. Fixed width, so a
//! wrong length is rejected before anything is parsed; all-alphabetic, so there is
//! no `0`/`O` or `1`/`l` to misread; unreserved throughout, so it drops straight
//! into the #110 seed surface and the #197 replay carrier with no escaping.
//!
//! The token is a **mixed-radix chain**: each field is pushed as a digit
//! (`value = value * radix + digit`), the whole value is scrambled, and the result
//! is written in base 26. Digits pop off in reverse, which is why a field whose
//! radix depends on another is pushed *first* — the count digits are read before
//! the combination indexes they size. In push order:
//!
//! | Field | Radix |
//! |---|---|
//! | seed | `2^`[`SEED_BITS`] |
//! | innate abilities | `2^`[`AbilityId::INNATE`]`.len()` — a bitset; the innate set is not capped |
//! | intel gate | [`GATE_VARIANTS`] |
//! | modifier combination | `C(`[`MODIFIER_TOGGLES`]`, count)` |
//! | modifier count | [`MODIFIER_CAP`]` + 1` |
//! | tech combination | `C(`[`AbilityId::TECH`]`.len(), count)` |
//! | tech count | [`AbilityId::MAX_TECH_HELD`]` + 1` |
//! | check | `2^`[`CHECK_BITS`] |
//!
//! **Held sets are combination indexes, not bitsets.** A bitset costs one bit per
//! catalogue entry whether set or not, so its length tracks the *roster*; a
//! combination index costs `log2(C(n, k))`, so its length tracks the *cap* and grows
//! only logarithmically in the roster. At today's sizes the two are the same twelve
//! characters — the seed dominates, and the config is under twelve bits either way —
//! so this buys nothing yet. It is here because retrofitting it later is a format
//! break, and because the cap is the thing that is actually settled (§8.3).
//!
//! **The counts are stored, and cost nothing.** Because a combination digit's radix
//! is `C(n, count)`, the chain's total is exactly `Σₖ C(n, k)` — identical to leaving
//! the count implicit. So the count is spelled out in the token and asserted on the
//! way back in, for free.
//!
//! **The magic is what makes a roster change loud.** [`MAGIC`] folds the format major
//! version, the roster sizes, *and the caps* into the check field. The caps have to
//! be in there: under a combination encoding they are radices, so moving
//! [`AbilityId::MAX_TECH_HELD`] would otherwise reinterpret every token ever shared.
//! This is not hypothetical — it already happened once. When the Vision passive
//! joined the tech pool (#286, six tech becoming seven entries in `ALL`), every
//! previously shared `#seed=8371` link began booting a *different* loadout, because
//! the seeded draw re-ran over a pool that had changed underneath it and the carrier
//! had no way to notice. A token from the wrong roster now fails to decode instead.
//!
//! **There is no bare-seed form, in or out.** A decimal number named *this build's
//! quick-play preset applied to that number*, not a run — which is how the #286 break
//! travelled. It is gone as an input as well as an output (#333 supersedes #328), so
//! there is exactly one thing a shared string can mean. The cost is real and worth
//! stating: "try seed 8371" no longer works, and every link shared before this stops
//! decoding. Those links were *already* booting the wrong run; failing loudly is the
//! better of the two.
//!
//! A token that does not decode is `None`, which the seed surface and the replay
//! carrier turn into a fresh run — never a bricked page (#110/#197).
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
/// `starting_abilities` count knob. With [`AbilityId::TECH`] holding six, a grant of
/// three is a **seeded draw of three of the six** (§8.3): the pool has
/// outgrown the grant, so the draw finally bites — a run holds a subset of the tech,
/// not all of it.
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

/// The token's fixed width in characters. Fixed rather than variable so a wrong
/// length is rejected before a single digit is parsed, and so a shared link is the
/// same shape whatever run it names.
pub const TOKEN_LEN: usize = 12;

/// The token's alphabet size — `a`–`z`, the "uncased alpha" the format is built on.
const ALPHABET: u128 = 26;

/// Every string the format can spell: `26^`[`TOKEN_LEN`]. The scramble is a
/// bijection over exactly this range, so every token maps back to *some* value —
/// validity is then decided by the check field and the residue, not by the encoding.
const TOKEN_SPACE: u128 = ALPHABET.pow(TOKEN_LEN as u32);

/// The format's major version, folded into [`MAGIC`]. Bump it for any change to the
/// field list or their order; the roster sizes and caps below are folded in
/// separately, so *those* need no manual bump.
const FORMAT_MAJOR: u64 = 2;

/// The seed field's width. Every run the game can create must fit here — see
/// [`LevelSeed::narrow_seed`], which is how an entropy source is brought into range.
/// Four billion facilities is far past what a player can exhaust, and the width is
/// what keeps the token to twelve characters rather than nineteen.
pub const SEED_BITS: u32 = 32;

/// The seed field's radix.
const SEED_SPACE: u64 = 1 << SEED_BITS;

/// The check field's width. Twelve bits reject ~99.98% of tokens from a stale roster
/// or a slipped keystroke. A character buys about five check bits, and twelve is the
/// point where one mistyped letter goes from 1-in-128 slipping through to 1-in-4096.
///
/// It is sized for **typos and stale formats**, not for tampering. Tamper resistance
/// is not available here at any width: the key would live in the wasm and can be read
/// out, so a longer field buys obfuscation, not security. Nor is it needed — forging
/// a token yields a run with abilities the player could have drawn anyway, in a game
/// with permadeath and no meta-progression (§2). If a daily challenge ever wants
/// verification, it belongs server-side against a submitted replay (§12.4), not here.
const CHECK_BITS: u32 = 12;

/// The check field's radix.
const CHECK_SPACE: u64 = 1 << CHECK_BITS;

/// How many [`IntelGate`] variants there are — an exact radix, not a bitfield padded
/// to two bits, so the unused fourth code that used to need rejecting cannot exist.
const GATE_VARIANTS: u64 = 3;

/// How many boolean modifier toggles the token carries — every [`LevelModifiers`]
/// field except the gate.
const MODIFIER_TOGGLES: usize = 4;

/// The most toggles that can be active at once. Today that is *all* of them: §12.6's
/// three modifier sources compose harder-ward and nothing bounds the result, so the
/// token must be able to say "everything on". It is named anyway because it is a
/// radix: if a cap ever lands, tightening this shrinks the token, and [`MAGIC`]
/// makes every token written under the old cap fail rather than mis-read.
const MODIFIER_CAP: usize = MODIFIER_TOGGLES;

/// A fixed multiplier applied to the packed value before it is written in base 26,
/// and undone by [`UNSCRAMBLE`] on the way back. Coprime to [`TOKEN_SPACE`]
/// (`2^12 · 13^12`), so it is a bijection over the whole range.
///
/// Without it the seed sits in the high digits and consecutive seeds share their last
/// five characters — the token would look enumerable, and read as broken.
const SCRAMBLE: u128 = 44_668_976_583_019_541;

/// The modular inverse of [`SCRAMBLE`] over [`TOKEN_SPACE`]. Pinned rather than
/// computed, and asserted in the tests.
const UNSCRAMBLE: u128 = 45_103_698_764_276_541;

/// The format fingerprint folded into every check field: the major version, the
/// roster sizes, and the **caps**. Any of them moving invalidates every token written
/// under the old shape — which is the point (see the module docs on #286).
///
/// It is computed off the catalogue rather than written down, so adding an ability or
/// a modifier changes it without anyone remembering to.
const MAGIC: u64 = {
    let parts = [
        FORMAT_MAJOR,
        AbilityId::ALL.len() as u64,
        AbilityId::TECH.len() as u64,
        AbilityId::INNATE.len() as u64,
        AbilityId::MAX_TECH_HELD as u64,
        MODIFIER_TOGGLES as u64,
        MODIFIER_CAP as u64,
        GATE_VARIANTS,
        SEED_BITS as u64,
        CHECK_BITS as u64,
    ];
    let mut hash = FNV_OFFSET;
    let mut i = 0;
    while i < parts.len() {
        hash = fnv_mix(hash, parts[i]);
        i += 1;
    }
    hash
};

/// FNV-1a's 64-bit offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a's 64-bit prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Fold one 64-bit value into an FNV-1a hash, byte by byte. Not cryptographic and not
/// trying to be ([`CHECK_BITS`] says why) — it needs to scatter a one-digit change
/// across the check field, and it does.
const fn fnv_mix(mut hash: u64, value: u64) -> u64 {
    let bytes = value.to_le_bytes();
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    hash
}

/// A run's whole reproducible starting config (§12.4/#245): the three pieces that
/// compose to a shareable [level-seed token](self) — the seed, the active
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

    /// Narrow an arbitrary entropy source to the [`SEED_BITS`] the token carries.
    ///
    /// The shell rolls a fresh run off the wall clock, which is far wider than the
    /// seed field; without this the token could not express the very runs the game
    /// creates. Applied at the *source*, never inside a constructor: a
    /// [`LevelSeed`] built from a given number keeps that number, so nothing
    /// silently boots a run other than the one asked for.
    pub const fn narrow_seed(raw: u64) -> u64 {
        raw & (SEED_SPACE - 1)
    }

    /// Encode to the [level-seed token](self) — twelve lowercase letters.
    ///
    /// `None` when the config is not one a run can hold, which is the honest answer
    /// rather than a token that would decode to something else: a seed wider than
    /// [`SEED_BITS`] (see [`narrow_seed`](Self::narrow_seed)), or a loadout holding
    /// more than [`AbilityId::MAX_TECH_HELD`] tech (§8.3 — [`Loadout::full`] is the
    /// obvious example, and is documented as not a loadout a run can hold). Every
    /// surface that shows or shares a token already has a "there is no token for
    /// this" branch, because a hand-built state has never had one.
    pub fn encode(&self) -> Option<String> {
        let (toggles, gate) = modifier_fields(self.modifiers);
        let mut chain = Chain::default();
        chain.push(self.seed, SEED_SPACE)?;
        chain.push(innate_bits(self.abilities), 1 << AbilityId::INNATE.len())?;
        chain.push(gate_code(gate), GATE_VARIANTS)?;
        chain.push_choice(&toggles, MODIFIER_CAP)?;
        chain.push_choice(&tech_held(self.abilities), AbilityId::MAX_TECH_HELD)?;
        chain.push(check_of(chain.0), CHECK_SPACE)?;
        Some(to_letters(scramble(u128::from(chain.0))))
    }

    /// Decode a [level-seed token](self), or `None` if it is not one.
    ///
    /// Rejects, in order: a wrong length or a non-alphabetic character; a check field
    /// that disagrees with [`MAGIC`] — a token from a build whose roster or caps
    /// differ, or one letter mistyped; a count past its cap; and a non-zero residue
    /// once every field has been read, which is what catches a value the format
    /// cannot have produced. `None` is a graceful fall to a fresh run, never a
    /// bricked page (#110/#197).
    ///
    /// Case-insensitive, because a token read aloud or through a form that
    /// capitalises should still boot its run; [`encode`](Self::encode) always emits
    /// lowercase.
    pub fn decode(raw: &str) -> Option<Self> {
        let mut chain = Chain(unscramble(from_letters(raw.trim())?).try_into().ok()?);

        // The check is popped first because it was pushed last, and it is verified
        // against what remains — the payload it was computed over.
        let check = chain.pop(CHECK_SPACE);
        if check != check_of(chain.0) {
            return None;
        }

        // Each held set pops its count before the combination index that count sizes
        // — the ordering the push order exists to produce.
        let tech: [bool; AbilityId::TECH.len()] = chain.pop_choice(AbilityId::MAX_TECH_HELD)?;
        let toggles: [bool; MODIFIER_TOGGLES] = chain.pop_choice(MODIFIER_CAP)?;
        let gate = gate_from_code(chain.pop(GATE_VARIANTS))?;
        let innate = chain.pop(1 << AbilityId::INNATE.len());
        let seed = chain.pop(SEED_SPACE);
        if chain.0 != 0 {
            return None; // a value the chain cannot have produced
        }

        let mut abilities = Loadout::empty();
        for (slot, id) in AbilityId::INNATE.into_iter().enumerate() {
            if innate >> slot & 1 == 1 {
                abilities = abilities.with(id);
            }
        }
        for (id, held) in AbilityId::TECH.into_iter().zip(tech) {
            if held {
                abilities = abilities.with(id);
            }
        }
        Some(Self {
            seed,
            modifiers: modifiers_from_fields(&toggles, gate),
            abilities,
        })
    }
}

/// Draw quick play's ability loadout from `seed` (#244): the innate set plus
/// [`QUICK_PLAY_TECH_GRANT`] tech chosen from [`AbilityId::TECH`]. Seeded off a
/// sub-stream independent of generation ([`LOADOUT_STREAM_SALT`]), so the draw is
/// deterministic yet never perturbs the facility. When the grant meets or exceeds
/// the pool every tech is granted and no randomness is drawn at all; with five tech
/// shipped and a grant of three, the partial draw below runs and picks a subset.
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
/// this, so a level-seed token reproduces the *same* run everywhere.
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

/// The token's packed value, built up one field at a time.
///
/// `push` appends a digit at the least-significant end (`value * radix + digit`), so
/// `pop` returns fields in the reverse of the order they were pushed. That reversal
/// is load-bearing: a field whose radix depends on another — a combination index
/// sized by its count — is pushed *before* it, so the count is already in hand by the
/// time the index needs reading.
#[derive(Default)]
struct Chain(u64);

impl Chain {
    /// Append `digit` in `radix`, or `None` if it does not belong in that radix or
    /// the chain would overflow — both meaning the config is not one the format can
    /// carry.
    fn push(&mut self, digit: u64, radix: u64) -> Option<()> {
        if digit >= radix {
            return None;
        }
        self.0 = self.0.checked_mul(radix)?.checked_add(digit)?;
        Some(())
    }

    /// Read off the least-significant digit in `radix`.
    fn pop(&mut self, radix: u64) -> u64 {
        let digit = self.0 % radix;
        self.0 /= radix;
        digit
    }

    /// Append a held set as a **count** and a **combination index** — the encoding
    /// whose width tracks `cap` rather than `N` (see the module docs).
    ///
    /// The count goes on last so it pops first, since it is the index's radix.
    /// `None` when more than `cap` entries are held: not a set a run can hold, and
    /// so not one the token will pretend to carry.
    fn push_choice<const N: usize>(&mut self, held: &[bool; N], cap: usize) -> Option<()> {
        let count = held.iter().filter(|&&held| held).count();
        if count > cap {
            return None;
        }
        self.push(combination_rank(held), binomial(N, count))?;
        self.push(count as u64, cap as u64 + 1)
    }

    /// Read back a held set pushed by [`push_choice`](Self::push_choice). `None` when
    /// the count exceeds `cap` — a token from a format with a wider cap, or a
    /// corrupted one.
    fn pop_choice<const N: usize>(&mut self, cap: usize) -> Option<[bool; N]> {
        let count = self.pop(cap as u64 + 1) as usize;
        if count > cap {
            return None;
        }
        combination_unrank(self.pop(binomial(N, count)), count)
    }
}

/// `C(n, k)` — how many ways `k` entries are held out of `n`, and so the radix of a
/// combination index. Saturates to zero past `n`, which is the honest count.
const fn binomial(n: usize, k: usize) -> u64 {
    if k > n {
        return 0;
    }
    // The multiplicative form, dividing as it goes so the running value stays small:
    // C(n, i) is always an integer, so the division is exact at every step.
    let mut value: u64 = 1;
    let mut i = 0;
    while i < k {
        value = value * (n - i) as u64 / (i as u64 + 1);
        i += 1;
    }
    value
}

/// The lexicographic rank of a held set among all sets of its size — the combination
/// index the token carries.
fn combination_rank<const N: usize>(held: &[bool; N]) -> u64 {
    let count = held.iter().filter(|&&held| held).count();
    let mut rank = 0;
    let mut remaining = count;
    for (position, &held) in held.iter().enumerate() {
        if held {
            remaining -= 1;
        } else if remaining > 0 {
            // Every set that takes this position instead sorts earlier: count them
            // and step past.
            rank += binomial(N - position - 1, remaining - 1);
        }
    }
    rank
}

/// The held set with lexicographic `rank` among the sets of size `count` — the
/// inverse of [`combination_rank`]. `None` when the rank is past the last such set.
fn combination_unrank<const N: usize>(mut rank: u64, count: usize) -> Option<[bool; N]> {
    let mut held = [false; N];
    let mut remaining = count;
    for (position, slot) in held.iter_mut().enumerate() {
        if remaining == 0 {
            break;
        }
        let skipped = binomial(N - position - 1, remaining - 1);
        if rank < skipped {
            *slot = true;
            remaining -= 1;
        } else {
            rank -= skipped;
        }
    }
    (remaining == 0 && rank == 0).then_some(held)
}

/// Split a [`LevelModifiers`] into the token's fields: the toggles in a fixed order,
/// and the gate. A struct destructure names every field, so a new modifier will not
/// compile until it is given a position here (§12.2 — the compiler enumerates the
/// encode sites), and adding one changes [`MAGIC`], so tokens written before it stop
/// decoding instead of quietly losing it.
fn modifier_fields(m: LevelModifiers) -> ([bool; MODIFIER_TOGGLES], IntelGate) {
    let LevelModifiers {
        guards_always_search_hideouts,
        sighting_lost_calls_a_guard,
        body_found_calls_two_guards,
        always_show_vision_cones,
        intel_to_exit,
    } = m;
    (
        [
            guards_always_search_hideouts,
            sighting_lost_calls_a_guard,
            body_found_calls_two_guards,
            always_show_vision_cones,
        ],
        intel_to_exit,
    )
}

/// Rebuild a [`LevelModifiers`] from the token's fields — the inverse of
/// [`modifier_fields`], in the same order.
fn modifiers_from_fields(toggles: &[bool; MODIFIER_TOGGLES], gate: IntelGate) -> LevelModifiers {
    let [guards_always_search_hideouts, sighting_lost_calls_a_guard, body_found_calls_two_guards, always_show_vision_cones] =
        *toggles;
    LevelModifiers {
        guards_always_search_hideouts,
        sighting_lost_calls_a_guard,
        body_found_calls_two_guards,
        always_show_vision_cones,
        intel_to_exit: gate,
    }
}

/// The intel gate's digit.
fn gate_code(gate: IntelGate) -> u64 {
    match gate {
        IntelGate::None => 0,
        IntelGate::AtLeastOne => 1,
        IntelGate::All => 2,
    }
}

/// The intel gate for a digit. Total over the radix — there is no unused code to
/// reject, which is the point of an exact radix rather than a padded bitfield.
fn gate_from_code(code: u64) -> Option<IntelGate> {
    match code {
        0 => Some(IntelGate::None),
        1 => Some(IntelGate::AtLeastOne),
        2 => Some(IntelGate::All),
        _ => None,
    }
}

/// The innate half of a loadout, as a bitset over [`AbilityId::INNATE`]. Innate
/// abilities are not drawn and not capped (§8.3), so they cost a bit each — there is
/// no cap for a combination index to track.
fn innate_bits(abilities: Loadout) -> u64 {
    AbilityId::INNATE
        .into_iter()
        .enumerate()
        .filter(|&(_, id)| abilities.contains(id))
        .map(|(slot, _)| 1 << slot)
        .sum()
}

/// The tech half of a loadout, as membership over [`AbilityId::TECH`]'s fixed order.
fn tech_held(abilities: Loadout) -> [bool; AbilityId::TECH.len()] {
    AbilityId::TECH.map(|id| abilities.contains(id))
}

/// The check field for a payload: [`CHECK_BITS`] off an FNV-1a fold of the payload
/// and [`MAGIC`]. Taken from the top of the hash, where the avalanche is best.
fn check_of(payload: u64) -> u64 {
    let hash = fnv_mix(fnv_mix(FNV_OFFSET, payload), MAGIC);
    hash >> (u64::BITS - CHECK_BITS) & (CHECK_SPACE - 1)
}

/// Spread the packed value over the token's digits, so consecutive seeds do not share
/// a visible prefix. A bijection over [`TOKEN_SPACE`] — see [`SCRAMBLE`].
fn scramble(value: u128) -> u128 {
    value * SCRAMBLE % TOKEN_SPACE
}

/// Undo [`scramble`].
fn unscramble(value: u128) -> u128 {
    value * UNSCRAMBLE % TOKEN_SPACE
}

/// Write a value as exactly [`TOKEN_LEN`] lowercase letters, most significant first
/// and zero-padded with `a`. Total over [`TOKEN_SPACE`], which is what the scramble
/// maps onto.
fn to_letters(mut value: u128) -> String {
    let mut letters = [b'a'; TOKEN_LEN];
    for slot in letters.iter_mut().rev() {
        *slot = b'a' + (value % ALPHABET) as u8;
        value /= ALPHABET;
    }
    String::from_utf8(letters.to_vec()).expect("the alphabet is ASCII")
}

/// Read [`TOKEN_LEN`] letters back to a value, or `None` on a wrong length or any
/// character outside `a`–`z`. Case-insensitive; the byte-length check is safe against
/// multi-byte input because a non-ASCII token is rejected on the same pass.
fn from_letters(token: &str) -> Option<u128> {
    if token.len() != TOKEN_LEN {
        return None;
    }
    let mut value: u128 = 0;
    for byte in token.bytes() {
        let letter = byte.to_ascii_lowercase();
        if !letter.is_ascii_lowercase() {
            return None;
        }
        value = value * ALPHABET + u128::from(letter - b'a');
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Outcome;

    /// Encoding a config a run can hold, for the tests that only care about the
    /// string.
    fn token(level: LevelSeed) -> String {
        level.encode().expect("a config a run can hold")
    }

    /// **One form, one width** (#333): every token is [`TOKEN_LEN`] lowercase
    /// letters, whatever the config it carries. There is no compact form and no
    /// display form to pick between — the preset a config happens to match changes
    /// nothing about how it is written down, which is precisely what stopped being
    /// true when a bare seed meant "this build's quick play, applied to a number".
    #[test]
    fn every_config_encodes_to_one_fixed_width_alphabetic_token() {
        for level in [
            LevelSeed::quick_play(8371),
            LevelSeed::sim(8371),
            LevelSeed::quick_play(0),
            LevelSeed::quick_play(u64::from(u32::MAX)),
        ] {
            let token = token(level);
            assert_eq!(token.len(), TOKEN_LEN, "fixed width: {token}");
            assert!(
                token.bytes().all(|b| b.is_ascii_lowercase()),
                "lowercase alphabetic only: {token}",
            );
            assert_eq!(
                LevelSeed::decode(&token),
                Some(level),
                "{token} round-trips"
            );
        }
    }

    /// A token is read case-insensitively — it survives being read aloud, or a form
    /// that capitalises — but is always **emitted** lowercase, so one config has one
    /// spelling.
    #[test]
    fn a_token_decodes_whatever_its_case() {
        let level = LevelSeed::sim(8371);
        let token = token(level);
        assert_eq!(LevelSeed::decode(&token.to_uppercase()), Some(level));
        assert_eq!(token, token.to_lowercase(), "emitted lowercase");
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
                LevelSeed::decode(&token(state.level().expect("a booted run"))),
                Some(level),
            );
        }
    }

    /// Quick play (#244): the intel gate at [`IntelGate::All`], the innate set, and a
    /// seeded draw of [`QUICK_PLAY_TECH_GRANT`] tech. With five tech shipped and a
    /// grant of three the draw bites (§8.3): the run holds every innate ability plus
    /// exactly three of the five tech — a strict subset of the full loadout.
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
        assert_eq!(tech_held, QUICK_PLAY_TECH_GRANT, "a three-of-six tech draw");
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

    /// **A bare decimal seed is not a token** (#333, superseding #328). It named
    /// this build's quick-play preset applied to a number, not a run, so a link
    /// carrying one silently re-resolved into a different run whenever the preset
    /// moved underneath it. It is gone as an *input* too — one thing a shared string
    /// can mean — and this test is where that decision is recorded, in place of the
    /// `quick_play(8371).encode() == "8371"` pin it replaces.
    #[test]
    fn a_bare_seed_is_no_longer_a_token() {
        assert_eq!(LevelSeed::decode("8371"), None);
        assert_eq!(LevelSeed::decode("0"), None);
        assert_eq!(LevelSeed::decode("  42 "), None);
        // Nor is the old structured form it shared the surface with.
        assert_eq!(LevelSeed::decode("L1-8371-4-cdz"), None);
        assert_eq!(LevelSeed::decode("L1-42-4-r"), None);
    }

    /// Every combination of the modifier fields and a spread of holdable loadouts
    /// survives the round-trip — the codec is total over the space of configs a run
    /// can hold, so no shared level can silently mutate in transit (#245).
    #[test]
    fn every_config_round_trips() {
        let gates = [IntelGate::None, IntelGate::AtLeastOne, IntelGate::All];
        let loadouts = [
            Loadout::empty(),
            Loadout::innate(),
            Loadout::innate().with(AbilityId::Camouflage),
            Loadout::empty()
                .with(AbilityId::Decoy)
                .with(AbilityId::Dephase),
            // The cap itself: the widest loadout a run can hold (§8.3).
            Loadout::innate()
                .with(AbilityId::Decoy)
                .with(AbilityId::Confusion)
                .with(AbilityId::Vision),
        ];
        for search in [false, true] {
            for cones in [false, true] {
                for sighting in [false, true] {
                    for body in [false, true] {
                        for gate in gates {
                            for abilities in loadouts {
                                let level = LevelSeed {
                                    seed: 12345,
                                    modifiers: LevelModifiers {
                                        guards_always_search_hideouts: search,
                                        sighting_lost_calls_a_guard: sighting,
                                        body_found_calls_two_guards: body,
                                        always_show_vision_cones: cones,
                                        intel_to_exit: gate,
                                    },
                                    abilities,
                                };
                                assert_eq!(
                                    LevelSeed::decode(&token(level)),
                                    Some(level),
                                    "round-trip failed for {level:?}",
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Every tech subset up to the cap round-trips through its combination index —
    /// the encoding that costs `log2(C(n, k))` rather than a bit per catalogue entry.
    #[test]
    fn every_holdable_tech_subset_round_trips() {
        let pool = AbilityId::TECH;
        let mut seen = 0;
        // Every subset of the pool, filtered to those a run can hold.
        for mask in 0..(1u32 << pool.len()) {
            if mask.count_ones() as usize > AbilityId::MAX_TECH_HELD {
                continue;
            }
            let mut abilities = Loadout::innate();
            for (slot, id) in pool.into_iter().enumerate() {
                if mask >> slot & 1 == 1 {
                    abilities = abilities.with(id);
                }
            }
            let level = LevelSeed {
                abilities,
                ..LevelSeed::sim(7)
            };
            assert_eq!(
                LevelSeed::decode(&token(level)),
                Some(level),
                "mask {mask:b}"
            );
            seen += 1;
        }
        // Σ C(6, k) for k ≤ 3 — the whole holdable space, not a sample of it.
        assert_eq!(seen, 1 + 6 + 15 + 20);
    }

    /// A config the format cannot carry encodes to `None` rather than to a token that
    /// would decode as something else. Two ways that happens, both meaning "not a run
    /// this game can hold": a seed wider than [`SEED_BITS`], and a loadout over the
    /// §8.3 tech cap ([`Loadout::full`] documents itself as exactly that).
    #[test]
    fn a_config_a_run_cannot_hold_has_no_token() {
        let wide = LevelSeed::quick_play(1 << SEED_BITS);
        assert_eq!(wide.encode(), None, "a seed past the field's width");
        // …and narrowing it is how an entropy source is brought into range.
        let narrowed = LevelSeed::quick_play(LevelSeed::narrow_seed(1 << SEED_BITS));
        assert_eq!(narrowed.seed, 0);
        assert!(narrowed.encode().is_some());

        let over_cap = LevelSeed {
            abilities: Loadout::full(),
            ..LevelSeed::sim(1)
        };
        assert_eq!(over_cap.encode(), None, "six tech is over the cap of three");
    }

    /// **The frozen token** (#333): a literal that must keep decoding to this exact
    /// config, forever, or the format has changed under every link ever shared.
    ///
    /// If this fails, something moved that the token's shape depends on — the roster,
    /// a cap, a field order, [`FORMAT_MAJOR`]. That is not necessarily wrong, but it
    /// is never *incidental*: decide it deliberately, bump the version, and expect
    /// every token in the wild to stop decoding. The [`MAGIC`] fold is what turns
    /// that into a loud failure rather than the silent re-resolution of #286.
    #[test]
    fn a_frozen_token_still_names_the_run_it_always_did() {
        const FROZEN: &str = "bcwdrhliqsmm";
        let expected = LevelSeed {
            seed: 8371,
            modifiers: LevelModifiers {
                guards_always_search_hideouts: true,
                sighting_lost_calls_a_guard: false,
                body_found_calls_two_guards: true,
                always_show_vision_cones: false,
                intel_to_exit: IntelGate::All,
            },
            abilities: Loadout::innate()
                .with(AbilityId::Camouflage)
                .with(AbilityId::Autodoors)
                .with(AbilityId::Vision),
        };
        assert_eq!(
            LevelSeed::decode(FROZEN),
            Some(expected),
            "the frozen token no longer names its run",
        );
        assert_eq!(token(expected), FROZEN, "…and it is still spelled this way");
    }

    /// **The #286 break, caught.** A token written against a different roster or a
    /// different cap fails the check rather than decoding into a plausible-looking
    /// different run. Simulated by perturbing the magic the check is folded over,
    /// which is exactly what adding an ability or moving a cap does.
    ///
    /// Not every wrong-magic token is caught — [`CHECK_BITS`] bounds that at about
    /// 99.98% — so this asserts the rate over the whole seed space rather than a
    /// single lucky rejection.
    #[test]
    fn a_token_from_another_roster_is_rejected() {
        let survivors = (0..20_000u64)
            .filter(|&seed| {
                let level = LevelSeed::sim(seed);
                // Re-spell the token as a build with one more ability would: same
                // payload, a check folded over a different magic.
                let packed = unscramble(from_letters(&token(level)).expect("its own token"));
                let payload = packed as u64 / CHECK_SPACE;
                let foreign = fnv_mix(fnv_mix(FNV_OFFSET, payload), MAGIC.wrapping_add(1))
                    >> (u64::BITS - CHECK_BITS)
                    & (CHECK_SPACE - 1);
                let reworded = to_letters(scramble(u128::from(payload * CHECK_SPACE + foreign)));
                LevelSeed::decode(&reworded).is_some()
            })
            .count();
        // 1-in-4096 slip through by collision; anything near 20,000 means the magic
        // is not reaching the check at all.
        assert!(
            survivors < 40,
            "{survivors} of 20000 foreign tokens decoded — the magic is not biting",
        );
    }

    /// A malformed token decodes to `None` — the graceful fall to a fresh run the
    /// seed surface and the replay carrier depend on (#110/#197): a bad token must
    /// never brick the page.
    #[test]
    fn a_malformed_token_decodes_to_none() {
        let valid = token(LevelSeed::sim(42));
        assert_eq!(LevelSeed::decode(""), None, "empty");
        assert_eq!(
            LevelSeed::decode(&valid[..TOKEN_LEN - 1]),
            None,
            "too short"
        );
        assert_eq!(LevelSeed::decode(&format!("{valid}a")), None, "too long");
        assert_eq!(LevelSeed::decode("abcdefghijk1"), None, "a digit");
        assert_eq!(LevelSeed::decode("abcdef ghijk"), None, "a space");
        assert_eq!(LevelSeed::decode("abcdefghijk-"), None, "punctuation");
        assert_eq!(LevelSeed::decode("ébcdefghijkl"), None, "not even ASCII");

        // A single mistyped letter is caught ~99.98% of the time; over every
        // one-letter slip in one token, none should survive.
        for at in 0..TOKEN_LEN {
            for letter in b'a'..=b'z' {
                let mut typo = valid.clone().into_bytes();
                if typo[at] == letter {
                    continue;
                }
                typo[at] = letter;
                let typo = String::from_utf8(typo).expect("ASCII");
                assert_eq!(LevelSeed::decode(&typo), None, "a typo decoded: {typo}");
            }
        }
    }

    /// The token is URL-safe by construction — letters only, so it drops into a
    /// `?seed=` field or a `#seed=` hash with no escaping — and **consecutive seeds
    /// do not look consecutive**. Without the scramble the seed sits in the high
    /// digits and neighbouring runs would share every trailing character, which reads
    /// as a broken token even though it decodes fine.
    #[test]
    fn neighbouring_seeds_do_not_share_a_visible_pattern() {
        let a = token(LevelSeed::quick_play(8371));
        let b = token(LevelSeed::quick_play(8372));
        assert!(a.chars().all(|c| c.is_ascii_lowercase()));
        let shared = a.bytes().zip(b.bytes()).filter(|(x, y)| x == y).count();
        assert!(
            shared <= 3,
            "neighbouring seeds share {shared} of {TOKEN_LEN} positions: {a} / {b}",
        );
    }

    /// The scramble is a bijection over the whole token space — [`UNSCRAMBLE`] is
    /// pinned rather than computed, so it is asserted rather than assumed. A wrong
    /// inverse would corrupt every token in a way no round-trip test would show,
    /// since encode and decode would agree with each other while agreeing with
    /// nothing already shared.
    #[test]
    fn the_scramble_inverts_exactly() {
        assert_eq!(SCRAMBLE * UNSCRAMBLE % TOKEN_SPACE, 1);
        for value in [0, 1, 2, 4095, TOKEN_SPACE - 1, TOKEN_SPACE / 3] {
            assert_eq!(unscramble(scramble(value)), value);
        }
    }

    /// The whole packed space fits the twelve characters it is written in, with room
    /// left. If a field ever widens past this, the token gets longer — which is a
    /// deliberate format change, not something to discover from a panic in
    /// [`to_letters`].
    #[test]
    fn the_packed_space_fits_the_token() {
        let widest = u128::from(SEED_SPACE)
            * (1 << AbilityId::INNATE.len())
            * u128::from(GATE_VARIANTS)
            * u128::from(combinations_up_to(MODIFIER_TOGGLES, MODIFIER_CAP))
            * u128::from(combinations_up_to(
                AbilityId::TECH.len(),
                AbilityId::MAX_TECH_HELD,
            ))
            * u128::from(CHECK_SPACE);
        assert!(
            widest <= TOKEN_SPACE,
            "{widest} does not fit {TOKEN_SPACE} — the token needs another character",
        );
    }

    /// `Σ C(n, k)` for `k ≤ cap` — the size of a combination field, for the capacity
    /// assertion above.
    fn combinations_up_to(n: usize, cap: usize) -> u64 {
        (0..=cap).map(|k| binomial(n, k)).sum()
    }

    /// The combination index is a bijection onto the sets of each size: every rank
    /// unranks to a set that ranks back, and nothing outside the range decodes.
    #[test]
    fn the_combination_index_is_a_bijection() {
        for count in 0..=AbilityId::MAX_TECH_HELD {
            let total = binomial(AbilityId::TECH.len(), count);
            for rank in 0..total {
                let held: [bool; AbilityId::TECH.len()] =
                    combination_unrank(rank, count).expect("in range");
                assert_eq!(held.iter().filter(|&&h| h).count(), count);
                assert_eq!(combination_rank(&held), rank);
            }
            // One past the last set of this size is not a set of this size.
            assert_eq!(
                combination_unrank::<{ AbilityId::TECH.len() }>(total, count),
                None,
            );
        }
    }

    /// Determinism (§12.4): a level-seed token reproduces the **exact** run — the
    /// same facility, modifiers, and loadout — every boot. A golden pin: decode a
    /// token, boot twice, and assert the rendered frames are byte-identical, and
    /// that booting from the decoded config matches booting from the config directly.
    #[test]
    fn a_token_reproduces_the_exact_run() {
        let level = LevelSeed::quick_play(2026);
        let decoded = LevelSeed::decode(&token(level)).expect("its own token decodes");
        assert_eq!(decoded, level);

        let a = start_level(&decoded).expect("the v1 config boots");
        let b = start_level(&level).expect("the v1 config boots");
        assert_eq!(
            crate::render(&a),
            crate::render(&b),
            "the token boots the same frame as the config",
        );
        // The resolved config is threaded into the running state — the loadout the
        // token carries (a three-of-five tech draw now, no longer the full set).
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
