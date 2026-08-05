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
//! Eighteen characters, `a`–`z`, nothing else: `prbjdokbxcqgjnrnco`. Fixed width, so
//! a wrong length is rejected before anything is parsed; all-alphabetic, so there is
//! no `0`/`O` or `1`/`l` to misread; unreserved throughout, so it drops straight into
//! the #110 seed surface and the #197 replay carrier with no escaping.
//!
//! **The format is specified in [`docs/level-seed-token.md`]** — the field layout,
//! the slot discipline, the integrity argument and the sizing trade-offs live there,
//! referenced from §12.6. What follows is only what a reader of *this module* needs.
//!
//! The token is a **mixed-radix chain**: each field is pushed as a digit
//! (`value = value * radix + digit`), the whole value is scrambled ([`SCRAMBLE`]),
//! and the result is written in base 26. Four fields, every radix a constant:
//!
//! | Field | Radix | Bits |
//! |---|---|---|
//! | seed | `2^`[`SEED_BITS`] | 17.00 |
//! | intel gate | [`GATE_VARIANTS`] | 1.58 |
//! | modifiers active | [`MODIFIER_SPACE`] — any ≤[`MODIFIER_CAP`] of [`SLOT_CAPACITY`] slots | 33.07 |
//! | tech held | [`TECH_SPACE`] — any ≤[`AbilityId::MAX_TECH_HELD`] of [`SLOT_CAPACITY`] slots | 21.42 |
//!
//! The innate set is **not carried**: §8.3 makes it always held, so it is restored on
//! decode rather than spelled out.
//!
//! Three properties carry the design, and each has a test named for it:
//!
//! - **Slots are permanent, and there are more of them than entries.** A held set is
//!   a combination index over [`SLOT_CAPACITY`] reserved positions, not over today's
//!   roster, so adding the seventh tech fills slot 6 and *every token ever shared
//!   keeps working*. That is the fix for the #286 break, in which the loadout was
//!   re-derived over a pool that had changed size and every link silently began
//!   booting something else.
//! - **The held-set encoding is dense** ([`SlotSet::ordinal`]). Its cost tracks the
//!   *cap* rather than the roster: `log2(C(n, k))` instead of a bit per entry, which
//!   is what makes 256 slots affordable at all — a bitset would need 51 characters.
//! - **There is no checksum field.** Integrity comes from the unused range
//!   ([`rejection_rate`]) plus the scramble, which is audited to catch every
//!   single-character slip and every transposition *with certainty*.
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
//! [`docs/level-seed-token.md`]: https://github.com/tk-auto/intrusion/blob/main/docs/level-seed-token.md
//!
//! # One boot path
//!
//! [`start_level`] turns a [`LevelSeed`] into a running [`State`] — the single boot
//! the web shell, the replay viewer, and the headless sim all call, so a seed here
//! is the very facility any of them plays (§13.2). A run's identity lives in one
//! place, and there is no second "almost the same" boot to drift from it.

use crate::ability::{AbilityId, Loadout};
use crate::cell::Direction;
use crate::difficulty::Difficulty;
use crate::generate::{generate_level, GenError};
use crate::modifiers::{GuardCount, IntelCount, IntelGate, LevelModifiers};
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

/// The token's fixed width in characters. Fixed rather than variable so a wrong
/// length is rejected before a single digit is parsed, and so a shared link is the
/// same shape whatever run it names.
///
/// **One length per format version**, and that is an invariant rather than a
/// coincidence (see [`FORMAT_MAJOR`]). The length is what tells versions apart, so a
/// future version that reserved more slots must also grow — the tempting move of
/// narrowing the seed to keep the length would make old tokens decodable under the
/// new rules, which is precisely the silent re-resolution this format exists to stop.
pub const TOKEN_LEN: usize = 18;

/// The token's alphabet size — `a`–`z`, the "uncased alpha" the format is built on.
const ALPHABET: u128 = 26;

/// Every string the format can spell: `26^`[`TOKEN_LEN`]. The scramble is a
/// bijection over exactly this range, so every token maps back to *some* value —
/// validity is then decided by the range and structure checks, not by the encoding.
const TOKEN_SPACE: u128 = ALPHABET.pow(TOKEN_LEN as u32);

/// The format's major version. It is **not carried in the token**: it is folded into
/// [`SCRAMBLE`], so a token written under another version unscrambles to noise and
/// fails the range check. That costs no bits at all, and it is why there is no
/// checksum field here — see [`rejection_rate`].
///
/// It also *implies* [`SLOT_CAPACITY`], which is the one number a decoder would need
/// to read a foreign token. A future version wanting to keep old links working keeps
/// this version's constants in a table and tries each in turn, newest first;
/// [`TOKEN_LEN`] tells them apart before any arithmetic runs.
const FORMAT_MAJOR: u64 = 1;

/// How many **slots** the format reserves for abilities and for modifiers — the
/// number the token is sized against, not the number that exist today (six tech and
/// four modifier toggles, as of writing).
///
/// This is the whole reason the format can outlive its own roster. Every ability and
/// every modifier owns a **permanent slot number**, and a held set is a combination
/// index over these 256 positions. Adding the seventh tech fills slot 6: no radix
/// moves, no token changes meaning, and **every link ever shared keeps working**.
/// That is what the previous format could not do — when the Vision passive joined the
/// pool (#286) every shared link silently began booting a different loadout.
///
/// The discipline it buys is the discipline it costs: **slot numbers are permanent**.
/// A retired ability leaves a tombstone slot rather than freeing it, and nothing may
/// ever be renumbered. Reserving 256 up front is what makes that affordable — a
/// target of ~100 live entries leaves room for the churn.
const SLOT_CAPACITY: usize = 256;

/// The most modifier toggles active at once. Unlike the ability cap this is a
/// *format* promise rather than a rule §12.6 enforces today, so it is the one number
/// here that the game must be held to: [`modifier_slots`] refuses to encode a config
/// that exceeds it. Five leaves one for each of §12.6's three composing sources with
/// room over for a mode that bundles.
const MODIFIER_CAP: usize = 5;

/// The seed field's width. Every run the game can create must fit here — see
/// [`LevelSeed::narrow_seed`], which is how an entropy source is brought into range.
///
/// **[START]** at 17 bits — 131,072 facilities. It is a balance, not a law: every bit
/// spent here is a bit taken from [`rejection_rate`], and the two trade one-for-one.
/// At 17 the first repeated facility is expected past ~450 runs, and rejection sits
/// near 1 in 3,000; a bit either way halves one to double the other.
pub const SEED_BITS: u32 = 17;

/// The seed field's radix.
const SEED_SPACE: u64 = 1 << SEED_BITS;

/// How many [`IntelGate`] variants there are — an exact radix, not a bitfield padded
/// to two bits, so there is no unused code to reject.
const GATE_VARIANTS: u64 = 3;

/// The largest held set the token carries, over both kinds — the width of a
/// [`SlotSet`]'s backing array.
const MAX_HELD_SLOTS: usize = if MODIFIER_CAP > AbilityId::MAX_TECH_HELD {
    MODIFIER_CAP
} else {
    AbilityId::MAX_TECH_HELD
};

/// How many distinct held sets of at most [`AbilityId::MAX_TECH_HELD`] slots exist —
/// the radix of the tech field.
const TECH_SPACE: u64 = sets_up_to(AbilityId::MAX_TECH_HELD);

/// How many distinct held sets of at most [`MODIFIER_CAP`] slots exist — the radix of
/// the modifier field, and the token's largest single field by some way.
const MODIFIER_SPACE: u64 = sets_up_to(MODIFIER_CAP);

/// Everything the token can say: the product of every field's radix. A decoded value
/// at or above this is one the format cannot have produced, and that range check is
/// the whole integrity mechanism — see [`rejection_rate`].
const PAYLOAD_SPACE: u128 =
    SEED_SPACE as u128 * GATE_VARIANTS as u128 * MODIFIER_SPACE as u128 * TECH_SPACE as u128;

// The format's own invariants, checked at build time rather than in a test — a token
// that did not fit its characters, or a roster that had outgrown its reserved slots,
// is not a failure worth discovering from a red suite.
const _: () = {
    assert!(
        PAYLOAD_SPACE < TOKEN_SPACE,
        "the payload does not fit TOKEN_LEN characters — the token needs another one",
    );
    assert!(
        rejection_rate() >= 1000,
        "too little of the token space is left over to reject a bad token with",
    );
    assert!(
        AbilityId::TECH.len() < SLOT_CAPACITY && MODIFIER_FIELDS < SLOT_CAPACITY,
        "the roster has outgrown its reserved slots — that is a new format version, \
         never a quiet bump of SLOT_CAPACITY, which would rewrite every token shared",
    );
    assert!(
        AbilityId::MAX_TECH_HELD <= MAX_HELD_SLOTS && MODIFIER_CAP <= MAX_HELD_SLOTS,
        "a cap exceeds the SlotSet it is carried in",
    );
};

/// One in how many arbitrary tokens decodes to *something* — the format's integrity,
/// and a plain consequence of how much of [`TOKEN_SPACE`] the payload leaves unused.
///
/// **There is no checksum field, deliberately.** A check field and unused range are
/// interchangeable: the scramble is a bijection, so an arbitrary token lands on a
/// uniform value, and exactly [`PAYLOAD_SPACE`] of [`TOKEN_SPACE`] values are valid
/// whatever fraction of the space a check field occupies. Spending bits on one would
/// buy nothing that spending them on length does not.
///
/// This figure covers **arbitrary** corruption — a random string, or a token from
/// another format version. The errors a human actually makes are covered far better:
/// a wrong length is rejected outright, and every single-character slip and every
/// transposition is rejected *with certainty*, by construction of [`SCRAMBLE`].
const fn rejection_rate() -> u128 {
    TOKEN_SPACE / PAYLOAD_SPACE
}

/// A nudge folded into [`SCRAMBLE`]'s derivation, to steer it onto a constant that
/// passes the corruption audit.
///
/// Five is not arbitrary and is not cosmetic: the first five derivations each leave
/// some single-character slip or transposition undetectable. See [`SCRAMBLE`] — and
/// if a change here ever fails `the_scramble_catches_every_realistic_slip`, bump this
/// number until it passes rather than weakening the test.
const SCRAMBLE_NONCE: u64 = 5;

/// A fixed multiplier applied to the packed value before it is written in base 26,
/// and undone by [`UNSCRAMBLE`] on the way back. Coprime to [`TOKEN_SPACE`]
/// (`2^18 · 13^18`), so it is a bijection over the whole range.
///
/// It does **three** jobs, and only the first is obvious.
///
/// 1. It stops consecutive seeds sharing a visible prefix. Without it the seed sits
///    in the high digits and neighbouring runs differ only in their last characters,
///    which reads as broken even though it decodes.
/// 2. It carries [`MAGIC`], and so the format version. A token from another version
///    unscrambles under a different constant, lands somewhere pseudo-random, and
///    fails the range check — no version field, no checksum, no bits.
/// 3. **It is what detects typos**, which is the part worth stating loudly. A
///    corrupted token decodes to `value + δ`, where δ is fixed by which characters
///    changed; the corruption is caught exactly when δ carries the value out of
///    [`PAYLOAD_SPACE`]. For a badly chosen multiplier hundreds of single-character
///    slips leave δ small enough to stay in range and slip through silently. This one
///    is audited: all 8,516 distinct single-character and transposition deltas are
///    carried out of range, so those corruptions are caught **with certainty** rather
///    than at [`rejection_rate`].
const SCRAMBLE: u128 = scramble_from(MAGIC, SCRAMBLE_NONCE);

/// The modular inverse of [`SCRAMBLE`] over [`TOKEN_SPACE`]. Pinned rather than
/// computed, and asserted in the tests — a wrong inverse would corrupt every token in
/// a way no round-trip test could show, since encode and decode would agree with each
/// other while agreeing with nothing already shared.
const UNSCRAMBLE: u128 = 12_520_201_768_539_098_941_729_755;

/// The format fingerprint: the major version, the slot capacity, the caps, and the
/// field widths. Everything whose movement would change what a token *means* — and
/// deliberately **not** the live roster sizes, which are free to grow into the
/// reserved slots without invalidating anything.
const MAGIC: u64 = {
    let parts = [
        FORMAT_MAJOR,
        SLOT_CAPACITY as u64,
        AbilityId::MAX_TECH_HELD as u64,
        MODIFIER_CAP as u64,
        GATE_VARIANTS,
        SEED_BITS as u64,
        TOKEN_LEN as u64,
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
/// trying to be — nothing here resists an attacker, and nothing needs to (forging a
/// token yields a run whose abilities were available anyway, in a game with permadeath
/// and no meta-progression, §2). It needs to scatter, and it does.
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

/// Derive the scramble multiplier from the format fingerprint, forced coprime to
/// [`TOKEN_SPACE`] (odd, and not a multiple of 13) so it stays a bijection.
const fn scramble_from(magic: u64, nonce: u64) -> u128 {
    let high = fnv_mix(fnv_mix(FNV_OFFSET, magic), nonce);
    let low = fnv_mix(fnv_mix(FNV_OFFSET, high), magic);
    let mut k = (((high as u128) << 64) | low as u128) % TOKEN_SPACE;
    if k.is_multiple_of(2) {
        k += 1;
    }
    while k.is_multiple_of(13) {
        k += 2;
    }
    k
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
        Self::quick_play_at(seed, Difficulty::Standard)
    }

    /// Quick play for `seed` at a **difficulty** (§12.6/#297): the preset above, with
    /// the modifiers [`Difficulty::draw`] picks for the level composed on top.
    ///
    /// [`Difficulty::Standard`] draws nothing, so this *is*
    /// [`quick_play`](Self::quick_play) — the axis costs the baseline nothing. The
    /// draw resolves here, before the run boots, and what the [`LevelSeed`] carries
    /// on is the **resolved set**: the difficulty number needs no field in the token
    /// and a shared token still reproduces the run exactly (§12.4).
    ///
    /// Composition is [`LevelModifiers::union`], the one rule §12.6 gives for putting
    /// two contributions together — so an easier draw *adds* what it reveals rather
    /// than relaxing the base's [`IntelGate::All`], which union composes harder-ward
    /// and the pool therefore leaves alone.
    pub fn quick_play_at(seed: u64, difficulty: Difficulty) -> Self {
        Self {
            seed,
            modifiers: LevelModifiers {
                intel_to_exit: IntelGate::All,
                ..LevelModifiers::default()
            }
            .union(difficulty.draw(seed)),
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

    /// Encode to the [level-seed token](self) — [`TOKEN_LEN`] lowercase letters.
    ///
    /// `None` when the config is not one a run can hold, which is the honest answer
    /// rather than a token that would decode to something else. Four ways that
    /// happens, all of them meaning "this is not a run this game can produce": a seed
    /// wider than [`SEED_BITS`] (see [`narrow_seed`](Self::narrow_seed)); a loadout
    /// over the §8.3 tech cap ([`Loadout::full`] documents itself as exactly that);
    /// a loadout missing an innate ability, which the token does not carry because
    /// §8.3 says a run always holds the innate set; and more than [`MODIFIER_CAP`]
    /// modifiers at once.
    ///
    /// Every surface that shows or shares a token already has a "there is no token
    /// for this" branch, because a hand-built state has never had one.
    pub fn encode(&self) -> Option<String> {
        let (modifiers, gate) = modifier_slots(self.modifiers)?;
        let mut chain = Chain::default();
        chain.push(u128::from(self.seed), u128::from(SEED_SPACE))?;
        chain.push(u128::from(gate_code(gate)), u128::from(GATE_VARIANTS))?;
        chain.push(
            u128::from(modifiers.ordinal(MODIFIER_CAP)?),
            u128::from(MODIFIER_SPACE),
        )?;
        chain.push(
            u128::from(tech_slots(self.abilities)?.ordinal(AbilityId::MAX_TECH_HELD)?),
            u128::from(TECH_SPACE),
        )?;
        Some(to_letters(scramble(chain.0)))
    }

    /// Decode a [level-seed token](self), or `None` if it is not one.
    ///
    /// Rejects, in order: a wrong length or a non-alphabetic character; a value
    /// outside [`PAYLOAD_SPACE`] once unscrambled — which is what catches a token
    /// from another format version, a mistyped one, and a random string alike (see
    /// [`rejection_rate`]); and a slot number this build has no ability or modifier
    /// for, which is a token from a build with a *bigger roster* and is rejected
    /// exactly rather than probabilistically.
    ///
    /// `None` is a graceful fall to a fresh run, never a bricked page (#110/#197).
    /// Case-insensitive, because a token read aloud or through a form that
    /// capitalises should still boot its run; [`encode`](Self::encode) always emits
    /// lowercase.
    pub fn decode(raw: &str) -> Option<Self> {
        let mut chain = Chain(unscramble(from_letters(raw.trim())?));
        let tech = SlotSet::from_ordinal(
            u64::try_from(chain.pop(u128::from(TECH_SPACE))).ok()?,
            AbilityId::MAX_TECH_HELD,
        )?;
        let modifiers = SlotSet::from_ordinal(
            u64::try_from(chain.pop(u128::from(MODIFIER_SPACE))).ok()?,
            MODIFIER_CAP,
        )?;
        let gate = gate_from_code(chain.pop(u128::from(GATE_VARIANTS)) as u64)?;
        let seed = chain.pop(u128::from(SEED_SPACE)) as u64;
        if chain.0 != 0 {
            return None; // a value the chain cannot have produced
        }

        // The innate set is not carried: §8.3 makes it always held, so it is restored
        // rather than read, and a token can never describe a run without it.
        let mut abilities = Loadout::innate();
        for slot in tech.iter() {
            abilities = abilities.with(*AbilityId::TECH.get(slot)?);
        }
        Some(Self {
            seed,
            modifiers: modifiers_from_slots(&modifiers, gate)?,
            abilities,
        })
    }
}

/// The token's packed value, built up one field at a time.
///
/// `push` appends a digit at the least-significant end (`value * radix + digit`), so
/// `pop` returns fields in the reverse of the order they were pushed. Every radix is
/// a constant — the dense [`SlotSet::ordinal`] is what makes that true — so the
/// packed space is exactly [`PAYLOAD_SPACE`] and the residue check after the last
/// field is an exact range test.
#[derive(Default)]
struct Chain(u128);

impl Chain {
    /// Append `digit` in `radix`, or `None` if it does not belong in that radix.
    fn push(&mut self, digit: u128, radix: u128) -> Option<()> {
        if digit >= radix {
            return None;
        }
        self.0 = self.0.checked_mul(radix)?.checked_add(digit)?;
        Some(())
    }

    /// Read off the least-significant digit in `radix`.
    fn pop(&mut self, radix: u128) -> u128 {
        let digit = self.0 % radix;
        self.0 /= radix;
        digit
    }
}

/// A held set, as up to [`MAX_HELD_SLOTS`] **slot numbers** in ascending order — the
/// tech a run holds, or the modifiers active on it.
///
/// Slots are permanent positions in a [`SLOT_CAPACITY`]-wide reserved space, not
/// indices into today's roster, which is what lets the roster grow without changing
/// what any token means (see [`SLOT_CAPACITY`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct SlotSet {
    held: [usize; MAX_HELD_SLOTS],
    len: usize,
}

impl SlotSet {
    /// Add a slot. Slots must be added in ascending order — every caller walks a
    /// fixed catalogue order, so they are. `None` past [`MAX_HELD_SLOTS`], which is a
    /// held set larger than any cap allows.
    fn push(&mut self, slot: usize) -> Option<()> {
        *self.held.get_mut(self.len)? = slot;
        self.len += 1;
        Some(())
    }

    /// The slots held, ascending.
    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.held[..self.len].iter().copied()
    }

    /// The **dense ordinal** of this set among every set of at most `cap` slots:
    /// sets ordered by size first, then lexicographically.
    ///
    /// Dense is the whole point. The obvious alternative — a count digit beside an
    /// index digit whose radix is `C(n, count)` — spends the same information but
    /// leaves the packed space *sparse*, bounded by the largest radix on any path
    /// rather than by the sum over paths. That overflows the token for the same field
    /// widths, silently, for configs on an expensive path. Here the count is implied
    /// by which size-block the ordinal falls in, recovered exactly on the way back,
    /// and the field's radix is the constant [`sets_up_to`].
    fn ordinal(&self, cap: usize) -> Option<u64> {
        if self.len > cap {
            return None;
        }
        // Skip past every smaller set, then rank lexicographically within this size.
        let mut ordinal = 0;
        let mut size = 0;
        while size < self.len {
            ordinal += binomial(SLOT_CAPACITY, size);
            size += 1;
        }
        let mut next = 0;
        for (position, slot) in self.iter().enumerate() {
            if slot >= SLOT_CAPACITY || slot < next {
                return None; // out of the reserved space, or not ascending
            }
            let remaining = self.len - position - 1;
            for skipped in next..slot {
                ordinal += binomial(SLOT_CAPACITY - skipped - 1, remaining);
            }
            next = slot + 1;
        }
        Some(ordinal)
    }

    /// The set with dense `ordinal` among the sets of at most `cap` slots — the
    /// inverse of [`ordinal`](Self::ordinal). `None` past the last such set.
    fn from_ordinal(mut ordinal: u64, cap: usize) -> Option<Self> {
        // Which size-block the ordinal lands in *is* the count.
        let mut len = 0;
        loop {
            if len > cap {
                return None;
            }
            let block = binomial(SLOT_CAPACITY, len);
            if ordinal < block {
                break;
            }
            ordinal -= block;
            len += 1;
        }
        let mut set = Self::default();
        let mut slot = 0;
        for position in 0..len {
            let remaining = len - position - 1;
            loop {
                if slot >= SLOT_CAPACITY {
                    return None;
                }
                let skipped = binomial(SLOT_CAPACITY - slot - 1, remaining);
                if ordinal < skipped {
                    break;
                }
                ordinal -= skipped;
                slot += 1;
            }
            set.push(slot)?;
            slot += 1;
        }
        (ordinal == 0).then_some(set)
    }
}

/// `C(n, k)` — how many ways `k` slots are held out of `n`. Saturates to zero past
/// `n`, which is the honest count.
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

/// `Σ C(SLOT_CAPACITY, k)` for `k ≤ cap` — how many distinct held sets fit under a
/// cap, and so the radix of a held-set field.
const fn sets_up_to(cap: usize) -> u64 {
    let mut total = 0;
    let mut k = 0;
    while k <= cap {
        total += binomial(SLOT_CAPACITY, k);
        k += 1;
    }
    total
}

/// Split a [`LevelModifiers`] into the token's fields: the active toggles as slot
/// numbers, and the gate. A struct destructure names every field, so a new modifier
/// will not compile until it is given a **permanent slot** here — and that slot is
/// then load-bearing forever (see [`SLOT_CAPACITY`]): appending is free, renumbering
/// silently rewrites every token ever shared.
///
/// `None` when more than [`MODIFIER_CAP`] are active. That cap is a format promise
/// §12.6 does not enforce — its three sources compose harder-ward without a bound —
/// so this is where the promise is actually kept.
fn modifier_slots(m: LevelModifiers) -> Option<(SlotSet, IntelGate)> {
    let LevelModifiers {
        guards_always_search_hideouts,
        sighting_lost_calls_a_guard,
        body_found_calls_two_guards,
        always_show_vision_cones,
        full_layout_known,
        calm_guards_detect_only_their_cone,
        automatic_doors,
        guard_count,
        intel_count,
        equipment_cache,
        intel_to_exit,
    } = m;
    let mut slots = SlotSet::default();
    for (slot, active) in [
        guards_always_search_hideouts,
        sighting_lost_calls_a_guard,
        body_found_calls_two_guards,
        always_show_vision_cones,
        full_layout_known,
        calm_guards_detect_only_their_cone,
        // Slot 6, appended (#452) — never inserted. A slot number is permanent: the
        // token encodes it by index, so renumbering would silently re-point every
        // token ever shared at a different modifier.
        automatic_doors,
        // Slots 7 and 8, appended (#232): the guard-count knob's two **ends**, one
        // slot each, rather than a new field of its own in the chain.
        //
        // That is the whole reason a bounded knob can be added for free. A field
        // would move every radix after it and change what a token *means* — the
        // §8 format bump, and every link ever shared stops decoding. Two more slots
        // out of the 256 reserved change no radix at all (spec §3): the encoded set
        // simply names one more member. The knob's baseline names neither slot, so a
        // run at the §10.2 count encodes byte-for-byte as it did before this existed.
        matches!(guard_count, GuardCount::More),
        matches!(guard_count, GuardCount::Fewer),
        // Slots 9 and 10, appended (#207): the intel-count knob's two ends, on the
        // guard knob's terms and for the same price — two more of the 256 reserved
        // slots, no radix moved, and a run at the §10.2 count naming neither. This is
        // what makes a campaign facility's flavour ride in its **token** (§12.7): the
        // level you are handed is the level the map offered, consoles and all.
        matches!(intel_count, IntelCount::More),
        matches!(intel_count, IntelCount::Fewer),
        // Slot 11, appended (#209): the equipment cache, a plain toggle on the terms
        // slot 6 was appended under. A campaign facility's flavour is what turns it on,
        // so this is the last piece of "the level you are handed is the level the map
        // offered" (§12.7) — a Workshop's token carries its crate.
        equipment_cache,
    ]
    .into_iter()
    .enumerate()
    {
        if active {
            slots.push(slot)?;
        }
    }
    Some((slots, intel_to_exit))
}

/// Rebuild a [`LevelModifiers`] from the token's fields — the inverse of
/// [`modifier_slots`], over the same permanent slots. `None` for a slot this build
/// has no modifier for: a token from a build with more modifiers than this one, which
/// is rejected exactly rather than guessed at.
///
/// Also `None` for a set naming **both ends of the guard-count knob** (#232). The
/// encoder cannot produce one — a knob holds one value — so such a token describes a
/// config no run can be in, and there is no honest way to pick which end was meant.
/// It joins the other "this is not a run this game can produce" rejections, and falls
/// gracefully to a fresh run like any token that does not decode.
fn modifiers_from_slots(slots: &SlotSet, gate: IntelGate) -> Option<LevelModifiers> {
    let mut active = [false; MODIFIER_FIELDS];
    for slot in slots.iter() {
        *active.get_mut(slot)? = true;
    }
    let [guards_always_search_hideouts, sighting_lost_calls_a_guard, body_found_calls_two_guards, always_show_vision_cones, full_layout_known, calm_guards_detect_only_their_cone, automatic_doors, more_guards, fewer_guards, more_intel, fewer_intel, equipment_cache] =
        active;
    let guard_count = match (more_guards, fewer_guards) {
        (false, false) => GuardCount::Baseline,
        (true, false) => GuardCount::More,
        (false, true) => GuardCount::Fewer,
        (true, true) => return None, // both ends at once is not a config a run can hold
    };
    // The same rejection over the intel knob (#207): a knob holds one value, so a set
    // naming both its ends describes a config no run can be in.
    let intel_count = match (more_intel, fewer_intel) {
        (false, false) => IntelCount::Baseline,
        (true, false) => IntelCount::More,
        (false, true) => IntelCount::Fewer,
        (true, true) => return None,
    };
    Some(LevelModifiers {
        guards_always_search_hideouts,
        sighting_lost_calls_a_guard,
        body_found_calls_two_guards,
        always_show_vision_cones,
        full_layout_known,
        calm_guards_detect_only_their_cone,
        automatic_doors,
        guard_count,
        intel_count,
        equipment_cache,
        intel_to_exit: gate,
    })
}

/// How many modifier slots this build actually uses — the live count, against which
/// a decoded slot number is checked. It grows into [`SLOT_CAPACITY`] without changing
/// the format. Not the same as the number of *fields*: the guard-count knob (#232) and
/// the intel-count knob (#207) spend one slot per end.
const MODIFIER_FIELDS: usize = 12;

/// The tech a loadout holds, as slot numbers over [`AbilityId::TECH`]'s permanent
/// order. `None` when the loadout is not one a run can hold: over the §8.3 cap, or
/// missing an innate ability the token does not carry and cannot describe the absence
/// of.
fn tech_slots(abilities: Loadout) -> Option<SlotSet> {
    if !AbilityId::INNATE
        .into_iter()
        .all(|id| abilities.contains(id))
    {
        return None;
    }
    let mut slots = SlotSet::default();
    for (slot, id) in AbilityId::TECH.into_iter().enumerate() {
        if abilities.contains(id) {
            slots.push(slot)?;
        }
    }
    Some(slots)
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

/// Spread the packed value over the token's digits — see [`SCRAMBLE`] for the three
/// jobs this does, only one of which is cosmetic.
fn scramble(value: u128) -> u128 {
    mul_mod(value, SCRAMBLE, TOKEN_SPACE)
}

/// Undo [`scramble`].
fn unscramble(value: u128) -> u128 {
    mul_mod(value, UNSCRAMBLE, TOKEN_SPACE)
}

/// `a · b mod m`, by double-and-add rather than by multiplying.
///
/// The direct form overflows: both factors run to nearly [`TOKEN_SPACE`] (~2^85), and
/// their product needs 170 bits. Doubling instead keeps every intermediate under
/// `2 · m`, so nothing here can exceed 2^86. Eighty-odd iterations, on a path that
/// runs twice per token.
const fn mul_mod(mut a: u128, mut b: u128, m: u128) -> u128 {
    let mut product = 0;
    a %= m;
    while b > 0 {
        if b & 1 == 1 {
            product = (product + a) % m;
        }
        a = (a << 1) % m;
        b >>= 1;
    }
    product
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
    // **What the equipment cache holds is decided before the building is carved**
    // (§2.2/§14 v3/#209), because whether there *is* one depends on the answer: the draw
    // is over the tech this run does not already carry
    // ([`cache_contents`](crate::cache_contents)), and a run that carries all of it has
    // nothing left to find. Rather than plant an empty crate, the modifier is withdrawn
    // for this facility and the level generates without one — so the grid and the prize
    // can never disagree.
    //
    // Unreachable in a real run and defined all the same: a campaign is seven facilities
    // against a pool of eight, and a level-seed token cannot carry more than
    // `AbilityId::MAX_TECH_HELD` tech at all. It takes a hand-built loadout to get here.
    let salvage = level
        .modifiers
        .equipment_cache
        .then(|| crate::salvage::cache_contents(level.seed, level.abilities))
        .flatten();
    let modifiers = LevelModifiers {
        equipment_cache: salvage.is_some(),
        ..level.modifiers
    };
    // **Modifiers are resolved before the carve** (§12.6/#452). Most of them are read
    // at runtime, but `automatic_doors` decides what a doorway *is*, so it has to
    // reach the generator — threaded as a parameter, never consulted from a global.
    let (layout, placement) = generate_level(config, &modifiers, &mut rng)?;
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
    // After `with_level`, which is where the loadout lands: the crate's contents were
    // drawn from that same loadout, and the deck this grants against has to be the one
    // the run is actually holding.
    .with_level(*level)
    .with_cache(salvage))
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
            LevelSeed::quick_play(SEED_SPACE - 1),
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

    /// The **difficulty axis over quick play** (§12.6/#297): the level draws its
    /// modifiers onto the preset, the baseline draws nothing, and — the part that
    /// matters for the format — the resolved set round-trips through the token with
    /// **no new field**. The difficulty number is not carried because it does not need
    /// to be: it is spent before the run boots, and what a shared token hands over is
    /// the run itself rather than a recipe for re-rolling one.
    #[test]
    fn a_difficulty_draws_onto_quick_play_and_still_fits_the_token() {
        for seed in [0, 42, 8371, SEED_SPACE - 1] {
            // The baseline is quick play exactly — the axis costs it nothing.
            assert_eq!(
                LevelSeed::quick_play_at(seed, Difficulty::Standard),
                LevelSeed::quick_play(seed),
            );
            for position in Difficulty::ALL {
                let level = LevelSeed::quick_play_at(seed, position);
                // The seed and the loadout are untouched: only the rules differ, so
                // the ±N arms of a comparison are the same building (§12.4).
                assert_eq!(level.seed, seed);
                assert_eq!(level.abilities, LevelSeed::quick_play(seed).abilities);
                // Quick play's objective survives every draw: `union` composes the
                // gate harder-ward, and the pool holds no knob that could relax it.
                assert_eq!(level.modifiers.intel_to_exit, IntelGate::All);
                // The drawn toggles are on top of the preset, and the whole set is
                // still a config the token can carry — no new field, same width.
                assert_eq!(level.modifiers.active().len(), position.picks() + 1);
                let token = token(level);
                assert_eq!(token.len(), TOKEN_LEN);
                assert_eq!(
                    LevelSeed::decode(&token),
                    Some(level),
                    "{position:?} at seed {seed} does not round-trip",
                );
                // …and the run it boots plays under exactly those rules.
                let state = start_level(&level).expect("the v1 recipe places");
                assert_eq!(state.modifiers(), level.modifiers);
            }
        }
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
            Loadout::innate(),
            Loadout::innate().with(AbilityId::Camouflage),
            Loadout::innate()
                .with(AbilityId::Decoy)
                .with(AbilityId::Dephase),
            // The cap itself: the widest loadout a run can hold (§8.3).
            Loadout::innate()
                .with(AbilityId::Decoy)
                .with(AbilityId::Confusion)
                .with(AbilityId::Vision),
        ];
        // The boolean fields as a bitmask rather than one nested loop each: the knob
        // (#232) would have made an eighth level of nesting out of a test whose whole
        // content is "every combination", and the cache (#209) a ninth.
        const TOGGLE_FIELDS: u32 = 8;
        for bits in 0..(1u32 << TOGGLE_FIELDS) {
            let on = |field: u32| bits & (1 << field) != 0;
            let (search, sighting, body, cones, layout, cone_only, doors, cache) =
                (on(0), on(1), on(2), on(3), on(4), on(5), on(6), on(7));
            for (guard_count, intel_count) in [
                (GuardCount::Baseline, IntelCount::Baseline),
                (GuardCount::More, IntelCount::More),
                (GuardCount::Fewer, IntelCount::Fewer),
                // The two knobs crossed the other way, so the sweep covers a set
                // holding one end of each rather than only matched pairs (#207).
                (GuardCount::More, IntelCount::Fewer),
            ] {
                for gate in gates {
                    for abilities in loadouts {
                        let modifiers = LevelModifiers {
                            guards_always_search_hideouts: search,
                            sighting_lost_calls_a_guard: sighting,
                            body_found_calls_two_guards: body,
                            always_show_vision_cones: cones,
                            full_layout_known: layout,
                            calm_guards_detect_only_their_cone: cone_only,
                            automatic_doors: doors,
                            guard_count,
                            intel_count,
                            equipment_cache: cache,
                            intel_to_exit: gate,
                        };
                        let level = LevelSeed {
                            seed: 12345,
                            modifiers,
                            abilities,
                        };
                        // With more modifier slots than [`MODIFIER_CAP`], the space
                        // now has a corner the format deliberately refuses (see
                        // [`modifier_slots`]) — the codec is total over the configs a
                        // run *can hold*, which is the claim, and refusing the rest
                        // **exactly** is the other half of it. A non-baseline knob
                        // spends a slot like any toggle, so it counts here too.
                        let active = [
                            search, sighting, body, cones, layout, cone_only, doors, cache,
                        ]
                        .into_iter()
                        .filter(|&flag| flag)
                        .count()
                            + usize::from(guard_count != GuardCount::Baseline)
                            + usize::from(intel_count != IntelCount::Baseline);
                        if active > MODIFIER_CAP {
                            assert_eq!(
                                level.encode(),
                                None,
                                "over the cap must be refused, not truncated: {level:?}",
                            );
                            continue;
                        }
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

    /// Every tech subset up to the cap round-trips through its combination index —
    /// the encoding that costs `log2(C(n, k))` rather than a bit per catalogue entry.
    /// How many tech subsets a run can hold: `Σ C(pool, k)` for `k ≤ MAX_TECH_HELD`.
    /// Note this is *not* [`TECH_SPACE`], which counts over the 256 reserved slots —
    /// the gap between them is the room the roster still has to grow into.
    const TECH_HOLDABLE_SETS: u64 = {
        let mut total = 0;
        let mut k = 0;
        while k <= AbilityId::MAX_TECH_HELD {
            total += binomial(AbilityId::TECH.len(), k);
            k += 1;
        }
        total
    };

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
        // The whole holdable space, not a sample of it. Counted off the catalogue
        // rather than written down: the pool grows every time a tech ships, and a
        // number here would be wrong within a ticket (see QUICK_PLAY_TECH_GRANT).
        assert_eq!(seen, TECH_HOLDABLE_SETS);
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
        const FROZEN: &str = "prbjdokbxcqgjnrnco";
        let expected = LevelSeed {
            seed: 8371,
            modifiers: LevelModifiers {
                guards_always_search_hideouts: true,
                sighting_lost_calls_a_guard: false,
                body_found_calls_two_guards: true,
                always_show_vision_cones: false,
                full_layout_known: false,
                calm_guards_detect_only_their_cone: false,
                // Slot 6 did not exist when this token was minted (#452), so the
                // frozen string decodes it off — which is exactly the property the
                // frozen token is here to prove: appending a slot leaves every token
                // ever shared naming the run it always named. Slots 7 and 8 (#232)
                // are the same story one ticket later — the knob decodes at its
                // baseline, which is the run this token has always named.
                automatic_doors: false,
                guard_count: GuardCount::Baseline,
                // Slots 9 and 10 (#207) are the third telling of the same story: the
                // intel knob decodes at its baseline, so the token still names the run
                // it named the day it was minted.
                intel_count: IntelCount::Baseline,
                // And slot 11 (#209) is the fourth: a token minted before caches
                // existed decodes as the facility it always named — one with no crate
                // in it.
                equipment_cache: false,
                intel_to_exit: IntelGate::All,
            },
            abilities: Loadout::innate()
                .with(AbilityId::Camouflage)
                .with(AbilityId::Autodoors)
                .with(AbilityId::Vision),
        };
        assert_eq!(token(expected), FROZEN, "the frozen token changed spelling");
        assert_eq!(
            LevelSeed::decode(FROZEN),
            Some(expected),
            "the frozen token no longer names its run",
        );
    }

    /// **A token from another format version is rejected.** Simulated by re-spelling
    /// a valid payload under a perturbed [`SCRAMBLE`] — exactly what a different
    /// [`FORMAT_MAJOR`], slot capacity or cap produces, since all of them feed
    /// [`MAGIC`] and so the multiplier.
    ///
    /// Rejection here is probabilistic — [`rejection_rate`] bounds it — so this
    /// asserts the *rate* over many seeds rather than one lucky refusal. Contrast
    /// `the_scramble_catches_every_realistic_slip`, where the guarantee is absolute.
    #[test]
    fn a_token_from_another_version_is_rejected() {
        let foreign = scramble_from(MAGIC.wrapping_add(1), SCRAMBLE_NONCE);
        let survivors = (0..20_000u64)
            .filter(|&seed| {
                let packed = unscramble(from_letters(&token(LevelSeed::sim(seed))).expect("own"));
                LevelSeed::decode(&to_letters(mul_mod(packed, foreign, TOKEN_SPACE))).is_some()
            })
            .count();
        // ~1 in `rejection_rate()` slip through by collision; anything near 20,000
        // would mean the version is not reaching the encoding at all.
        let expected = 20_000 / rejection_rate() as usize;
        assert!(
            survivors <= expected * 4 + 10,
            "{survivors} of 20000 foreign tokens decoded (expected about {expected})",
        );
    }

    /// **Every realistic slip is caught with certainty** — not at
    /// [`rejection_rate`], but always.
    ///
    /// A corruption shifts the packed value by a delta fixed by which characters
    /// changed; it is caught exactly when that delta carries the value clear of
    /// [`PAYLOAD_SPACE`]. This asserts it over **every distinct delta** a
    /// single-character slip or a transposition can produce — so it is an
    /// enumeration, not a sample, and it holds for every token rather than for the
    /// ones a test happened to try.
    ///
    /// This is what [`SCRAMBLE_NONCE`] exists to satisfy, and the property is
    /// entirely a gift of the multiplier: `the_scramble_constant_is_load_bearing`
    /// shows what a careless one costs. If a format change fails this, bump the
    /// nonce until it passes — do not relax the test.
    #[test]
    fn the_scramble_catches_every_realistic_slip() {
        for (delta, what) in corruption_deltas() {
            let shifted = mul_mod(delta, UNSCRAMBLE, TOKEN_SPACE);
            let distance = shifted.min(TOKEN_SPACE - shifted);
            assert!(
                distance >= PAYLOAD_SPACE,
                "{what} can go undetected: delta lands {distance} from a valid value",
            );
        }
    }

    /// The multiplier is **load-bearing for error detection**, not decoration: a
    /// careless one leaves hundreds of single-character slips silently decoding to a
    /// different run. Recorded as a test so nobody "simplifies" [`SCRAMBLE`] to
    /// something tidy without seeing the cost.
    #[test]
    fn the_scramble_constant_is_load_bearing() {
        let blind = |multiplier: u128| {
            corruption_deltas()
                .filter(|&(delta, _)| {
                    let shifted = mul_mod(delta, multiplier, TOKEN_SPACE);
                    shifted.min(TOKEN_SPACE - shifted) < PAYLOAD_SPACE
                })
                .count()
        };
        assert_eq!(blind(UNSCRAMBLE), 0, "the chosen constant is clean");
        assert!(
            blind(1) > 100,
            "an unscrambled token should be full of blind spots — if this fails the \
             test no longer demonstrates anything",
        );
    }

    /// Every distinct value a single-character slip or a transposition can add to the
    /// packed value. Signs are symmetric under the distance test, so only the
    /// positive of each pair is enumerated.
    fn corruption_deltas() -> impl Iterator<Item = (u128, String)> {
        let power = |i: usize| ALPHABET.pow(i as u32);
        let singles = (0..TOKEN_LEN).flat_map(move |i| {
            (1..ALPHABET).map(move |d| (d * power(i), format!("a slip of {d} at position {i}")))
        });
        let transpositions = (0..TOKEN_LEN).flat_map(move |i| {
            ((i + 1)..TOKEN_LEN).flat_map(move |j| {
                (1..ALPHABET).map(move |d| {
                    (
                        d * (power(j) - power(i)),
                        format!("a transposition of {i} and {j} differing by {d}"),
                    )
                })
            })
        });
        singles.chain(transpositions)
    }

    /// **The worst case actually round-trips.** The maximum seed crossed with the
    /// fullest held sets is the corner the packing is tightest in, and the corner a
    /// test that only tries small seeds and simple configs never reaches — which is
    /// exactly how the previous format shipped an overflow.
    #[test]
    fn every_extreme_config_round_trips() {
        let widest_tech = Loadout::innate()
            .with(AbilityId::TECH[AbilityId::TECH.len() - 3])
            .with(AbilityId::TECH[AbilityId::TECH.len() - 2])
            .with(AbilityId::TECH[AbilityId::TECH.len() - 1]);
        // The widest set the format admits is [`MODIFIER_CAP`] slots, and the widest
        // *payload* takes the **highest** ones — so this is the top five a run can
        // actually hold, which now reaches slot 11, the equipment cache (#209). Each
        // knob's two ends cannot both be held, so the highest five are the cache (11),
        // one end of each knob (10 and 8), and the two highest toggles below them
        // (6 and 5). Holding more than five at once is over the cap and refused
        // outright, asserted in `every_config_round_trips`.
        let all_modifiers = LevelModifiers {
            guards_always_search_hideouts: false,
            sighting_lost_calls_a_guard: false,
            body_found_calls_two_guards: false,
            always_show_vision_cones: false,
            full_layout_known: false,
            calm_guards_detect_only_their_cone: true,
            automatic_doors: true,
            guard_count: GuardCount::Fewer,
            intel_count: IntelCount::Fewer,
            equipment_cache: true,
            intel_to_exit: IntelGate::All,
        };
        for seed in [0, 1, SEED_SPACE - 2, SEED_SPACE - 1] {
            for modifiers in [LevelModifiers::default(), all_modifiers] {
                for abilities in [Loadout::innate(), widest_tech] {
                    let level = LevelSeed {
                        seed,
                        modifiers,
                        abilities,
                    };
                    assert_eq!(
                        LevelSeed::decode(&token(level)),
                        Some(level),
                        "the extreme corner does not round-trip: {level:?}",
                    );
                }
            }
        }
    }

    /// The held-set ordinal is a **bijection** onto the sets it claims to enumerate:
    /// every ordinal below the field's radix unranks to a set that ranks back, and
    /// the radix itself is one past the last.
    ///
    /// Dense is the property under test. A sparse encoding would leave holes here,
    /// and holes are what overflowed the last format.
    #[test]
    fn the_slot_ordinal_is_a_dense_bijection() {
        for (cap, space) in [
            (AbilityId::MAX_TECH_HELD, TECH_SPACE),
            (MODIFIER_CAP, MODIFIER_SPACE),
        ] {
            // Walk the whole space where it is small, and its edges where it is not.
            let sampled = (0..space.min(20_000)).chain(space.saturating_sub(64)..space);
            for ordinal in sampled {
                let set = SlotSet::from_ordinal(ordinal, cap).expect("in range");
                assert!(set.len <= cap);
                assert_eq!(set.ordinal(cap), Some(ordinal), "cap {cap}");
            }
            assert_eq!(SlotSet::from_ordinal(space, cap), None, "one past the last");
        }
    }

    /// A token naming a slot **this build has no entry for** is rejected exactly —
    /// the "token from a newer build" case, and the reason growth is safe: slots
    /// below the live roster keep working while slots above it are refused rather
    /// than guessed at.
    #[test]
    fn a_token_naming_an_unknown_slot_is_rejected() {
        // A modifier slot past the live roster, packed by hand.
        let mut unknown = SlotSet::default();
        unknown.push(MODIFIER_FIELDS).expect("one slot fits");
        assert_eq!(
            modifiers_from_slots(&unknown, IntelGate::All),
            None,
            "a modifier this build does not have",
        );
        // And the same for tech, through the public surface.
        let mut chain = Chain::default();
        chain.push(0, u128::from(SEED_SPACE)).expect("seed");
        chain.push(0, u128::from(GATE_VARIANTS)).expect("gate");
        chain
            .push(0, u128::from(MODIFIER_SPACE))
            .expect("no modifiers");
        let mut beyond = SlotSet::default();
        beyond.push(AbilityId::TECH.len()).expect("one slot fits");
        let ordinal = beyond
            .ordinal(AbilityId::MAX_TECH_HELD)
            .expect("within the cap");
        chain
            .push(u128::from(ordinal), u128::from(TECH_SPACE))
            .expect("tech");
        assert_eq!(
            LevelSeed::decode(&to_letters(scramble(chain.0))),
            None,
            "a tech slot this build does not have",
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
        assert_eq!(mul_mod(SCRAMBLE, UNSCRAMBLE, TOKEN_SPACE), 1);
        for value in [0, 1, 2, 4095, TOKEN_SPACE - 1, TOKEN_SPACE / 3] {
            assert_eq!(unscramble(scramble(value)), value);
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
