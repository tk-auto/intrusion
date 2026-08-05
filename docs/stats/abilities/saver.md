# Saver

**Salvaged tech (§8.3)** — **passive**, with a **per-level use budget of 1**. The
first guard to reach the player in a facility is taken down where it stood instead of
capturing them: a body is left and the §7.3 clock starts, and the run continues. Then
it is spent for the rest of the level — no recharge (§8.2) — so a second guard reaching
the player, that turn or twenty turns later, captures exactly as §4.5 says.

It is **the one declared exception to a [SETTLED] rule** (§4.5: contact is the only
loss condition), which is why it is on trial rather than settled. Appendix 43 records
why it is a passive with a budget rather than the toggle-and-cooldown #243 proposed.

## What the cue says

**Nothing, and here that is a decision with a reason of its own.** A passive has no
activation to cue — the Vision argument, `crates/sim/src/cue.rs` states it in its own
arm — but the Saver is the first passive that has something to *spend*, so the obvious
next thought is a cue that makes the bot play bolder while the save is unspent.

That is deliberately not built. The measurement wanted from this ability is what the
**unchanged** policy's outcomes do when one capture is survivable; a bot that plays
differently for holding it would be deciding the ability is good and then proving
itself right, which is the §13.3 line the cue seam exists to stay on the right side of.
If a boldness knob is ever wanted it belongs to `Profile`, beside the other temperament
dials, not to a cue.

## What the sim measured

**A large effect, in every batch, both seed blocks, all four temperaments.** Innate
against innate-plus-Saver, `--runs 100 --cap 1000`, at `5d368aa+243-saver`:

| profile | `win_rate` seed 0 | `win_rate` seed 100 | captures seed 0 | takedowns seed 0 |
|---|---|---|---|---|
| `balanced` | 0.35 → **0.63** | 0.30 → **0.53** | 62 → 33 | 0 → 62 |
| `cautious` | 0.53 → **0.74** | 0.51 → **0.66** | 44 → 22 | 0 → 44 |
| `aggressive` | 0.40 → **0.69** | 0.37 → **0.59** | 60 → 31 | 22 → 86 |
| `careless` | 0.43 → **0.66** | 0.37 → **0.50** | 56 → 33 | 21 → 80 |

The usage histogram is structurally zero, as it is for any passive: it counts presses,
and nothing here is pressed. **The takedown counter is the instrument instead** — the
save leaves a body like any takedown, and the bot lands almost none of its own, so the
rise is the fire count. `balanced` reads 62 saves in 100 runs, of which 33 runs went on
to be captured anyway: the ability fires in about two runs in three and converts about
half of those.

### With competition present

Dominance is read from a real three-tech kit (§8.3's cap), so the Saver was swapped in
for Confusion in kit A:

| batch | `camouflage,decoy,confusion` | `camouflage,decoy,saver` |
|---|---|---|
| `balanced` | 0.45 | **0.62** |
| `aggressive` | 0.42 | **0.74** |

It is the strongest verb in the kit by a wide margin — the one measurement on these
pages where a single swap moves a temperament by more than 0.3.

### Read this before treating that as a balance verdict (§13.4)

An ability that refunds your first capture is worth exactly as much as your first
capture is likely, and **the bot's is very likely**: it has no fear, perfect recall,
and will take a 5% capture risk forever, so it walks into the moment this ability pays
out far more often than a frightened human does. The honest statement is the one the
numbers support — the ability is nowhere near inert, and it is strong for a player who
gets caught. Whether it flattens a *human* run's stakes is a question for the playtest,
not for this page. If it is retuned, the knob is `SAVER_USES` and the direction is not
up.

## History

- `5d368aa+243-saver` (#243) — page created with the ability: the with/without pair
  across both seed blocks, the kit-A swap, and why there is no cue.
