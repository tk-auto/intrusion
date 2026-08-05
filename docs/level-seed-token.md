# The level-seed token

**Format v1.** The string that carries a run: `prbjdokbxcqgjnrnco`.

This is the specification. The design doc (§12.4, §12.6) says *that* a run has a
shareable identity and why; this says what that identity is written as. The
implementation is `crates/core/src/level_seed.rs`, and every claim below is pinned by
a test named in the margin.

---

## 1. What it is

Eighteen characters, `a`–`z`, nothing else.

A token encodes a run's whole reproducible starting config — `(seed, modifiers,
abilities)` — so handing one to someone reproduces **the run**, not a family of runs
that happen to share a number. With a replay's `[inputs]` it reproduces a run
byte-for-byte (§12.4).

Three properties fall out of the shape:

- **Fixed width** — a wrong length is rejected before a single digit is parsed, and
  a truncated paste is the most likely handling error.
- **All alphabetic, case-insensitive on input** — no `0`/`O` or `1`/`l` to misread,
  and a token survives being read aloud. Always emitted lowercase, so one config has
  exactly one spelling.
- **Unreserved throughout** — drops into `?seed=` or `#seed=` with no escaping.

**A decimal number is not a token.** See §7.

---

## 2. Field layout

The token is a **mixed-radix chain**. Each field is pushed as a digit
(`value = value × radix + digit`); the packed value is scrambled; the result is
written in base 26, most significant character first, zero-padded with `a`.

| Field | Radix | Bits | Share |
|---|---|---|---|
| Seed | `2^17` | 17.00 | 24% |
| Intel gate | 3 | 1.58 | 2% |
| Modifiers active | `Σ C(256, k)`, k ≤ 5 = 8,987,138,113 | 33.07 | 46% |
| Tech held | `Σ C(256, k)`, k ≤ 3 = 2,796,417 | 21.42 | 30% |
| **Payload** | **9,882,220,285,345,577,435,136** | **72.07** | |
| Token capacity (`26^18`) | | 84.61 | |
| Slack — *this is the integrity check* (§5) | | 12.54 | |

Every radix is a constant. That is a deliberate property, not an accident of the
current numbers: it makes the packed space exactly the product above, so the
"residue is zero after the last field" test is an exact range check. See §4 for what
a *variable* radix costs.

**The innate set is not carried.** §8.3 makes innate abilities always held, so decode
restores them rather than reading them. A loadout missing one is not a run, and has
no token (§6).

---

## 3. Slots are permanent

A held set — the active modifiers, the tech a run holds — is encoded as a
**combination index over 256 reserved slots**, not over the entries that exist today
(six tech and seventeen modifier slots as of writing).

This is the single most important property of the format.

> **Every ability and every modifier owns a permanent slot number. Adding an entry
> fills the next free slot. No radix changes, no token changes meaning, and every
> link ever shared keeps working.**

### Why it exists

The previous format sized itself against the *live* roster. When the Vision passive
joined the tech pool (#286), the pool went from five entries to six, the seeded draw
re-ran over a pool of a different size, and **every `#seed=8371` link shared before
that commit silently began booting a different loadout** — same facility, different
abilities, nothing anywhere saying so. Reserving slots ahead of the roster is what
makes that structurally impossible rather than merely detected.

### The discipline it costs

- **Slot numbers are load-bearing forever.** Appending is free; renumbering silently
  rewrites every token ever shared — with no error, and nothing to notice it by.
- **Retire an entry in place, never by removing it.** Leave a **placeholder** in its
  slot: a reserved entry that holds the position and is never granted, drawn, or
  offered. Deleting the entry and closing the gap shifts every entry after it, so
  every token in the wild starts naming different abilities. A tombstone, not a hole.
- **Don't reorder for tidiness.** The list's order is data that tokens depend on, not
  a style choice.
- Reusing a slot, or renumbering, is a **format version bump** (§8) and its own
  ticket — never a quiet edit inside another one.
- Reserving 256 against a target of ~100 live entries is what makes the churn
  affordable: a placeholder costs a slot, and there are 256 of them.

**A bounded knob spends one slot per end, not a field of its own.** The guard count
(#232) is the worked example: `More` takes slot 7 and `Fewer` slot 8, and its baseline
names neither, so a run at the §10.2 count encodes byte-for-byte as it did before the
knob existed. The **intel count** (#207) is the second telling, at slots 9 and 10 — the
campaign map's reward axis, and the reason a campaign facility's *flavour* rides in its
own token rather than in a recipe the token cannot carry (design §12.7). The alternative — a new radix-3 field in the chain beside the intel
gate's — would have moved every field after it and changed what a token *means*: a §8
version bump, and every link ever shared stops decoding. Two slots out of 256 change no
radix at all. **So a knob joins the format for free if its ends can be slots**, and only
a knob whose values are too many to spell as slots is worth a field.

**A knob's ends need not be adjacent, and one of them may arrive years later.** The
**layout knob** (#233) is that case: slot 4 was the `full_layout_known` *toggle*, and
when the opposite rule shipped the two became one knob — the toggle's slot kept its
meaning as the knob's easier end, and the harder end was **appended at slot 16** rather
than tidied in beside it. Both halves of that follow the rule above: a slot number is
permanent, so an existing slot whose meaning is unchanged keeps decoding every token
ever minted, and a new value takes the next free position wherever that lands. Slot
order is the wire format; it was never a reading order.

A set naming **both ends at once** is refused by `decode` (§6): the encoder cannot
produce one, so it describes no run, and there is no honest way to pick which end was
meant. This holds over the slot *pairs*, not over adjacency — the layout knob's ends are
twelve slots apart.

The compiler helps: `modifier_slots` destructures `LevelModifiers` by name, so a new
modifier will not compile until it is given a slot.

> **A slot is not the same promise as a seed.** These rules keep a token *naming the
> same run* — the same seed, modifiers and loadout — forever. They do **not** promise
> that the seed carves the same building: a level is a function of the seed **and the
> generator** (§12.4), so any change to generation re-carves every token ever shared,
> quietly and by design. #452 is the worked example: making automatic doors a level
> modifier dropped a per-doorway RNG draw, which shifted the stream, so a `#seed=N`
> link from before it names a different facility. The token still decodes to exactly
> the run it always described; that run is simply played in a different building.
>
> The alternative was to consume a throwaway draw and keep the stream aligned. It was
> rejected: it buys compatibility by making the generator perform a step it does not
> need, and a generator with a vestigial draw in it is a generator nobody can read.
> Take the break, refresh the committed baseline and the replay fixtures in the same
> PR, and say so in the commit.

*Checked at build time by the `const _` block in `level_seed.rs` — a roster that had
outgrown its slots fails the compile, not the suite. Test:
`a_token_naming_an_unknown_slot_is_rejected`.*

---

## 4. Held sets are dense combination indexes

A **bitset** costs one bit per catalogue entry whether set or not, so its length
tracks the *roster*. A **combination index** costs `log2(C(n, k))`, so its length
tracks the *cap* and grows only logarithmically in the roster.

At 256 slots that is the difference between **18 characters and 51**. It is the only
reason a reserved space this large is affordable.

The caps are therefore part of the format: `MAX_TECH_HELD` (3, §8.3) and
`MODIFIER_CAP` (5). Both feed the radices, so both are in the magic (§5).

`MODIFIER_CAP` deserves a note: unlike the ability cap it is a **format promise that
§12.6 does not enforce** — its three composing sources (mode, alert, flavour) stack
harder-ward without a bound. `modifier_slots` is where the promise is actually kept,
by refusing to encode a config that exceeds it.

### Dense, not count-plus-index

The ordinal is `Σ_{i<count} C(n, i) + <lexicographic rank within this size>`: a single
digit whose radix is the constant `Σ_k C(n, k)`. The count is recovered on decode from
*which size-block the ordinal falls in*, and asserted against the cap.

The obvious alternative — a count digit beside an index digit of radix `C(n, count)` —
spends the same information but leaves the packed space **sparse**. Its maximum is
bounded by the largest radix on any path rather than by the sum over paths, which is
larger. A previous draft of this format shipped that form and silently overflowed the
token for configs on an expensive path: the encode succeeded, the decode returned a
different config, and the capacity test asserted the *dense* bound and so stayed
green.

Two habits came out of that, both worth keeping: assert the encoder's real bound, and
test the extreme corner (maximum seed × fullest held sets) rather than only the
convenient middle.

*Tests: `the_slot_ordinal_is_a_dense_bijection`, `every_extreme_config_round_trips`.
That the payload fits its characters at all is a build-time assertion, so it cannot be
green while the encoder overflows — which is exactly how the sparse draft slipped
through.*

---

## 5. Integrity: there is no checksum field

Nor should there be. A check field and unused range are **interchangeable**: the
scramble is a bijection over `26^18`, so an arbitrary token lands on a uniform value,
and exactly `PAYLOAD_SPACE` of `TOKEN_SPACE` values are valid — whatever fraction of
the space a check field happens to occupy. Bits spent on a checksum buy nothing that
spending them on length does not.

So integrity is the slack:

**1 in 2,983 arbitrary tokens decodes to something.**

That figure covers a random string, and a token from another format version. But it
is the *worst* case, and not the case that matters most:

| Error | Detection |
|---|---|
| Wrong length | **100%**, before any arithmetic |
| Any single-character slip | **100%** — all 900 distinct deltas, by enumeration |
| Any transposition | **100%** — all 7,650 distinct deltas, by enumeration |
| Wrong format version | 1 in 2,983 |
| Random string | 1 in 2,983 |
| Two independent slips | 1 in 2,983 average, **not uniform** — see below |

### Why the certainty

A corrupted token decodes to `value + δ`, where δ is fixed by which characters
changed. The corruption is caught exactly when δ carries the value clear of the
payload range — so detection is **bimodal per δ** (certain or not at all), rather
than a flat probability. Across all single-character slips and transpositions there are only 8,516 distinct δ
(900 and 7,650, overlapping in 34), and the scramble constant is chosen so that every
one of them lands out of range.

That is *stronger* than a checksum, which always leaves a 2⁻ⁿ tail even for one
changed character.

### The scramble constant is load-bearing

`SCRAMBLE` is not decoration. It does three jobs:

1. Stops consecutive seeds sharing a visible prefix (without it, neighbouring runs
   differ only in their last few characters and the token reads as broken).
2. Carries `MAGIC`, and so the format version — a token from another version
   unscrambles under a different constant and fails the range check. **This is why
   there is no version field: it costs no bits.**
3. **Detects typos**, per the above.

A carelessly chosen multiplier leaves hundreds of single-character slips
undetectable — with no scramble at all, 708 of 900. The constant is derived from
`MAGIC` and steered by `SCRAMBLE_NONCE` onto one that passes the audit; the first
five derivations fail it. **If a format change fails the audit test, bump the nonce
until it passes — do not weaken the test.**

### The known weakness

Two *independent* single-character slips in one token are **not** uniformly detected.
Expected blind spots scale as `pairs × 2 × payload / capacity`, around 64 of the 95,625
pairs. This cannot be tuned away — driving it to zero would need a search through
roughly e⁶⁴ constants — and a flat guarantee there would require a real check symbol,
costing one to two characters.

Left imperfect deliberately: for a string that is nearly always pasted rather than
typed, and where every *single* slip and transposition is already caught with
certainty, it is the right corner to leave open.

### Not tamper resistance

Deliberately absent, and not achievable: any key would live in the wasm and can be
read out, so a MAC would be obfuscation rather than security. Nor is it needed —
forging a token yields a run whose abilities the player could have drawn anyway, in a
game with permadeath and no meta-progression (§2). If a daily challenge ever wants
verification it belongs server-side against a submitted replay (§12.4), not here.

*Tests: `the_scramble_catches_every_realistic_slip`,
`the_scramble_constant_is_load_bearing`, `a_token_from_another_version_is_rejected`,
`a_malformed_token_decodes_to_none`, `the_scramble_inverts_exactly`.*

---

## 6. When there is no token

`encode` returns `None` — the honest answer, rather than a token that would decode to
something else. Four cases, all meaning *"this is not a run this game can produce"*:

- a seed wider than `SEED_BITS`;
- a loadout over the §8.3 tech cap (`Loadout::full` documents itself as exactly that);
- a loadout missing an innate ability, which the token does not carry (§2);
- more than `MODIFIER_CAP` modifiers active at once.

`decode` likewise refuses a slot set naming **both ends of one bounded knob** (§3) — a
config no run can be in.

Every surface that shows or shares a token already had a "there is no token for this"
branch, because a hand-built state has never had one.

`decode` returns `None` for anything it cannot read, and callers turn that into a
**fresh run — never a bricked page** (#110/#197).

*Test: `a_config_a_run_cannot_hold_has_no_token`.*

---

## 7. A number is not a token

A bare `?seed=8371` named *this build's quick-play preset applied to 8371* — not a
run. §12.4 settles a run's identity as `(seed, modifiers, abilities, inputs)`; a bare
seed carried one quarter of it and trusted the rest to be reconstructed by whatever
opened the link. That is how the #286 break travelled.

So the decimal form is gone as an **input** as well as an output. Two costs, both
real and neither retroactive:

- **"Try seed 8371" no longer works.** A player who wants a fresh run presses new-run
  and shares what comes out. Keeping a number input alive would resurrect exactly the
  preset-versus-run ambiguity this format exists to close.
- **Links shared before v1 stop decoding.** They were already booting the wrong run;
  failing loudly is the better of the two.

Numeric seeds remain a **programmatic** concept: `LevelSeed::sim(n)` and the headless
sim's numeric sweeps (§13.2) never touch the string form.

*Test: `a_bare_seed_is_no_longer_a_token`.*

---

## 8. Sizing, and how it moves

Every number here is **[START]** — a chosen balance, not a law.

The two knobs trade one-for-one: **every bit spent on the seed is a bit taken from
rejection**, and a character is worth 26× (≈4.7 bits) of whichever you want.

| Seed | Levels | First expected repeat | Rejection |
|---|---|---|---|
| 15 bits | 32,768 | ~226 runs | 1 in 11,932 |
| 16 bits | 65,536 | ~320 runs | 1 in 5,966 |
| **17 bits** | **131,072** | **~453 runs** | **1 in 2,983** |
| 18 bits | 262,144 | ~641 runs | 1 in 1,491 |
| 20 bits | 1,048,576 | ~1,283 runs | 1 in 372 |

Seventeen is the balance point: rejection stays comfortably above 1 in 1,000, and a
repeated facility is not expected until well past 450 runs — over a thousand hours at
a 2–3 hour run.

### Growing the slot space

Slot capacity is **implied by the format version**, not carried in the token. Should
256 ever run out, v2 reserves more:

| Slots | Seed | Length | Rejection |
|---|---|---|---|
| **256 (v1)** | 17b | **18** | 1 in 2,983 |
| 512 (v2) | 17b | 20 | 1 in 7,802 |

Old tokens **keep working**, because growing the slot space is an *extension*: slots
0–255 keep their meanings, so a v1 token decoded under v1's rules still names exactly
the run it always named. A decoder keeps each version's constants in a table and
tries them newest-first.

> **Invariant: one length per version.** The length is what tells versions apart,
> before any arithmetic runs. A future version that reserves more slots must also
> grow — the tempting move of narrowing the seed to keep the length would make old
> tokens decodable under the *new* rules, at rates as bad as 1 in 14, which is the
> silent re-resolution this whole format exists to prevent. If a version ever wants a
> length already taken, pad it by a character.

---

## 9. Reference

| Constant | Value |
|---|---|
| `TOKEN_LEN` | 18 |
| `FORMAT_MAJOR` | 1 |
| `SLOT_CAPACITY` | 256 |
| `MODIFIER_CAP` | 5 |
| `AbilityId::MAX_TECH_HELD` | 3 |
| `SEED_BITS` | 17 |
| `SCRAMBLE_NONCE` | 5 |
| `MAGIC` | `0x3a1d83bec74ddf59` |

`MAGIC` folds the major version, the slot capacity, the caps and the field widths —
everything whose movement would change what a token *means*. It deliberately does
**not** fold the live roster sizes, which are free to grow into the reserved slots
without invalidating anything. That distinction is the whole of §3.
