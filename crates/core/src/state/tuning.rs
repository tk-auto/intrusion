//! **The turn loop's tuning catalogue** (§12.2/#540) — every `[START]` range, radius
//! and clock the loop's rules turn on, in one place, with the cross-constant
//! relations pinned beside them.
//!
//! Nothing in `state.rs` itself reads these: the consumers are the sibling modules
//! (`view` for the sense ranges, `sense` for the cue decays, `effects` for the
//! blast radii and flashes, `lockdown` for its seal, `abilities` for the phase
//! eject). They live together anyway because the `const _` asserts below relate
//! them **across** those modules — a value moved in isolation could silently break
//! a relation its own module never sees. Re-exported through `state` (and `lib`),
//! so call sites and the public API are untouched by the move.

/// The player and every guard are solid and exclusive — fill 1.0 (§4.3). A cell
/// already holding one admits no other actor.
pub(crate) const ACTOR_FILL: f32 = 1.0;

/// The player's **guard-sense** range (§9.1 **[START]**): the player always knows the
/// exact cell of every guard within this Chebyshev box, **through walls** — a 21×21
/// box, the same shape as sight (§6.1) at a smaller size. It reveals *position only*;
/// facing and the cone are shown only for a guard actually seen (§9.2). Pinned by a
/// test so a later change is a deliberate, visible edit.
pub const PLAYER_SENSE_RANGE: u32 = 10;

/// The guard-sense range on a turn the player spent **waiting** (§9.1 **[START]**): a
/// 41×41 box. Wait already buys 360° vision for the turn (§8.3); it now *also* widens
/// the sense, 10 → 20 — "stop and take stock of the whole area", cost-is-load-bearing
/// applied to information (§2.3). Pinned by a test.
pub const PLAYER_SENSE_RANGE_WAITING: u32 = 20;

/// The guard-sense range while **inside a duct** (§10.7 **[START]**): a reduced box,
/// smaller than the open-floor [`PLAYER_SENSE_RANGE`]. Degraded information is the
/// duct's whole cost (§2.3): mid-crawl you perceive only your memory of the building
/// and this shortened sense, and the safe way to resurface is the mouth peek (§6.1).
/// **Waiting does not widen it** — the [`PLAYER_SENSE_RANGE_WAITING`] extension is an
/// open-floor affordance; a crawlspace is exactly where you should *not* be able to
/// take stock of the whole area. Pinned by a test.
pub const DUCT_SENSE_RANGE: u32 = 5;

/// The player's **door-sense** range (§9.4/§10.4 **[START]**): a door that opens or
/// shuts away from the player — a guard walking through one, or an automatic door
/// timing shut (§10.4) — is felt at this Chebyshev range, **through walls**, and
/// leaves a fading on-grid cue in the [`Category::Sensed`](crate::Category::Sensed)
/// channel — the same one as a guard felt through a wall. It is a
/// louder, coarser event than a guard's exact position, so it carries **farther**
/// than the guard sense: `DOOR_SENSE_RANGE > `[`PLAYER_SENSE_RANGE`] (pinned by a
/// test). Doors are the facility breathing around you — you feel that from across a
/// wing even when you could not pinpoint the guard that did it. It is a `[START]`
/// tuning number: large enough to reach the next wing, small enough that the whole
/// facility does not pulse on every guard step (§2.3 in the other direction).
///
/// **Waiting does not widen it** (unlike the guard sense): the Wait extension is
/// about taking careful stock of *precise* guard positions, whereas a door change is
/// already loud enough to feel regardless. **Inside a duct it shrinks** to
/// [`DUCT_SENSE_RANGE`] with the rest of the crawlspace's degraded perception
/// (§10.7) — see [`door_sense_range`](State::door_sense_range).
pub const DOOR_SENSE_RANGE: u32 = 15;

/// The door sense reaches **farther** than the guard sense (§9.4/§10.4) — a coarse
/// "a door moved over there" carries past the precise "that guard is on that cell".
/// Pinned at compile time so the asymmetry can never silently invert.
const _: () = assert!(DOOR_SENSE_RANGE > PLAYER_SENSE_RANGE);

/// How many turns a door-change cue stays lit before it fades (§9.4/§10.4
/// **[START]**): a door open/close is a **discrete** event that will not restate
/// itself, so its cue is inherently a fading mark. It reads like sensed evidence —
/// visible while the fact is fresh — rather than a single-frame flash. Placed at full
/// life the turn the door changes and decremented once per world turn, so the cue
/// shows for this many renders and is gone on the next. Pinned by a test.
///
/// It is the **longer** of the sense channel's two lives (§9.4/#192): the guard cue
/// beside it ([`GUARD_CUE_DECAY_TURNS`]) is re-stamped every turn its guard is still
/// felt, so it needs to carry only the tail behind a live position.
pub const DOOR_CUE_DECAY_TURNS: u32 = 3;

/// How many turns a **guard** cue stays lit before it fades (§9.2/§9.4 **[START]**,
/// #192): the sense stamps the cell of every guard it feels through a wall, once per
/// world turn, and the stamp fades over this many turns. It is the other half of the
/// one persist-and-fade channel [`DOOR_CUE_DECAY_TURNS`] governs, and the tuning story
/// is shared: what varies between them is only how long each fact stays worth showing.
///
/// **Two**, and deliberately the shortest life in the channel. Its job is a **trail**
/// short enough to say *was just here* and no longer — a mark that outlived that would
/// be a readable heading, and §9.2 gives position, never intent. Two turns of tail
/// behind the live dot is enough to see a guard is on the move (and, when it leaves the
/// box, to leave a ghost of the last cell it was felt in) without handing the player a
/// vector to extrapolate. Raising it is the one knob that can turn the trail into an
/// arrow, so it is pinned by a test.
pub const GUARD_CUE_DECAY_TURNS: u32 = 2;

/// The guard trail stays **no longer** than the door cue (§9.4/#192) — the tail behind
/// a position the sense re-states every turn cannot outlive the discrete event that
/// gets one chance to be read. Pinned at compile time so the pair cannot silently
/// invert into a heading-length trail.
const _: () = assert!(GUARD_CUE_DECAY_TURNS <= DOOR_CUE_DECAY_TURNS);

/// The **Confusion** blast radius (§8.3/§9/#240/#325 **[START]**): the ability fires
/// once, and every guard standing within this Chebyshev box of the player *at that
/// moment* is dazed — measured the same way as the guard sense (§6.1 box metric) and,
/// like it, reaching **through walls** (§9).
///
/// It stays **smaller** than [`PLAYER_SENSE_RANGE`] (asserted below) so the blast can
/// never reach a guard the player cannot already sense — but it covers a guard's whole
/// *certain* zone (`CERTAIN_RANGE` = 5, §7.6), so the guards actually bearing down on
/// you are the ones it catches. Raised from the first pass's 4, where the blast was
/// tight enough that a chaser could sit just outside it. Pinned by a test so a later
/// change is a visible edit.
///
/// This is the **cap**, not the answer: the reach actually fired is
/// [`confusion_blast`](State::confusion_blast), which clamps it down to the live
/// [`sense_range`](State::sense_range) so the blast can never outreach what the player
/// perceives — inside a duct, that is [`DUCT_SENSE_RANGE`].
///
/// The clamp is stated over the **range**, and that is what makes it survive a run whose
/// sense is switched off entirely (§12.6/#493): that modifier suppresses the perceived
/// channel and leaves this ladder alone, so the blast keeps its reach and the clamp keeps
/// its wording. A modifier that had zeroed the range would have zeroed the ability.
pub const CONFUSION_RADIUS: u32 = 6;

/// The **Lockdown** seal radius (§8.3/§10.4/#242 **[START]**): activating Lockdown
/// shuts and seals every door with a cell inside this Chebyshev box of the player —
/// measured the same way as the guard sense and Confusion's bubble (§6.1 box metric)
/// and, like them, reaching **through walls**.
///
/// Deliberately **small**, and smaller than [`CONFUSION_RADIUS`]: the doors it takes
/// are the ones in the room you are standing in and the corridor you just left, not a
/// whole wing. A radius that sealed a wing would freeze the level's traffic for its
/// whole window — an ability that plays the map for you — where this one buys a
/// pursuer's detour and nothing more (§7.6). It is the ability's main power lever, so
/// it is pinned by a test and expected to move in playtest.
pub const LOCKDOWN_RADIUS: u32 = 4;

/// Lockdown's reach stays **within the guard sense** (§9/§8.3), for the same reason
/// Confusion's does: a door sealed beyond the range the player can perceive would be a
/// wall they had no way to read. Pinned at compile time.
const _: () = assert!(LOCKDOWN_RADIUS <= PLAYER_SENSE_RANGE);

/// The **Repel** field radius (§7.6/§8.3/#554 **[START]**): activating Repel stamps the
/// Chebyshev box of this radius around the cell it fired on as ground no guard may step
/// into — measured the same way as the guard sense, Confusion's blast and Lockdown's
/// seal (§6.1 box metric) and, like them, reaching **through walls**.
///
/// **Three, and smaller than every other area in the catalogue**, which is the whole of
/// what keeps a wall you can put down anywhere from being the wall that ends the game. A
/// 7×7 disc is a hub room's middle or the width of a corridor and its two lanes: enough
/// that a pursuit has to go round it, nowhere near enough to seal a wing. Every cell of
/// radius past that grows the area quadratically, and past about five the field stops
/// being a detour and starts being a region of the map deleted for eight turns — the
/// failure §8.3 names for Confusion (*"a no-guard-may-act field you carry"*) arriving by
/// the other door.
///
/// **Deliberately not clamped to the guard sense**, unlike Confusion's blast: this is
/// terrain rather than perception, and a cell a guard will not walk into is still a cell
/// it will not walk into when the player cannot see it — Lockdown's answer
/// ([`lockdown_area`](State::lockdown_area)) rather than
/// [`confusion_blast`](State::confusion_blast)'s. The assertion below is what makes that
/// harmless in practice: at three the whole field fits inside even a crawler's degraded
/// sense (§10.7), so there is no reach here for a clamp to take away.
///
/// It is the ability's main power lever, so it is pinned by a test and expected to move
/// in playtest (§13.2) — and it is the first lever to reach for if Repel and Lockdown
/// prove to be the same press (§8.3/#554).
pub const REPEL_RADIUS: u32 = 3;

/// The field fits inside the **narrowest** sense the game has (§9/§10.7/#554), not merely
/// inside the open-floor one — so a player who fires it from a duct can perceive every
/// cell of what they just stamped. Pinned at compile time, and it is what lets the radius
/// go unclamped where Confusion's cannot.
const _: () = assert!(REPEL_RADIUS <= DUCT_SENSE_RANGE);

/// How often the **Guide's** bearing shows (§8.3/#505 **[START]**): the compass lights
/// on one turn in this many and is dark on the rest — a needle that **pulses** rather
/// than one that stands.
///
/// **Three, and it is the ability's main balance lever.** A standing bearing is a line
/// you can simply follow: glance down, walk, glance down, walk, and §11.5a's fog stops
/// being something you plan around at all. Pulsing turns the same information into
/// something you have to *hold* — you get a fix, and then you walk on your own memory of
/// it for two turns, which is what a compass actually feels like to use and what leaves
/// the exploration §11.5a exists to create still standing. It is also what keeps a
/// permanent new user of the `Effect` cyan from sitting a cell from the player's eye for
/// the whole run (`docs/render-reference.md` §5).
///
/// **Turn zero is dark**, which falls out of the same rule rather than being a case
/// beside it: the run opens with no fix, so the first thing the ability asks of you is
/// to spend a few turns before it answers. A compass handed to you already pointing on
/// the frame you arrive would make the opening move free.
pub const GUIDE_BLINK_TURNS: u32 = 3;

/// The **False Call** reach (§7.7/§8.3/#504 **[START]**): the spoofer's broadcast, as a
/// Chebyshev box around the player — the same §6.1 box metric the guard sense,
/// Confusion's blast and Lockdown's seal are measured in, and like them reaching
/// **through walls**.
///
/// **The widest of the three, and that is the ability.** Confusion and Lockdown act on
/// the ground around you, so a wide one would play the map for you; this one moves
/// guards *off* ground you are about to leave, and a reach that only emptied the room
/// you stand in would empty nothing worth emptying. Ten is a wing's worth of corridor on
/// a 40×40 board (§10.2) without being the building — and it is near the ceiling of what
/// the board can take: a 21×21 box already covers a quarter of the floor, and the reach
/// above that stops being *a wing* and becomes *the facility* (§13.2 measured a 14 that
/// summoned most of the level onto the player).
///
/// **Unclamped by the guard sense**, unlike Confusion's blast — it is a radio, and
/// eyesight is not what a transmitter is measured in
/// ([`false_call_area`](State::false_call_area) has the argument). So it reaches a
/// little past what the player can perceive on open floor, and a **crawling player
/// broadcasts in full**: a duct degrades perception (§10.7) and a transmitter does not
/// perceive.
///
/// The radius is the **transmitter's**, never control's own net: §7.7's "no radio
/// range" stays true of every call the facility makes
/// ([`send_call`](State::send_call) gains no reach).
///
/// It is one of the two levers this ability is tuned on — the other is its lockout —
/// so it is pinned by a test and expected to move in playtest (§13.2). It is also where
/// the ticket's second risk lives: pulling a wing off its patrol on a 30-turn cooldown
/// could delete the §7.7 pressure the design says the difficulty comes from, and this
/// pair is what the sim sweeps first.
pub const FALSE_CALL_RADIUS: u32 = 10;

/// The broadcast reaches **at least as far as the guard sense** (§9/§8.3/#504) — the
/// opposite of what Confusion's and Lockdown's assertions hold, and pinned here so a
/// later tune that quietly brought it back inside eyesight would have to say so.
///
/// It matters for what the ability is *allowed to tell you*. Because the reach can
/// cover guards the player cannot perceive — always in a duct (§10.7), and anywhere the
/// §5 cone does not look — a firing must never report what it found, or the ability
/// would be a detector wearing a transmitter's name. That is why the press is never
/// refused for an empty reach and why the near line carries no count.
const _: () = assert!(FALSE_CALL_RADIUS >= PLAYER_SENSE_RANGE);

/// And it must stay **inside the board** (§10.2): a reach wider than the facility is a
/// number that no longer describes anything, since every firing would call every guard
/// alive and the radius would stop being a lever at all.
const _: () = assert!(FALSE_CALL_RADIUS * 2 < crate::LevelConfig::V1.height);

/// How far a **Dart** flies (§7.2/§8.3/#239 **[START]**): the dart travels at most this
/// many cells from the player along the cardinal they face, stopping at the first solid or
/// the first guard.
///
/// **A count of cells along a line, not a box.** Every other reach here is a §6.1
/// Chebyshev radius because every other effect acts on an *area*; this one walks a
/// cardinal ray, so its range is simply how many steps it takes. The two metrics agree on
/// a cardinal anyway — the eighth cell due north is `sight_distance` 8 — so nothing here
/// introduces a second notion of distance (§6.1).
///
/// **Eight, and it is the lever that changes the *play* rather than the frequency.** The
/// use budget decides how often a facility allows a ranged takedown; this decides what
/// kind of shot exists at all.
///
/// **It is deliberately shorter than the §5 sight range of 15** (asserted below), so
/// *seeing* a guard is never the same thing as being able to shoot it — the walk up the
/// corridor to get inside eight is the exposure the ability is paid for with. That gap is
/// load-bearing twice: it is also half of why an **open-floor** dart can never land
/// somewhere the player cannot see, which is what makes the clamp in
/// [`dart_shot`](State::dart_shot) inert outside a crawlspace (that function has the
/// argument, and the crawlspace counter-example the clamp exists for).
///
/// **It stays inside [`PLAYER_SENSE_RANGE`]** too (asserted below), so the clamp never
/// shortens an open-floor shot: `min(8, 10)` is 8.
///
/// **The corridor case is the risk the ticket names**, and the generator already answers
/// most of it: §10.1a stamps partial cover into every over-long straight run, and a table
/// is solid, so a long clear firing line is exactly the geometry the sightline rule
/// forbids the level to be *born* with. That is the argument, not a guarantee — if the
/// end-of-corridor shot still plays as a reliable free kill, this is the number to cut.
pub const DART_RANGE: u32 = 8;

/// The dart flies **less far than the player sees** (§5/§6.1/#239) — pinned at compile
/// time, because [`dart_shot`](State::dart_shot)'s whole safety argument rests on it: a
/// range that reached past the §5 cone would put cells in flight that the fog could be
/// hiding, and then the flight's wash would start reporting them.
const _: () = assert!(DART_RANGE < crate::PLAYER_SIGHT_RANGE);

/// And **inside the guard sense** (§9/#239), so the [`dart_shot`](State::dart_shot) clamp
/// is a crawlspace rule rather than a standing nerf: on open floor `min(8, 10)` leaves the
/// shot exactly as long as this constant says.
const _: () = assert!(DART_RANGE <= PLAYER_SENSE_RANGE);

/// How long a guard caught by a Confusion blast stays **dazed** (§8.3/#325
/// **[START]**): blinded and frozen for this many turns, counted down on the guard's
/// own clock ([`Guard::shake_off_daze`](crate::Guard)) rather than on the player's.
///
/// Six, on §8.2's convention — the firing turn is the first of them, every phase of
/// which already saw the guard frozen, so N means N. It is deliberately the same
/// number as the six-turn *window* the fired model replaced: what changed is when the
/// set of guards is decided and where the timer lives, not how much time the panic-buy
/// buys, so the retune (if the sim wants one, §13.2) stays a separate, visible edit.
/// Pinned by a test.
pub const CONFUSION_DAZE_TURNS: u32 = 6;

/// How many turns a **momentary** effect mark stays painted (§11.5 **[START]**,
/// #308/#338): the cyan wash that reports an effect which *is* a moment — Confusion's
/// box, the cell a bore opened, and every later fixed-cell effect. **One turn** — a
/// true flash, the acting frame and nothing after it. It exists to answer *what just
/// happened, and where* once, at the moment the player asks it, and a 13×13 field of
/// background is a great deal of ink to leave on the board while the danger overlay is
/// the thing that matters (§11.5 [SETTLED]).
///
/// What carries a *state* for longer is a **standing** mark instead (a guard still
/// frozen, later a live decoy or concealment in force), which costs no ink beyond the
/// cell it rides. That division is why one turn is the right life here: a momentary
/// mark reports **an event** — where it landed and how far it went — while *what is
/// still held* is a standing fact the other lifetime says better, and says truthfully
/// for a guard that has since walked out of the box (§8.3/#325).
///
/// Lit at full life the turn the effect acts and decremented once per spent turn, so
/// the mark shows for this many renders and is gone on the next — the same
/// persist-and-fade shape as [`DOOR_CUE_DECAY_TURNS`], which is why raising it is a
/// one-number change if playtest wants the boundary visible for longer. Pinned by a
/// test.
pub const EFFECT_FLASH_TURNS: u32 = 1;

/// Confusion's blast stays **within the guard sense** (§9/#240): a guard is never
/// frozen before the player can even sense its dot, so the effect is always legible
/// on the map. Pinned at compile time so the two ranges can never silently invert.
///
/// This is the **open-floor** half of the promise, and only that. Where the sense
/// itself shrinks below the cap — inside a duct, §10.7 — what keeps the promise is the
/// clamp in [`confusion_blast`](State::confusion_blast). The two are deliberately not
/// interchangeable: this one states a fact about the catalogue's numbers at compile time,
/// the clamp states the rule at every firing, and neither stands in for the other.
const _: () = assert!(CONFUSION_RADIUS <= PLAYER_SENSE_RANGE);

/// The flat part of the **stun** the safety eject costs (§8.3 **[START]**, #329) —
/// the turns owed for being thrown *at all*, on top of one turn per cell thrown
/// ([`phase_eject_stun`]).
///
/// While the counter runs, every [`Input`] resolves as a *stunned pass* — the turn is
/// spent, the world phases run, and the player changes nothing (see
/// [`player_phase`](State::player_phase)). It is a real price in a patrolled facility:
/// capture is contact (§4.5 **[SETTLED]**), so standing still on a cell the RNG picked
/// can end the run just as the wall used to — only now by a guard the player could see
/// coming. Pinned by a test so a later change is a deliberate, visible edit.
pub const PHASE_EJECT_STUN_BASE: u32 = 1;

/// How long the safety eject leaves the player **stunned** (§8.3 **[START]**, #329):
/// [`PHASE_EJECT_STUN_BASE`] plus **one turn per cell thrown** — the §6.1 box distance
/// from the solid they were stuck in to where they landed.
///
/// The length scales with the throw because that is what *prices recklessness*. A
/// phase that ends clipping the corner of a table is one cell out and costs the
/// smallest stun there is; burying yourself in the middle of a thick wall block is
/// three cells out and costs three times as much helplessness to undo. A flat rate
/// charged both the same, which let the worst case — deep inside a structure, far from
/// anywhere to stand — be as cheap as the near miss. The distance is the one the eject
/// search already found ([`eject_from_solid`](State::eject_from_solid)), so the price
/// cannot disagree with the throw that set it.
///
/// The flat base is what stops the cheapest case being free: you are on the floor
/// however short the trip was.
pub fn phase_eject_stun(cells_thrown: u32) -> u32 {
    PHASE_EJECT_STUN_BASE + cells_thrown
}
