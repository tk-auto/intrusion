//! The turn loop and the running game state (§4.2, §4.4, §4.5, §12.1).
//!
//! This is the heartbeat: `state × input → state, events`. [`State::step`] resolves
//! one turn in the fixed three-phase order (§4.2) — **player, sight, guards** — and
//! returns the events it produced. The core is pure and deterministic (§12.1): the
//! same state and the same input always yield the same next state and the same event
//! stream, which is what makes a run a `(seed, [inputs])` replay (§12.4).
//!
//! Three rules the loop is built around:
//!
//! - **Turn cost (§4.4), the rule that matters most.** *Every action that changes the
//!   world costs the turn.* A move, a bump that opens a door, taking the intel — all
//!   advance the turn, which is what lets the guards act. The exceptions are few and
//!   enumerated: moving into a wall is **free** (it's a mis-input, not a decision),
//!   and toggling an ability off is free (§4.4/#304). A free action does not
//!   end the turn, so the world does not move and the guards do not get a go.
//! - **Win and lose (§4.5), the only two.** *Lose:* a guard moving into your cell
//!   captures you — contact, not detection, so being unseen is not being safe. *Win:*
//!   take enough intel for the run's gate ([`exit_ready`](State::exit_ready), §4.5/
//!   §12.6 — quick play wants all of it, the sim one), then return to the exit you
//!   came in by; bumping it short of the gate refuses. There is no health, no combat.
//! - **The startup turn (§4.2).** One full turn runs at level start, before the first
//!   input, so guards have position and sight established when the player first acts.
//!
//! **Sight is real** (§6): phase 2 recomputes every viewer's field of view — the
//! player's ~180° half-disc (360° on a turn spent waiting, §8.3) and each guard's
//! ~90° wedge — from its *current* position and facing, which is what designs out the
//! old one-turn sensory lag (§4.2). **Guards patrol** (§7.5): phase 3 runs each
//! guard's `decide` step, which reads the sight this loop just recomputed and, for a
//! Calm guard, sweeps its territory toward the farthest cell it has not yet looked at.
//! Guards detect on **vision alone** (§9 **[SETTLED]** — there is no sound, no
//! hearing): a player in a guard's cone flips it to Chasing or Investigating (§7.6),
//! and a lost lead searches its area, watches, and only then stands back down. The
//! whole §7.4 state machine is built — the reactive states, the bounded search, the
//! radio dispatch (§7.3), the decoy — and every one of them sets a guard's destination
//! the same way and reuses the same walk-toward-it movement.
//!
//! # Where the loop lives
//!
//! This file owns the [`State`] itself, [`step`](State::step), the player phase and
//! the sight phase. The rest is next door, each in its own module: the tuning
//! catalogue ([`tuning`]), the §4.3 interaction ladder the player phase resolves
//! ([`bump`]), the public [`Input`]/[`Event`]/[`Affordance`] vocabulary
//! ([`events`]), the read surface the renderer and the §13.2 bot ask ([`view`]),
//! phase 3 with its knobs and the §7.3 radio net ([`guards`]), the doors' own turn
//! ([`doors`]), the §9 sense channel ([`sense`]), the ability machinery
//! ([`abilities`], [`activation`], [`effects`], [`lockdown`], [`bore`], the
//! piloting seam [`control`]), the §7.3 reinforcements ([`reinforcements`]) and the
//! #57 auto-slide ([`traversal`]). They are all `impl State` blocks over the *same*
//! struct — plain structs, not an ECS (§12.3), so the coupling stays visible in the
//! types.

use serde::{Deserialize, Serialize};

use crate::ability::{AbilityId, AbilityState, AbilityStatus, Behaviour, Deck, Effect, Loadout};
use crate::alert::{Alert, AlertReadout, AlertTrigger, AlertTuning};
use crate::body::Body;

use crate::category::Category;
use crate::cell::{Cell, Direction};
use crate::control::{transfers_control, Remote};
use crate::cover;
use crate::duct::Duct;
use crate::exchange::{Choice, Exchange};
use crate::facility::{Facility, Terrain};
use crate::generate::Layout;
use crate::guard::{
    Dwell, Guard, GuardState, PatrolStyle, Plan, GUARD_CLOSE_CHANCE_PERCENT,
    GUARD_DWELL_CHANCE_PERCENT,
};
use crate::level_seed::LevelSeed;
use crate::modifiers::{DebugModifiers, LevelModifiers};
use crate::radio;
use crate::region::{DoorCell, DoorId, RegionId, RegionKind};
use crate::rng::Rng;
use crate::score::par_for;
use crate::status::MessageHistory;
use crate::verdict::{Ending, RunStats, Verdict};
use crate::vision::{
    field_of_view_with_peek, GuardSight, VisibleSet, ENHANCED_SIGHT_RANGE, FULL_SIGHT_ARC,
    PLAYER_SIGHT_ARC, PLAYER_SIGHT_RANGE,
};
use crate::DoorAction;

mod abilities;
mod activation;
mod bore;
mod bump;
mod control;
mod dart;
mod doors;
mod effects;
mod events;
mod guards;
mod lockdown;
mod reinforcements;
mod sense;
mod traversal;
mod tuning;
mod view;

pub use bore::BoreRefusal;
pub use dart::DartShot;
pub use effects::EffectArea;
pub use events::{Affordance, Event, Input};
pub(crate) use reinforcements::{RUNG_THREE_REINFORCEMENTS, RUNG_TWO_REINFORCEMENTS};
pub use sense::SenseMark;
pub use tuning::*;

use activation::Aimed;
use bump::BumpKind;
use effects::EffectMark;
use sense::{SenseCue, SenseSource};

/// How the player perceives a guard this frame (§9.2) — the two states of a
/// perceived guard, and the gap between them is the whole §9 design. A guard the
/// player perceives at all is in exactly one; a guard in neither is out of reach and
/// [`perceive_guard`](State::perceive_guard) returns `None`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GuardPerception {
    /// In the player's field of view, line of sight clear (§6): the full threat —
    /// glyph in its state colour, facing, vision cone, and the danger overlay (§11.5).
    Seen,
    /// Within the guard-sense box ([`PLAYER_SENSE_RANGE`]), through walls, but **not**
    /// in the player's FOV: a bare position marker — the exact cell, nothing about
    /// where it is looking (§9.2). Never carries a danger overlay.
    Sensed,
}

/// Whether the run is still going, and if not, how it ended (§4.5).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Outcome {
    /// The run is live; the player may act.
    Playing,
    /// Objectives taken and the exit reached.
    Won,
    /// A guard walked into the player.
    Lost,
}

/// One objective: an intel console and whether it has been taken. How many of them
/// the exit demands is the run's gate ([`exit_ready`](State::exit_ready), §4.5/§12.6),
/// not a fixed rule — all of them in quick play, one in the sim, none in campaign.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Objective {
    cell: Cell,
    taken: bool,
}

/// One of the facility's **equipment caches** (§2.2/§8.3/§14 v3/#209): where the crate
/// stands, what is in it, and whether it has been opened.
///
/// **What it holds is decided before the level boots**, by
/// [`cache_contents`](crate::cache_contents) over the facility seed alone — so a facility
/// handed to somebody else holds the same crates, and a run that already carries this
/// tech is simply out of luck (#209). Once taken the crate stays on the state as spent
/// scenery: the renderer recolours it Neutral like a used console (§11.2), and what it
/// gave is on the loadout, not here.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Cache {
    cell: Cell,
    holds: AbilityId,
    taken: bool,
}

/// The running game: the world, the actors on it, the objectives, and the outcome.
///
/// Plain structs, not an ECS (§12.3). The level owns its layout, its player, and its
/// guards directly, so the coupling between them is visible in the types.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    layout: Layout,
    player: Cell,
    facing: Direction,
    /// The player's field of view, recomputed every sight phase (§4.2/§6).
    player_fov: VisibleSet,
    /// Tile memory (§11.5a): the running union of every FOV the player has ever
    /// had, absorbed each sight phase. Monotonic — a cell once seen stays seen for
    /// the whole run — and deterministic, since it is derived purely from the FOV
    /// sequence. The fog renderer reads it to decide which *contents* are
    /// remembered; live state never consults it (§11.5a keeps those apart).
    memory: VisibleSet,
    /// Which duct the player is **inside**, as an index into [`Layout::ducts`], or
    /// `None` on open floor (§10.7). This *is* the "in a duct" state — it must be
    /// stored, not derived from position, because a duct's interior may now cross
    /// room and corridor **floor** (§10.7 cross-room routing): those interior cells
    /// are ordinary walkable floor to a player who is not crawling, so the cell alone
    /// can no longer tell "walking the room" from "crawling the duct over it". Set
    /// when the player bumps a mouth to climb in ([`EnterDuct`](BumpKind::EnterDuct))
    /// and cleared when they step out ([`Move`](BumpKind::Move)); derived purely from
    /// those transitions, so it stays deterministic (§12.4).
    in_duct: Option<usize>,
    /// Whether the player has **set foot in the facility** this run (§4.5/#466) — false
    /// only during the opening crawl, before they first climb out of their own tunnel.
    ///
    /// It gates one thing: whether the §11.4 usable line offers the **way out**. On the
    /// border cell the row would otherwise open every run by pointing at the way home,
    /// which is the one thing turn one is not about — you have not been in yet. The gate
    /// is the *predictor's* alone, exactly like the FOV gate beside it: the bump itself
    /// still answers (§4.5's refusal), so a player who presses outward anyway is told
    /// why, and nothing the row does say has changed.
    ///
    /// Set the moment a step lands the player on facility floor
    /// ([`walk_into`](Self::walk_into) — a crawl is not one), and true from the start for
    /// a state that begins outside a duct, which is every hand-built fixture.
    entered_the_facility: bool,
    /// Whether the last **spent** turn was a Wait — which widens the next sight
    /// computation to the full 360° (§8.3). A free action (a wall bump) spends
    /// nothing and changes nothing (§4.4), so it does not clear this.
    waited: bool,
    /// Whether the player's cell changed on the last spent turn — the fact
    /// Camouflage reads (§8.3: undetectable **while you don't move**; the turn
    /// you move, you are revealed). Derived in [`step`](Self::step) from the
    /// position itself, never per-arm bookkeeping, so a sprint's extra cell, a
    /// hideout entry, or a stationary interaction (a bump, a grab, a wait) can
    /// never be misclassified. Free actions leave it alone (§4.4).
    moved_this_turn: bool,
    /// Turns of **stun** left (§8.3/#329): how many more inputs are swallowed as
    /// stunned passes after Dephase threw the player clear of a solid. Set to
    /// [`phase_eject_stun`] of the throw's length by the eject, and decremented once
    /// per spent turn,
    /// at the same end-of-turn beat as the ability clocks — so the eject's own turn
    /// (already spent by the action that ran the duration out) is not one of them and
    /// the player loses exactly that many turns of agency. Zero is the ordinary
    /// state: nothing else in the game writes it.
    stunned: u32,
    /// The table the player is crouched behind (§10.3), set by bumping it and
    /// cleared by any spent action other than a Wait (waiting holds the pose).
    /// Always orthogonally adjacent by construction: the bump that sets it is a
    /// bump into an adjacent cell, and every action that could move the player
    /// clears it.
    crouched_behind: Option<Cell>,
    /// The cupboard the player climbed into **this turn**, or `None` — the entry-turn
    /// signal the §15 Q5 witness check reads. Set in the [`BumpKind::Hide`] arm of
    /// phase 1 and consumed by [`guard_phase`](Self::guard_phase) the same turn; reset
    /// at the head of every [`step`](Self::step), so a guard only ever witnesses a dive
    /// on the turn it actually happens (a guard that later aims its cone at an occupied
    /// cupboard did not see the player go in, and never gains the flush, §10.3).
    entered_hideout: Option<Cell>,
    /// The door **the player's immediately preceding action opened from its hinge**
    /// (#320/#148), or `None`. It suppresses the very next frame bump on that door:
    /// bumping the hinge again would otherwise shut what you just opened, so while
    /// the mark stands the frame reads as a dead bump ([`BumpKind::HingeHeld`]) and
    /// is offered to the #57 lateral shift instead — you round the frame onto the
    /// open panel rather than undoing the open. Written only by phase 1's own door
    /// arm, so the world's doors (a guard's open, the #147 auto-close, the Autodoors
    /// step) never set it, and expired by
    /// [`expire_frame_bump_mark`](Self::expire_frame_bump_mark) at the end of every
    /// action, free or spent — the smallest window that fixes the catch and keeps
    /// "bump a hinge to close" one action away at all times. Derived purely from the
    /// input stream, so a replay reproduces it (§12.4).
    door_just_opened: Option<DoorId>,
    guards: Vec<Guard>,
    /// The bodies takedowns have left (§7.2) — solid entities the level owns
    /// (§12.3), each remembering its guard's post for the radio net (§7.3).
    /// Only ever appended, so an index into it is stable for the run.
    bodies: Vec<Body>,
    /// The body being dragged (§8.3), as an index into [`bodies`](Self::bodies)
    /// — `None` when the player's hands are free. Set by bumping a body (the
    /// grab, a spent turn); cleared by bumping it again (free, §4.4) or never.
    /// One body at a time.
    dragging: Option<usize>,
    /// The live decoy's cell (§8.3, #105), if one is out. At most one — the
    /// economy already guarantees it (duration 20 < cooldown 30, so a second
    /// activation can never overlap the first) — and its lifetime *is* the
    /// ability's active window, both ways: expiry and early toggle-off remove
    /// it, and being stepped on ends the ability into its full cooldown.
    decoy: Option<Cell>,
    /// The **remote unit** the player has put into the facility (§8.1/#273), if any —
    /// a drone today, whatever a later control-transfer ability deploys tomorrow. At
    /// most one: the abilities that place one are activated (§8.2), so a second could
    /// only overlap the first by way of an ability that is already running.
    ///
    /// Its life is its source ability's active window and nothing else
    /// ([`Remote::source`]) — there is no linger counter, because the linger *is* the
    /// rest of the duration (#273).
    remote: Option<Remote>,
    /// Whether the player's input drives the [`remote`](Self::remote) rather than their
    /// own body (§8.1/#273).
    ///
    /// Deliberately **not** derived from the ability's window: the whole shape of a
    /// control transfer is that letting go and the machine dying are different events,
    /// so "is it alive" and "am I holding the keys" are two facts and this is the
    /// second. Derived purely from the input stream — the press that takes the controls
    /// and the press that hands them back — so a replay reproduces it (§12.4).
    piloting: bool,
    /// The **half-speed convention** (§8.3: "you move at half speed while
    /// dragging"), documented here: a successful move while dragging leaves a
    /// haul debt, and the next spent turn pays it — a Step under debt is spent
    /// but stationary, and a Wait or an activation absorbs it too (resting
    /// counts; the §8.2 timing stays exact: N moves cost 2N spent turns at
    /// worst, and every turn is a real, guard-advancing turn). Free actions
    /// (§4.4) touch neither the debt nor anything else.
    drag_debt: bool,
    /// Per-ability economy runtime (§8.2): activation, duration, and cooldown for
    /// each activated ability, stepped by the turn loop. The v1 set is available
    /// from the start (§8.3/#104), so this begins all-ready.
    abilities: Deck,
    objectives: Vec<Objective>,
    /// The facility's comms console (§7.3/§7.7), or `None` for a facility that has
    /// none — a hand-built test state, which plays the unmodified radio game exactly
    /// as before. Derived once in [`new`](Self::new) from the stamped grid rather than
    /// taken as a parameter, so no boot path has to remember to pass it and the cell
    /// can never disagree with the terrain.
    comms_console: Option<Cell>,
    /// The facility's equipment caches (§2.2/§14 v3/#209), in placement order — empty on
    /// a facility that hides none, which is every quick-play level and every hand-built
    /// test state. Set at boot by [`with_caches`](Self::with_caches), which is where the
    /// crates' cells (the generator's) and their contents (the facility's stock) meet.
    caches: Vec<Cache>,
    /// The **exchange** a crate is offering right now (§8.3/#266), or `None` — which is
    /// almost always, since it takes a full run bumping a crate to open one.
    ///
    /// World state rather than view state, and that is the load-bearing part: while it
    /// is `Some` the turn loop answers nothing but [`Input::Discard`], so the facility
    /// is genuinely *waiting* on the player rather than a picture drawn over a game that
    /// carried on underneath (§12.1's split read the other way — this changes the world,
    /// so it is not the shell's to hold). It is cleared by the answer, either one.
    exchange: Option<Exchange>,
    /// Whether the radio net has been **killed** for the rest of the level (§7.3/§7.7)
    /// — set by bumping the comms console and never cleared. One-way and permanent by
    /// construction: nothing in the loop writes `false`, so there is no window in which
    /// the net could come back and no timer to read.
    radio_silenced: bool,
    /// Whether the player is carrying a guard's **key** (§10.4/#236) — what opens the
    /// prize room the locked-room modifier shuts. Every guard carries one, so any
    /// takedown (§7.2) sets it, and like [`radio_silenced`](Self::radio_silenced) it is
    /// one-way: nothing in the loop writes `false`. A key is a key; having taken it off
    /// a guard once, you do not go on needing another.
    ///
    /// It is a plain flag rather than an inventory because there is exactly one lock in
    /// the building and one key that opens it. A per-door key would be a search on top
    /// of a takedown, which is a second price for one rule (§2.3).
    holds_key: bool,
    exit: Cell,
    turn: u32,
    /// The facility-wide alert (§7.3): the [`Alert`] ladder's rung and the tallies
    /// its triggers count against. Every step has a concrete source — a confirmed
    /// sighting, a post that stopped answering the radio, a tampered console, a found
    /// body — and every step **does something**, which is the whole point after the
    /// old "never written to, never read" failure (§2.3). It never decays within a
    /// level (§7.3): a rung reached is a fact about the run.
    alert: Alert,
    /// Reinforcements an escalation has called for but the turn has not yet landed
    /// (§7.3/#374), one entry per guard, each holding the **trigger cell** it will
    /// walk to and search. Filled by [`raise_alert`](Self::raise_alert) wherever the
    /// rung rises and emptied by [`land_reinforcements`](Self::land_reinforcements) at
    /// the end of the same turn's world phases — never carried, so there is no backlog.
    ///
    /// The queue exists because phase 3 resolves its per-guard readings once, up front,
    /// indexed by position in [`guards`](Self::guards): growing that vector mid-phase
    /// would leave every later pass reading past the end of its own snapshot. See
    /// [`reinforcements`] for the whole argument.
    pending_reinforcements: Vec<Cell>,
    outcome: Outcome,
    /// **Why** the run ended (§14 v2/#138), latched from the terminal event the turn
    /// it fired — `None` while the run is live, and set exactly once, since the loop
    /// is inert afterwards ([`step`](Self::step)).
    ///
    /// It is latched rather than derived because the cause is only true at the
    /// instant of contact: the capturing guard's mood (§7.4) is a live reading that
    /// the finished board no longer holds, and a screen that re-derived it would be
    /// telling the player a mistake they did not make (§2.2's traceability promise).
    ending: Option<Ending>,
    /// Fresh detections this run (§7.6) — one per [`Event::Detected`], so a chase
    /// that holds the player in sight for ten turns counts once. The §13.2 sim counts
    /// the same event the same way; the end screen reads it from here (#138).
    detections: u32,
    /// Takedowns landed this run (§7.2) — one per [`Event::TakenDown`]. Counted
    /// rather than read off [`bodies`](Self::bodies), which a stow can hide and a
    /// drag can move: the run's ledger is what the player *did*, not what is still
    /// lying about.
    takedowns: u32,
    /// The events of the player's most recent action, free or spent — what the
    /// near line reads (§11.7: messages clear on the next action, so holding
    /// exactly one action's events *is* the clearing rule). Empty before the
    /// first input; frozen once the run ends, so the final message stays.
    last_events: Vec<Event>,
    /// What the near line said **before** the current action (§11.7/#300) — a bounded
    /// ring of the last few message-bearing actions, newest first, filled in
    /// [`step`](Self::step) as each action's events stop being live. The deployed log
    /// stacks it under the current block; the near line never reads it. Empty on a
    /// fresh [`State`], so a level starts with no memory of the last one (§12.6).
    message_history: MessageHistory,
    /// The live cues of the **sense channel** (§9/§9.4, #192): everything the player
    /// has felt through a wall recently enough to still show — the cell of each guard
    /// the sense stamped this turn and the turns just before it
    /// ([`record_guard_cues`](Self::record_guard_cues)), and every door that opened or
    /// shut away from them ([`record_door_cues`](Self::record_door_cues)). One store,
    /// because they are one channel: each entry fades on its own life in
    /// [`decay_sense_cues`](Self::decay_sense_cues), and the renderer reads the union —
    /// cell plus age — through [`sense_marks`](Self::sense_marks). A small set (a
    /// handful of doors, a short trail per sensed guard), so a plain `Vec` scan is
    /// cheaper than a map.
    sense_cues: Vec<SenseCue>,
    /// The §11.5 **effect layer**: every ability effect currently made visible as a
    /// background mark, one entry per (ability, placement) (#308/#338). Lit from the
    /// turn's events in [`record_effect_marks`](Self::record_effect_marks) with the very
    /// geometry the effect resolved against, aged on the one schedule in
    /// [`decay_effect_marks`](Self::decay_effect_marks) — momentary marks count down,
    /// standing ones end with the state they report — and dropped in
    /// [`clear_effect_marks`](Self::clear_effect_marks) the moment an effect *with* a
    /// window ends either way (§8.2 expiry, §4.4 toggle-off). A handful of entries at
    /// most, so a plain `Vec` scan beats a map.
    effect_marks: Vec<EffectMark>,
    /// Doors the **Autodoors** ability (§8.3/§7.6) opened in the player's path and
    /// still owes a close-behind, as [`DoorId`]s. A door is armed here the turn the
    /// player steps through it ([`BumpKind::AutoDoor`]) and swings shut — via the
    /// §10.4 crush-safe close ([`close_armed_autodoors`](Self::close_armed_autodoors))
    /// — on the first world turn its throat is clear of the player and any dragged
    /// body, then drops from the set. **Both** door kinds are armed: a manual door has
    /// no self-close, and an automatic one would otherwise dawdle open for its full
    /// `delay` (§10.4/#147) — too slow for the flight edge, so the ability shuts it
    /// promptly too. A small set — a player passes through one door at a time — so a
    /// plain `Vec` scan beats a map.
    autodoors_pending: Vec<DoorId>,
    /// The run's seeded random source (§12.4), carried through the turn loop for the
    /// two stochastic guard decisions: a Calm guard's chance to close a door behind
    /// itself (§10.4/#146) and its chance to dwell on reaching a patrol destination
    /// (§7.5/§153). It is the *continuation* of the generation
    /// stream — the same single seed the level was carved from — threaded in via
    /// [`with_rng`](Self::with_rng), never a fresh source (§12.4 rule 1). A state built
    /// without one keeps a fixed default stream, which is all a test that never
    /// exercises the close needs; the real game and the sim thread the run seed.
    rng: Rng,
    /// The percentage chance a Calm guard closes a hinged door behind itself
    /// (§10.4/§7.6), out of 100 — the playtest knob (§7.6 warns against always).
    /// Defaults to [`GUARD_CLOSE_CHANCE_PERCENT`]; `0` disables the close entirely
    /// (and draws no RNG, so it perturbs nothing), `100` always closes.
    close_chance: u32,
    /// The percentage chance a Calm guard dwells on reaching a patrol destination
    /// (§7.5 dwell, §153), out of 100 — the playtest knob paired with the takedown
    /// tickets. Defaults to [`GUARD_DWELL_CHANCE_PERCENT`]; `0` disables dwelling
    /// entirely (and draws no RNG, so it perturbs nothing), `100` always dwells.
    dwell_chance: u32,
    /// Whether the auto lateral-shift past an obstacle is on (§57/#57) — the
    /// runtime kill-switch for the traversal experiment. Defaults to
    /// [`AUTO_SLIDE_DEFAULT`](traversal::AUTO_SLIDE_DEFAULT) (on); `false` makes
    /// every dead bump the free §4.4 no-op again, so the feature can be disabled
    /// for a playtest — or wholesale — without touching the slide logic.
    auto_slide: bool,
    /// The level modifiers active for this facility (§12.6) — the one resolved
    /// value guards, vision, and render branch on, threaded in at boot by
    /// [`with_modifiers`](Self::with_modifiers). Defaults to the baseline (every
    /// modifier off), so a hand-built state and every current run play the
    /// unmodified game; a mode preset (#244) or a campaign source (#210) resolves
    /// a non-default set through [`ModifierSources`](crate::ModifierSources).
    modifiers: LevelModifiers,
    /// The **debug** modifiers this build was baked with (§12.6) — playtest-only
    /// switches over what the *player perceives*, threaded in by
    /// [`with_debug`](Self::with_debug) and read in the sight phase
    /// ([`recompute_sight`](Self::recompute_sight)). Deliberately *not* part of the
    /// [`LevelSeed`] above: no generation seam reads them and no shared token can
    /// carry them. Defaults to all off — the game as everybody else gets it.
    debug: DebugModifiers,
    /// Whether this run has **ever** had [`DebugModifiers::ghost`] on (§12.6/#507) —
    /// the latch that refuses its replay export.
    ///
    /// It latches on the **run**, not on the switch. Turning ghost back off does not
    /// restore the export: the inputs already recorded were played under bent rules,
    /// and no later toggle un-bends them. That mirrors the honesty the omni-vision row
    /// already shows about tile memory — turning it back off does not restore what was
    /// already seen, *which is honest rather than surprising: you did see it*.
    ///
    /// Set by [`with_debug`](Self::with_debug) and [`toggle_ghost`](Self::toggle_ghost),
    /// and never cleared. It is on the state rather than on the shell because it is a
    /// fact about the **run**, so it survives the autosave the same way the run does.
    ghosted: bool,
    /// The run's reproducible starting config (§12.4/#245), threaded in at boot by
    /// [`with_level`](Self::with_level) — the one handle that reproduces *this* run
    /// exactly, which the help panel shows (#272). `None` for a hand-built state,
    /// which was assembled cell by cell and has no seed to reproduce it from. It is
    /// the **starting** config and never changes with play: a loadout the run later
    /// grows by salvaging tech (§8.3) belongs to the run, not to the token that
    /// boots it.
    level: Option<LevelSeed>,
}

impl State {
    /// Assemble a level and run the startup turn (§4.2).
    ///
    /// The objective cells are stamped as intel consoles and the exit as the exit
    /// tile (§10.3) so the loop's bump interactions meet solid, distinctly-typed
    /// terrain. Real levels get this from placement (#12); a hand-built state does it
    /// here. `facing` is the player's initial facing (it changes only by moving, §5).
    ///
    /// One full turn — sight, then guards — runs before this returns, so the first
    /// [`step`](Self::step) already faces settled guards (§4.2).
    ///
    /// **The run opens inside the tunnel** (§4.5/§10.7/#466). When `player` is the
    /// way-out cell of the layout's exit duct — which is where placement puts it — the
    /// state starts *in* that duct, facing along it into the facility: concealed,
    /// contact-safe, perceiving only the shortened sense (§10.7), and a few crawl steps
    /// from the mouth at `E`. The first inputs of the run are that crawl, and the peek
    /// out of the mouth is what reads the room before the player climbs out.
    ///
    /// That replaced the **opening look** (#383), where the first frame was computed as
    /// if the previous turn had been a Wait — a free 360° arc and the widened sense, to
    /// show a player the room they had materialised in. Nobody materialises any more, so
    /// the exemption has nothing left to paper over: the opening posture is now an
    /// ordinary consequence of §10.7, and the player looks where they chose to look.
    pub fn new(
        mut layout: Layout,
        player: Cell,
        facing: Direction,
        guards: Vec<Guard>,
        objectives: impl IntoIterator<Item = Cell>,
        exit: Cell,
    ) -> Self {
        let objectives: Vec<Objective> = objectives
            .into_iter()
            .map(|cell| {
                layout.place(cell, Terrain::Console);
                Objective { cell, taken: false }
            })
            .collect();
        layout.place(exit, Terrain::Exit);
        // The comms console (§7.3/§7.7), stamped here with the other solid usables
        // rather than by the generator, so the carve stays bare for the §10.5/§10.6
        // work that runs on it. Placement recorded the cell on the layout, so no boot
        // path has to pass it in.
        if let Some(cell) = layout.comms_console() {
            layout.place(cell, Terrain::CommsConsole);
        }
        // The equipment caches (§2.2/§14 v3/#209), stamped here with the other solid
        // usables and for the same reason. Only the *crates* land here: what is in them
        // arrives with the stock ([`with_caches`](Self::with_caches)), and a state that
        // never gets one plays a facility whose crates are scenery — which is why the two
        // halves are not allowed to drift apart, and why `with_caches` pairs its list
        // against the layout's rather than being told the cells a second time.
        for &cell in layout.equipment_caches().to_vec().iter() {
            layout.place(cell, Terrain::EquipmentCache);
        }
        // One source of truth for the cell: the grid. Reading it back also picks up a
        // hand-built fixture that stamped its own console without a placement.
        let comms_console = layout.facility().find(Terrain::CommsConsole);

        // The run begins **inside the tunnel** (§4.5/§10.7/#466), on its way-out cell at
        // the border — the one spawn that is not on the floor. Derived from the layout
        // rather than passed in, so no boot path has to remember it, and narrow on
        // purpose: only the way-out cell starts a player crawling, never an interior
        // cell a duct's path merely overlies (which is ordinary floor to anyone walking
        // it, §10.7). A hand-built fixture with no exit duct starts on foot, as before.
        // (Only the exit tunnel has a way out at all, so matching on it is the whole
        // test — no need to ask which duct is which.)
        let in_duct = layout
            .ducts()
            .iter()
            .position(|duct| duct.way_out() == Some(player));
        // Facing **into the facility**: along the tunnel, the way the crawl runs. The
        // caller's `facing` is what a player standing on the floor was given, and there
        // is nothing to see behind you in a crawlspace anyway (§10.7).
        let facing = in_duct
            .and_then(|i| {
                let cells = layout.ducts()[i].cells();
                Direction::between(cells[cells.len() - 1], cells[cells.len() - 2])
            })
            .unwrap_or(facing);
        debug_assert!(
            layout
                .exit_duct()
                .is_none_or(|duct| duct.cells()[0] == exit),
            "the exit tunnel must start at the exit it is the way out of (§4.5)"
        );

        let mut state = Self {
            layout,
            player,
            facing,
            player_fov: VisibleSet::default(),
            memory: VisibleSet::default(),
            in_duct,
            // A run that opens in the tunnel has not been inside yet (§4.5/#466); every
            // other state — a hand-built fixture, a scene staged mid-run — begins on the
            // floor and so begins having been.
            entered_the_facility: in_duct.is_none(),
            // The run opens with the **ordinary** posture — §5's half-disc and the plain
            // sense box. The free 360° opening look (#383) is gone with #466: it existed
            // to show a player the room they had materialised standing in, and nobody
            // materialises any more. You crawl in, you peek out of the mouth (§6.1/§10.7)
            // and you climb out looking where you chose to look — which is a decision,
            // where the old rule handed out a fact.
            waited: false,
            moved_this_turn: false,
            stunned: 0,
            crouched_behind: None,
            entered_hideout: None,
            door_just_opened: None,
            guards,
            bodies: Vec::new(),
            dragging: None,
            decoy: None,
            remote: None,
            piloting: false,
            drag_debt: false,
            // A hand-built state holds the **innate** set and nothing else (§8.3):
            // loadouts are built up from empty, never carved down from everything,
            // so a state that uses salvaged tech says which tech it has
            // ([`with_loadout`](Self::with_loadout)) instead of inheriting the lot.
            // That matters most for a passive, which reshapes perception for the
            // whole run (§8.3/#265) — never something to acquire by default.
            abilities: Deck::new(Loadout::innate()),
            objectives,
            comms_console,
            // Filled by [`with_caches`](Self::with_caches): `State::new` stamps the
            // crates (below) but cannot say what is in them, because the stock is drawn
            // at boot and a bare state has none.
            caches: Vec::new(),
            exchange: None,
            radio_silenced: false,
            // Nobody starts a raid holding the building's keys (§10.4/#236).
            holds_key: false,
            exit,
            turn: 0,
            alert: Alert::new(),
            pending_reinforcements: Vec::new(),
            outcome: Outcome::Playing,
            ending: None,
            detections: 0,
            takedowns: 0,
            last_events: Vec::new(),
            message_history: MessageHistory::default(),
            sense_cues: Vec::new(),
            effect_marks: Vec::new(),
            autodoors_pending: Vec::new(),
            // A fixed default stream until [`with_rng`](Self::with_rng) threads the
            // run seed. The startup world phase below draws nothing — a guard cannot
            // have passed through a door before it has taken a step — so setting the
            // real stream after construction observes the identical stream position.
            rng: Rng::new(0),
            close_chance: GUARD_CLOSE_CHANCE_PERCENT,
            dwell_chance: GUARD_DWELL_CHANCE_PERCENT,
            auto_slide: traversal::AUTO_SLIDE_DEFAULT,
            modifiers: LevelModifiers::default(),
            debug: DebugModifiers::default(),
            ghosted: false,
            level: None,
        };
        // The level-start full turn (§4.2): sight and guards, no player phase.
        let _ = state.run_world_phases();
        state
    }

    /// Kill the radio net directly (§7.3/§7.7) — the seam a test uses to reach the
    /// flag without staging a comms-console bump, so a scene built for the §7.7
    /// call-ins can be replayed with the net dead and nothing else changed. The real
    /// game only ever sets it through the bump ([`BumpKind::SilenceRadio`]).
    #[cfg(test)]
    pub(crate) fn silence_radio_for_test(&mut self) {
        self.radio_silenced = true;
    }

    /// Thread the run's seeded random source into the state (§12.4) — the
    /// continuation of the very stream the level was generated from, so a whole run
    /// is one seed end to end. The loop uses it for the guard close-behind roll
    /// (§10.4/#146) and the patrol dwell roll (§7.5/§153); everything else in the
    /// loop stays deterministic without it. The
    /// real game and the headless sim call this; a test that never exercises the
    /// close can rely on the fixed default set in [`new`](Self::new).
    pub fn with_rng(mut self, rng: Rng) -> Self {
        self.rng = rng;
        self
    }

    /// Run the facility alert on `tuning` instead of the shipped §7.3 **[START]**
    /// thresholds (#376) — the seam the §13.2 sim sweeps a rung's difficulty through
    /// without a rebuild.
    ///
    /// Applied *after* [`new`](Self::new)'s startup turn, which is safe by
    /// construction: placement guarantees no guard's turn-one cone covers the spawn
    /// (§10.6), so the startup turn tallies no contact, and the tuning is written onto
    /// the ladder rather than replacing it — there is no tally for this to discard,
    /// and the sliding window is re-pruned against the live thresholds on the next
    /// turn regardless.
    ///
    /// Like [`with_debug`](Self::with_debug) this is **not** part of the
    /// [`LevelSeed`]: no shared token can carry it, so a swept run is an instrument
    /// reading rather than a game a player could be handed. A state built without it
    /// plays the shipped ladder, which is every real run.
    #[must_use]
    pub fn with_alert_tuning(mut self, tuning: AlertTuning) -> Self {
        self.alert.set_tuning(tuning);
        self
    }

    /// Thread the facility's resolved [`LevelModifiers`] into the state (§12.6) —
    /// the one value the guard search (§7.6) and the danger overlay (§11.5) read,
    /// resolved once at facility start from its sources
    /// ([`ModifierSources::resolve`](crate::ModifierSources::resolve)). A state
    /// built without this keeps the baseline (every modifier off), which is all a
    /// test of the unmodified game needs; the real game and the sim resolve and
    /// thread the set at boot.
    #[must_use]
    pub fn with_modifiers(mut self, modifiers: LevelModifiers) -> Self {
        self.modifiers = modifiers;
        // The startup turn (§4.2) has already run its sight phase by the time a
        // builder is called, so re-run it here — `calm_guards_detect_only_their_cone`
        // (#410) is a **sight** rule, and without this the opening frame would carry
        // turn-zero cones cast under the other arm: the danger overlay would lie
        // about the flank, and a first-turn flank takedown would be refused by a
        // stale reading. Sight is a pure recompute (no RNG, §12.4), so running it
        // twice costs a frame's work and changes nothing — the same reasoning, and
        // the same call, as [`with_debug`](Self::with_debug).
        self.recompute_sight();
        // And the §9 sense channel is stale in exactly the same way (#493). That same
        // startup turn stamped a cue on every guard it felt and on any door its guards
        // moved, under the *unsuppressed* rule — so without this the opening frame of a
        // run whose sense is off would carry a turn-zero dot the run can never produce
        // again. Dropping is the whole of the correction: the modifier can only ever take
        // marks away, so there is nothing to re-stamp, and every later turn records
        // through the seams that already read the resolved set.
        if self.modifiers.sense_suppressed {
            self.sense_cues.clear();
        }
        self
    }

    /// The level modifiers active for this facility (§12.6), for the systems that
    /// branch on them and for a test to assert what was resolved.
    #[must_use]
    pub fn modifiers(&self) -> LevelModifiers {
        self.modifiers
    }

    /// Thread this build's [`DebugModifiers`] into the state (§12.6) — the
    /// playtest-only perception switches, set by a baked build and by nothing else (a
    /// level-seed token cannot carry them). Separate from
    /// [`with_level`](Self::with_level) on purpose: the level is what the run *is*,
    /// and these are only how much of it whoever is watching gets to see.
    #[must_use]
    pub fn with_debug(mut self, debug: DebugModifiers) -> Self {
        self.debug = debug;
        // A run that *starts* under the ghost is latched from turn zero (§12.6/#507):
        // a build baked with the switch on bends the rules for the whole run, so there
        // was never a stretch of it that could honestly be handed on as a replay.
        self.ghosted |= debug.ghost;
        // The startup turn (§4.2) has already run its sight phase by the time a
        // builder is called, so re-run it here for the switches that shape sight —
        // otherwise the reveal would only take hold from the player's first action,
        // and the opening frame would still be fogged. Sight is a pure recompute (no
        // RNG, §12.4), so running it twice costs a frame's work and changes nothing.
        //
        // **The guards' own startup look is not re-run**, and the ghost therefore takes
        // hold from the first player turn rather than from turn zero. Re-running it
        // would step the guards twice, which is a worse bargain than the one thing it
        // buys — and it buys nothing in a real run: §10.6 guarantees a spawn no cone
        // eyes, so there is nothing for that look to have found.
        self.recompute_sight();
        self
    }

    /// The debug modifiers in force (§12.6) — the sight phase reads the reveal, and
    /// the §10.3 concealment seam reads the ghost (#507).
    #[must_use]
    pub fn debug(&self) -> DebugModifiers {
        self.debug
    }

    /// Whether this run has **ever** been played under the ghost (§12.6/#507) — the
    /// latch, once set never cleared, that refuses the replay export.
    ///
    /// The shell asks it before offering the export: a run whose inputs were played
    /// under bent rules would replay into a desync on the first turn a guard would have
    /// seen the player, and teaching the link to carry the switch would put a rule-bend
    /// inside a shareable URL — the exact thing §12.6 keeps out of the token. The
    /// containment is worth more than the export.
    ///
    /// It is a **latch and not a live read** of [`debug`](Self::debug): switching ghost
    /// back off does not restore the export, because the turns already recorded do not
    /// un-bend.
    #[must_use]
    pub fn ghosted(&self) -> bool {
        self.ghosted
    }

    /// Flip the **omni-vision** switch on a running game (§12.6/#459) — the live half
    /// of [`with_debug`](Self::with_debug), for the debug session's `omni [v]` control.
    ///
    /// A mid-run flip is safe for exactly the reason the baked switch is: it is applied
    /// in the sight phase and read by no rule, so the recompute it triggers changes
    /// what the player perceives and nothing else. Sight is pure (no RNG, §12.4), so
    /// this consumes nothing from the run's stream and the run replays identically
    /// whether it was flipped or not — the property the panel's control rests on, and
    /// the one the tests pin rather than assume.
    ///
    /// It costs no turn (§4.4): the world does not step, no guard moves, and the frame
    /// after is the frame before with more (or less) of it visible. What was seen while
    /// it was on stays **remembered** afterwards (§11.5a), because memory accumulates
    /// from sight like it does for any cell — switching it off is not a way to unsee.
    pub fn toggle_reveal(&mut self) {
        self.debug.reveal_whole_level = !self.debug.reveal_whole_level;
        self.recompute_sight();
    }

    /// Flip the debug session's **ghost** switch on a running game (§12.6/#507) — the
    /// live half of [`with_debug`](Self::with_debug), for the Options tab's `ghost` row.
    ///
    /// Unlike [`toggle_reveal`](Self::toggle_reveal) this one **touches the facility**,
    /// and the whole of what makes that containable is here: switching it on latches
    /// [`ghosted`](Self::ghosted) for the rest of the run, so the replay export is
    /// refused from this moment on and stays refused however many times it is toggled
    /// after.
    ///
    /// It costs no turn (§4.4) and consumes nothing from the run's stream (§12.4):
    /// concealment is derived per query, so there is nothing to recompute and the very
    /// next guard phase simply reads the player as concealed. Flipped on **mid-chase**,
    /// the pursuers lose sight through the ordinary §7.6 path — each searches where it
    /// last had you — rather than through any special case here.
    pub fn toggle_ghost(&mut self) {
        self.debug.ghost = !self.debug.ghost;
        self.ghosted |= self.debug.ghost;
    }

    /// Thread the run's whole [`LevelSeed`] into the state (§12.4/#245) — the boot
    /// path's one call, replacing separate
    /// [`with_modifiers`](Self::with_modifiers) /
    /// [`with_loadout`](Self::with_loadout) calls: it records the config *and*
    /// applies its two halves, so the token the help panel shows (#272) and the
    /// rules the run actually plays under cannot disagree. A hand-built state simply
    /// omits it and keeps the baseline.
    #[must_use]
    pub fn with_level(mut self, level: LevelSeed) -> Self {
        self.level = Some(level);
        self.with_modifiers(level.modifiers)
            .with_loadout(level.abilities)
    }

    /// The run's reproducible starting config (§12.4/#245), or `None` for a
    /// hand-built state. [`LevelSeed::encode`] turns it into the shareable
    /// level-seed token the help panel displays (#272).
    #[must_use]
    pub fn level(&self) -> Option<LevelSeed> {
        self.level
    }

    /// Thread the run's resolved ability **loadout** into the state (§8.3/#244) —
    /// the set of economy abilities the player actually holds. Quick play grants
    /// the innate set plus a seeded draw of tech, a campaign accumulates its set
    /// (§2.2); both resolve to a [`Loadout`] carried in the shareable level-seed
    /// string (#245). A state built without this keeps the full loadout (every
    /// ability), which is all a test of the whole set needs; the real game and the
    /// sim resolve and thread a preset's loadout at boot. Set before the first
    /// [`step`](Self::step): a fresh deck is all-ready, so this only chooses which
    /// abilities exist, never their live state.
    #[must_use]
    pub fn with_loadout(mut self, loadout: Loadout) -> Self {
        self.abilities = Deck::new(loadout);
        self
    }

    /// Thread in **what the facility's equipment caches hold** (§2.2/§8.3/§14 v3/#209):
    /// the pieces of salvaged tech a bump on each crate hands over, in the order the
    /// crates were placed.
    ///
    /// The cells come from the **layout** rather than from the caller, exactly as the
    /// comms console's does: [`new`](Self::new) stamped them from that same list, so the
    /// generator is the one source of truth for *where* and this decides only *what*. A
    /// crate with nothing paired against it is left as the scenery it is, and a stock
    /// longer than the crates it was drawn for is truncated — the two lists are zipped,
    /// so neither can promise something the other cannot show.
    ///
    /// Called by [`start_level`](crate::start_level) at boot. A hand-built fixture that
    /// stamped its own crate hands its stock the same way; there is no path that gives a
    /// crate contents without the grid having a crate on it.
    #[must_use]
    pub fn with_caches(mut self, stock: impl IntoIterator<Item = AbilityId>) -> Self {
        let cells: Vec<Cell> = if self.layout.equipment_caches().is_empty() {
            // A hand-built fixture stamps its crates and records nothing; read the grid
            // back so a fixture needs one line rather than a layout it has to build.
            self.layout.facility().find_all(Terrain::EquipmentCache)
        } else {
            self.layout.equipment_caches().to_vec()
        };
        self.caches = cells
            .into_iter()
            .zip(stock)
            .map(|(cell, holds)| Cache {
                cell,
                holds,
                taken: false,
            })
            .collect();
        self
    }

    /// Thread in the **pre-level scout** (§11.5a/§14 v3/#215): mark the facility's points
    /// of interest as already remembered, so it opens with them on the board.
    ///
    /// **It writes tile memory and nothing else**, which is the whole design. §11.5a
    /// already has a state for *"you know this is here and you cannot see it now"* — the
    /// remembered one — and the renderer already draws a console, a crate and a cupboard
    /// in it ([`render`](crate::render)). So a scouted facility needs no third knowledge
    /// state, no second fog rule and no flag the renderer has to consult: it needs the
    /// player to have *found* those cells, a turn early.
    ///
    /// What that buys is bounded by what memory is worth. **Live state never consults
    /// memory** (§11.5a), so a scout hands over no guard, no door pose and no danger
    /// cone; the room around a scouted console is as unexplored as it was, because the
    /// scout marks the console's own cell and not its surroundings. Handing over *where*
    /// and
    /// never *what is happening there* is not a restraint applied on top of this — it is
    /// what marking one cell can express.
    ///
    /// Called by [`start_level`](crate::start_level) with the resolved
    /// [`LevelModifiers::scouted`](crate::LevelModifiers::scouted), after the crates are
    /// stamped: memory is a fact about the finished board. A state built without it plays
    /// the unchanged §11.5a game, which is every quick-play level and every hand-built
    /// fixture.
    #[must_use]
    pub fn with_scouted(mut self, scouted: bool) -> Self {
        if !scouted {
            return self;
        }
        for cell in crate::scout::scouted_cells(self.layout.facility()) {
            self.memory.mark(cell);
        }
        self
    }

    /// The run's ability loadout (§8.3/#244) — the abilities it holds, for a test
    /// to assert what a preset resolved and for the level-seed token to carry.
    #[must_use]
    pub fn loadout(&self) -> Loadout {
        self.abilities.loadout()
    }

    /// Set the chance a Calm guard closes a hinged door behind itself, as a
    /// percentage 0–100 (§10.4/§7.6) — the playtest knob this behaviour's
    /// **[START]** value is tuned with, and the replacement for the old blanket
    /// auto-close switch. `0` turns the close off, `100` makes it certain; values
    /// above 100 saturate. Deterministic given the seed threaded by
    /// [`with_rng`](Self::with_rng).
    pub fn set_guard_close_chance(&mut self, percent: u32) {
        self.close_chance = percent.min(100);
    }

    /// Set the chance a Calm guard dwells on reaching a patrol destination, as a
    /// percentage 0–100 (§7.5 dwell, §153) — the playtest knob this behaviour's
    /// **[START]** value is tuned with, and the isolation switch a test that wants
    /// a fixed patrol cadence flips to `0`. `0` turns dwelling off (drawing no RNG,
    /// so the stream is untouched), `100` makes it certain; values above 100
    /// saturate. Deterministic given the seed threaded by [`with_rng`](Self::with_rng).
    pub fn set_guard_dwell_chance(&mut self, percent: u32) {
        self.dwell_chance = percent.min(100);
    }

    /// Turn the auto lateral-shift past an obstacle on or off (§57/#57) — the
    /// runtime kill-switch for the traversal experiment, on by default
    /// ([`AUTO_SLIDE_DEFAULT`](traversal::AUTO_SLIDE_DEFAULT)). `false` restores the
    /// plain §4.4 free bump on every dead-end step, so a playtest — or a later
    /// decision to drop the experiment — can disable it without a code change to the
    /// slide logic.
    pub fn set_auto_slide(&mut self, enabled: bool) {
        self.auto_slide = enabled;
    }

    /// Resolve one turn: player, then — only if the turn was actually spent — sight
    /// and guards (§4.2). Returns the events, in order.
    ///
    /// Once the run is over the loop is inert: a call on a finished [`State`] changes
    /// nothing and returns no events.
    pub fn step(&mut self, input: Input) -> Vec<Event> {
        if self.outcome != Outcome::Playing {
            return Vec::new();
        }
        // **An open exchange takes nothing but its answer** (§8.3/#266). While a crate is
        // offering, the only input the loop resolves is the discard: a step, a wait, an
        // activation each stop here, so the world cannot move on around a decision it is
        // waiting for.
        //
        // It returns **before** the phases *and before the message bookkeeping below*,
        // which is the part worth stating. A swallowed input is not a free action that
        // happened and reported nothing — it is an input that never reached the loop at
        // all, so it must not file the standing question into the log and replace it with
        // silence. Pressing an arrow at a crate used to wipe the very line telling the
        // player what they were being asked (§11.7).
        //
        // The rule lives **here**, in the core, and not in the shell that draws the row.
        // A shell can only make its own input path obey it; this makes every path obey
        // it — the browser, a replayed script and the §13.2 sim alike — so a run cannot
        // walk away from a half-answered crate in one of them and not the others (§12.4).
        // Non-trapping either way (§11.6): the decline is one of the four presses, so
        // there is always a way out of the offer.
        if self.exchange.is_some() && !matches!(input, Input::Discard(_)) {
            return Vec::new();
        }

        let mut events = Vec::new();
        // A fresh turn: no hideout has been climbed into yet. The §15 Q5 witness check
        // in phase 3 reads whatever the Hide bump sets below, this turn only.
        self.entered_hideout = None;
        // Phase 1. A free action (wall bump, refused exit) does not end the turn.
        let from = self.player;
        // The frame-bump mark (#320) has to survive *into* this action to be readable
        // at all — `bump_kind` consults it, and so does the §11.4 usable line between
        // turns — so it is expired at the far end of phase 1 rather than at the head
        // of the turn like `entered_hideout`. Held across the resolve, spent by it.
        let carried = self.door_just_opened;
        let spent = self.player_phase(input, &mut events);
        self.expire_frame_bump_mark(carried);

        if self.outcome == Outcome::Playing && spent {
            self.turn += 1;
            // Whether this spent turn moved the player — read straight off the
            // position, the fact Camouflage's stillness rule consumes (§8.3).
            self.moved_this_turn = self.player != from;
            // Phases 2 and 3 only happen because the player spent the turn (§4.2/§4.4).
            events.extend(self.run_world_phases());
            // Ability durations tick HERE — at end of turn, after all three phases —
            // so a freshly activated N-turn ability yields N protected turns and the
            // activation turn itself is covered (§8.2's N-yields-N−1 trap): the
            // activation ran in phase 1 and every phase this turn saw it active; only
            // now does its remaining count drop. Cooldowns, frozen through the
            // duration, drain here too, but only for now-inactive abilities. Only a
            // *spent* turn reaches this, so a free action never advances the clock.
            let mut expired = Vec::new();
            self.abilities.tick(&mut expired);
            // The guards' daze counters run on the same convention, in the same beat
            // (§8.2/#325): a guard caught by this turn's blast gets its full N turns,
            // the firing turn — every phase of which already saw it frozen — being the
            // first, and only now does its own count drop. The clock lives on the
            // guard rather than on the player, which is what makes distance stop
            // mattering once the flash has gone off.
            self.tick_guard_daze();
            let phase_ended = expired.iter().any(|&id| declares(id, Effect::Phase));
            for &ability in &expired {
                // One teardown, shared with the early ends (§8.2/#528): whatever the
                // window was doing to the world — the decoy, the remote and its
                // controls, the door seals, the marks — dies with it here, from the
                // same list a toggle-off or a trade walks.
                self.unwind_effect(ability, &mut events);
            }
            events.extend(
                expired
                    .into_iter()
                    .map(|ability| Event::AbilityExpired { ability }),
            );
            // Dephase expiring somewhere a solid body cannot stand throws the player
            // clear and leaves them stunned (§8.3/#329) — the cost that keeps phasing
            // from being free, paid in turns instead of in the run. It is *not* a
            // rescue: the landing cell is drawn at random from the nearest legal ones,
            // so a phase that ends in a wall never doubles as a reliable way through
            // one, and the stun that follows is spent helpless in a patrolled
            // facility. Skipped if the run already ended this turn (a capture is its
            // own, truthful loss).
            //
            // The stun's own clock is ticked first, beside the ability clocks: this
            // spent turn was a stunned pass if anything was owed, and paying it here
            // — *before* an eject can set a fresh count — keeps the turn the player is
            // thrown clear (already spent by the action that ran the duration out)
            // from ever being one of the turns they owe.
            self.stunned = self.stunned.saturating_sub(1);
            if phase_ended && self.outcome == Outcome::Playing && !self.can_rematerialize() {
                self.eject_from_solid(&mut events);
            }
            // Latch the marks of every effect that acted this turn (§11.5/#308/#338),
            // once, at the very end of it — *after* the fade at the head of the world
            // phases, exactly the door cues' shape (§9.4), so a flash lit this turn
            // keeps its full life instead of losing a turn to the very tick that placed
            // it, and after the last phase that can still produce an effect event.
            //
            // That last part is why this sits below the eject rather than beside the
            // world phases: the safety eject resolves after the ability clocks, so an
            // `Ejected` latched any earlier would be latched from a list it was not yet
            // in and never drawn at all (#339). Running here also means a mark from an
            // ability whose window ended this very turn survives `clear_effect_marks`
            // above — which is exactly right for the eject, an event whose whole
            // occasion *is* the window ending.
            self.record_effect_marks(&events);
        }

        // The run's ledger and its ending, read off this action's events (§14 v2/#138)
        // — one place, on the way out, so nothing that can end a run or break stealth
        // has to remember to keep score at its own site.
        self.record_verdict(&events);

        // Every action replaces the near line's source, free bumps included —
        // this assignment is §11.7's "messages clear on the next action".
        //
        // The outgoing set is filed first (#300): what the near line stops showing is
        // exactly what the deployed log remembers, so the two can never disagree about
        // what a past turn said. Silent actions file nothing.
        self.message_history.record(&self.last_events);
        self.last_events = events.clone();
        events
    }

    /// Keep the run's score, and latch its ending if this action ended it
    /// (§14 v2/#138).
    ///
    /// Every number the end screen shows is either already a fact of the state (the
    /// turn count, the intel in hand, the alert rung — which only rises, §7.3, so the
    /// standing rung *is* the peak) or is counted here, from the very events the §13.2
    /// sim counts. Two readers of one vocabulary, not two tallies that merely agree.
    fn record_verdict(&mut self, events: &[Event]) {
        for event in events {
            match *event {
                Event::Detected { .. } => self.detections += 1,
                Event::TakenDown { .. } => self.takedowns += 1,
                Event::Captured { guard, state, at } => {
                    self.ending = Some(Ending::Captured { guard, state, at });
                }
                Event::Entombed { at } => self.ending = Some(Ending::Entombed { at }),
                Event::Won => self.ending = Some(Ending::Escaped),
                _ => {}
            }
        }
    }

    /// Phase 1 (§4.2). Returns whether the turn was spent (a world-changing action)
    /// or was free (a mis-input that ends nothing).
    fn player_phase(&mut self, input: Input, events: &mut Vec<Event>) -> bool {
        // Stunned (§8.3/#329): the player cannot act, so **every** input — step, wait,
        // activation, toggle-off alike — resolves as the same stunned pass. The turn
        // is spent (the world moves on around a player who cannot), and nothing else
        // changes. One rule with no exceptions on purpose: carving out the two free
        // actions (§4.4) would leave a helpless player a free poke at the world, and
        // "you cannot act" would stop being true. It is emphatically **not** a Wait:
        // the flag is cleared, so a stunned turn buys neither the 360° look nor the
        // widened sense (§8.3/§9.1) — being knocked flat is not taking careful stock
        // of the room.
        if self.stunned > 0 {
            self.waited = false;
            return true;
        }
        // **Flying** (§8.1/#273): while the player holds a remote's controls, phase 1
        // is not about their body at all. The keys that move it move the machine, a
        // wait hovers, letting go is the free toggle-off (§4.4), and everything else —
        // every other ability, every interaction — is refused, because your hands are
        // on the controls. One rule with no carve-outs, exactly as the stun above:
        // "you are not driving your body" stops being true the moment it has
        // exceptions, and the §11.4 surfaces grey the whole bar to say so
        // ([`aim`](Self::aim), [`affordances`](Self::affordances)).
        if self.piloting {
            return self.piloted_phase(input, events);
        }
        match input {
            // Waiting is a real action: it spends the turn where you stand (§5) —
            // and buys the 360° look-around the coming sight phase grants (§8.3).
            // It also *holds* a crouch (§10.3): the pose survives exactly the
            // turns spent holding still.
            Input::Wait => {
                self.waited = true;
                // Resting pays off any haul debt (§8.3 half-speed convention):
                // the spent turn is the cost either way.
                self.drag_debt = false;
                // A wait spent standing on a body with free hands **takes hold**
                // (§8.3/#451). The look still happens — `waited` is already set, so
                // the coming sight phase grants its 360° all the same. The turn is
                // spent either way and the look costs nothing to keep, so the verb
                // loses nothing and no new key is invented; the only consequence is
                // that you cannot wait *over* a body without picking it up, and a
                // cell off is where you stand if that is what you wanted.
                self.take_body(events);
                true
            }
            Input::Step(dir) => {
                let posture = self.crouched_behind;
                let spent = self.resolve_step(dir, events);
                // Only a *spent* action stands the player up / narrows the arc: a
                // free action changes nothing, not even posture (§4.4). Two spent
                // actions keep the pose: the crouch itself — recognisable as the
                // action that changed it — and the **crouch-walk** (§10.3): plain
                // movement that lands still hugging the anchored run, corners of
                // the bench included. Any other spent step (an interaction, a
                // move that leaves the furniture) stands the player up.
                if spent {
                    self.waited = false;
                    if self.crouched_behind == posture && !self.crouch_walked(posture, events) {
                        self.crouched_behind = None;
                    }
                }
                spent
            }
            // Activating an ability spends the turn (§4.4) — but only if it actually
            // switched on; activating an unavailable ability is a mis-input and, like
            // a wall bump, is free and changes nothing. An ability whose per-level
            // budget is spent (§8.2/#302) is that same free no-op, refused inside the
            // deck — the turn cost is untouched by the budget in either direction
            // (§4.4 stands). A real activation is a spent action other than Wait, so
            // it stands the player up and narrows the arc.
            //
            // Everything *beyond* the economy — a decoy's faced cell, Pierce Wall's
            // one wall, a lockdown's doors, a blast with somebody in it — is settled
            // up front by the one precondition ladder ([`State::aim`], §8.4/#345),
            // before the deck is touched, so a refusal spends neither the turn nor a
            // use and the resolved target is carried straight to the effect that
            // consumes it. The bar reads that same ladder to grey the entry (§11.4's
            // contextual `Unusable`), which is what stops it advertising a press that
            // cannot fire.
            Input::Activate(id) => {
                // **Taking the controls back** (§8.1/#273): the remote is out, its
                // window still runs, and nobody is flying it. The press is not an
                // activation — the deck is already `Active` and would refuse one
                // (§8.2) — so it never reaches the economy; what it spends is the turn
                // (§4.4), which is the same turn the launch cost and buys the same
                // thing: your body parked while you look elsewhere. It still goes
                // through the one precondition ladder, so taking the keys from inside a
                // crawlspace is refused exactly as launching from one is.
                if self.remote_awaits(id) {
                    return match self.aim(id) {
                        Ok(_) => {
                            self.take_control(events);
                            self.waited = false;
                            self.crouched_behind = None;
                            self.drag_debt = false;
                            true
                        }
                        Err(refused) => {
                            events.extend(refused.event());
                            false
                        }
                    };
                }
                let aimed = match self.aim(id) {
                    Ok(aimed) => aimed,
                    Err(refused) => {
                        // A run that does not hold the ability presses the key in
                        // silence (§4.4/#244): there is no rule to teach about a tool
                        // that was never theirs.
                        if self.abilities.loadout().contains(id) {
                            events.extend(refused.event());
                        }
                        return false;
                    }
                };
                if self.abilities.activate(id) {
                    if let Aimed::Decoy(cell) = aimed {
                        self.decoy = Some(cell);
                    }
                    // The budget's remaining count is read *after* the deck spent
                    // it (§8.2/#302), so the message speaks what is actually left.
                    events.push(Event::AbilityActivated {
                        ability: id,
                        uses_left: self.abilities.uses_left(id),
                    });
                    match aimed {
                        Aimed::Bore(wall) => self.bore_wall(wall, events),
                        Aimed::Seal(doors) => self.seal_doors(&doors, events),
                        Aimed::Blast(blast) => self.fire_confusion(blast, events),
                        Aimed::Call(reach) => self.fire_false_call(reach, events),
                        Aimed::Dart(shot) => self.fire_dart(&shot, events),
                        Aimed::Launch(from) => self.deploy_remote(id, from, events),
                        Aimed::Nothing | Aimed::Decoy(_) => {}
                    }
                    self.waited = false;
                    self.crouched_behind = None;
                    // A spent turn pays the haul debt (§8.3), like a Wait.
                    self.drag_debt = false;
                    true
                } else {
                    false
                }
            }
            // Toggling an ability off is free (§4.4): it never spends the turn, so —
            // like every free action — it leaves posture and the waited flag alone.
            // Toggling the decoy's ability off takes the decoy with it: its lifetime
            // is the active window (§8.3). Toggling Dephase off somewhere a solid
            // body cannot stand is **refused** (a free no-op): there is nowhere to
            // rematerialize — the lethal squeeze is the duration's alone (§8.3),
            // never a mis-pressed key (§2.2: every death traceable to a decision).
            // The refusal *speaks* (§11.7): now that a player can reach this key
            // (#304), a press that changes nothing has to say why.
            Input::Deactivate(id) => {
                if declares(id, Effect::Phase) && !self.can_rematerialize() {
                    // Only a phase actually running has a toggle-off to refuse; a
                    // press with nothing on stays the silent no-op it always was.
                    if matches!(self.ability_state(id), AbilityState::Active { .. }) {
                        events.push(Event::RematerializeRefused);
                    }
                    return false;
                }
                // A remote out and unattended cannot be switched off from here
                // (§8.2/#273/#528): the window is the machine's whole life and there
                // is no early recall, so the key that would end it is refused — free,
                // and silent like every dead key around the drone, because the reason
                // is already on the board (§11.7: the machine is drawn under its own
                // mark). While flying, the same key is the let-go, handled in the
                // piloted phase before control reaches this arm.
                if self.remote_awaits(id) {
                    return false;
                }
                if self.abilities.deactivate(id) {
                    self.unwind_effect(id, events);
                    events.push(Event::AbilityDeactivated { ability: id });
                }
                false
            }
            // **Answering the exchange** (§8.3/#266). The four candidates on the bar are
            // the run's three pieces of tech and the crate's one, and this names the one
            // to discard — a trade if it is held, the decline if it is the crate's.
            //
            // A press naming anything else, or arriving with no offer open, is a
            // mis-input: free, silent, nothing changed (§4.4). The same shape as
            // activating an ability that is cooling.
            Input::Discard(id) => {
                let Some(offer) = self.exchange else {
                    return false;
                };
                match offer.resolve(self.abilities.loadout(), id) {
                    // The trade: the crate opens, the old tech goes and the new arrives
                    // on the deck ready to use this turn — the same "usable immediately"
                    // a plain salvage promises (§14 v3). **This is the one spent turn in
                    // the whole exchange**, and it is the one a plain salvage would have
                    // cost: the bump that opened the offer was free, so trading at a
                    // crate costs exactly what taking from one costs.
                    Some(Choice::Trade { dropped }) => {
                        let taken = offer.offered();
                        let at = offer.at();
                        if let Some(cache) = self.caches.iter_mut().find(|c| c.cell == at) {
                            cache.taken = true;
                        }
                        // Dropped first, then granted: the two are one exchange, and in
                        // this order the run is never momentarily over the §8.3 cap.
                        self.abilities.revoke(dropped);
                        // Whatever the dropped ability was still doing to the world goes
                        // with it — the same unwind an early toggle-off does, because
                        // that is what `revoke` just did to its slot. The remote too
                        // (#528): a machine whose ability has left the run has nothing
                        // left that could ever end it.
                        self.unwind_effect(dropped, events);
                        self.abilities.grant(taken);
                        self.exchange = None;
                        events.push(Event::Traded { taken, dropped });
                        self.waited = false;
                        self.crouched_behind = None;
                        // A spent turn pays the haul debt (§8.3), like a Wait.
                        self.drag_debt = false;
                        true
                    }
                    // The decline: the crate keeps its tech and stands where it was, so
                    // a run that comes back having traded that piece away finds it
                    // unopened. Free — nothing changed (§4.4).
                    Some(Choice::Decline) => {
                        self.exchange = None;
                        events.push(Event::ExchangeDeclined {
                            id: offer.offered(),
                        });
                        false
                    }
                    None => false,
                }
            }
        }
    }

    /// Take back whatever an ability's **window** was still doing to the world, once
    /// its slot has stopped running it (§8.2): the decoy it was standing up, the remote
    /// it was keeping alive, the doors it was holding shut, the marks it had painted.
    ///
    /// **The one teardown list**, shared by every way a window can end — expiry in the
    /// turn loop, the player's free toggle-off (§4.4/#304), and the exchange trading
    /// the ability away (#266) — so an ability that grows something to unwind cannot
    /// have it unwound on one path and left behind on another (#528: expiry once kept
    /// its own copy, and the remote leaked through the gap).
    fn unwind_effect(&mut self, id: AbilityId, events: &mut Vec<Event>) {
        // The decoy's lifetime is its ability's active window (§8.3).
        if declares(id, Effect::SpawnDecoy) {
            self.decoy = None;
        }
        // A remote's life is its ability's window and nothing else (§8.1/#273): the
        // window ending kills the machine and, if the player was still flying it,
        // hands the keys back with it. On the early paths this only ever fires for a
        // trade — the toggle-off of a live remote is refused before it gets here —
        // so "no early recall" holds at the key while a machine can still die with
        // the loadout that was its life.
        self.end_remote(id, events);
        // The seals are the window (§8.3/#242): ending it early hands every door back
        // at once. It refunds nothing — the full lockout still runs (§8.2).
        if declares(id, Effect::SealDoors) {
            self.release_lockdown();
        }
        // The effect is gone, so its marks go with it (#308/#338) — an early end
        // leaves no residue to fade over nothing.
        self.clear_effect_marks(id);
    }

    /// Whether the spent step just resolved was a **crouch-walk** (§10.3): the
    /// pose survives only plain movement — the turn's events carry a
    /// [`Event::Moved`], so an interaction that spends the turn in place (a
    /// door, a grab, a haul-debt payment) still stands the player up — that
    /// lands still hugging the anchored run ([`cover::run_hugs`]: within one
    /// cell of any of its tables, the diagonal past a bench's end included, so
    /// the walk can round the corner). A sprinting step (§8.3 Run) is judged
    /// where it *ends*, like every other consequence of the two-cell move.
    fn crouch_walked(&self, posture: Option<Cell>, events: &[Event]) -> bool {
        let Some(anchor) = posture else {
            return false;
        };
        events.iter().any(|e| matches!(e, Event::Moved { .. }))
            && cover::run_hugs(
                &cover::cover_run(self.layout.facility(), anchor),
                self.player,
            )
    }

    /// Whether `table` belongs to the run the player is currently crouched
    /// behind (§10.3) — the "is this bump the pose I already hold" question the
    /// interaction ladder asks to keep a held re-bump free (§4.4).
    fn crouch_covers(&self, table: Cell) -> bool {
        self.crouched_behind
            .is_some_and(|anchor| cover::cover_run(self.layout.facility(), anchor).contains(&table))
    }

    /// Phases 2 and 3 (§4.2): recompute sight, run the radio net, then let the
    /// guards act. Shared by the startup turn and every spent player turn. The
    /// radio sits between sight and the guards deliberately: a responder it
    /// dispatches this turn has its cone recomputed (it moved last turn) and then
    /// senses and steps *this* turn, so the dot peels off the moment control
    /// notices the silence (§7.3), not a turn late.
    fn run_world_phases(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        // Whether the facility was already searching when this turn's world opened
        // (§7.6/§11.7/#224). Read here rather than inside the guard phase because a
        // search can end in any of the phases below — the radio can pull the last
        // searcher onto an errand, a guard can release, a fresher lead can supersede
        // one — and the near line reports the *boundary*, not the mechanism that
        // crossed it. Reported at the foot of this function, against the same reading.
        let was_searching = self.search_under_way();
        // Fade the sense channel one turn *before* this turn's facts can relight it
        // (§9/§9.4) — the door cues and the guard trail alike — so a cue placed this
        // turn keeps its full life and a re-stamp refreshes rather than
        // double-decrements.
        self.decay_sense_cues();
        // The effect marks age on the same schedule and for the same reason
        // (§11.5/#308/#338): one turn of life spent before this turn's activation can
        // light a fresh one ([`record_effect_marks`](Self::record_effect_marks)).
        self.decay_effect_marks();
        self.recompute_sight();
        self.radio_phase(&mut events);
        self.guard_phase(&mut events);
        // Rungs 2 and 3 send guards in (§7.3/#374). *After* the guards have acted, and
        // deliberately so: phase 3 indexes its per-guard readings by position, so a
        // body found mid-phase must not grow the vector it is reading. The arrival is
        // the end of the turn it was called for, and a guard that has just walked in
        // acts from the next one — it has not looked yet, and its cone is recomputed
        // with everybody else's at the head of it.
        self.land_reinforcements(&mut events);
        // A reinforcement whose errand ended this turn changes the guard set, so the
        // level's §7.5 partition is recut here: the newcomer takes ground around where
        // it finished rather than where it walked in (§7.3/#374), and the incumbents'
        // beats shrink to make room. It sits after the arrivals so a guard that lands
        // and stands down on the same turn is still settled on that turn, and it is a
        // no-op on every other turn.
        self.recut_beats();
        self.door_phase(&mut events);
        // Autodoors shuts the doors the player passed through this run once their
        // throats clear (§8.3/§7.6) — after the guards move, so a pursuer stepping
        // into the doorway holds it open exactly as any occupant does (§10.4).
        self.close_armed_autodoors(&mut events);
        // Latch a fading cue on every door the player did not cause (§10.4) — the
        // player's own open is emitted in phase 1 and never reaches here.
        self.record_door_cues(&events);
        // Stamp the other half of the same channel (§9.2/#192): the cell of every guard
        // the player still feels through a wall, read *after* the guards have moved and
        // the reinforcements have landed, so the mark is where the turn actually left
        // them.
        self.record_guard_cues();
        // Last, so the pair reports what the whole world turn settled on rather than
        // what any one phase did (§11.7/#224). Its rung is below every threat message,
        // so being the final push costs it nothing: `loudest_first` sorts by priority,
        // and only a tie with another search boundary — which cannot happen, the two
        // being a diff of one bool — could turn on the order.
        self.report_search_boundary(was_searching, &mut events);
        events
    }

    /// Step the facility alert ladder (§7.3) and report it if it actually rose, `at`
    /// being the cell the escalation is **about** — the body that was found, the
    /// console that was tampered with, the post that went quiet, the player's own cell
    /// for a sighting.
    ///
    /// The one place the rung is written, so the no-decay rule (§7.3) and the
    /// "one event per escalation" rule are both held in a single line of code. A
    /// trigger at or below the current rung is silent: the ladder reports
    /// *escalations*, not occurrences, so a chase that keeps producing rung-1
    /// triggers at rung 3 says nothing.
    ///
    /// Being the only writer is also what makes rungs 2 and 3 send their guards
    /// **once** (#374): the rungs crossed are read here, from a number that only ever
    /// rises, so nothing has to remember what has already been paid for. `at` is the
    /// errand each arriving guard is given, which is why the cell is threaded in rather
    /// than looked up — control sends them to what it just learned about, not to
    /// wherever the player happens to be by the time they walk in (§7.6).
    fn raise_alert(&mut self, trigger: AlertTrigger, at: Cell, events: &mut Vec<Event>) {
        let from = self.alert.rung();
        if let Some(rung) = self.alert.raise(trigger) {
            events.push(Event::AlertRaised { rung, trigger });
            self.queue_reinforcements(from, rung, at);
        }
    }

    /// Phase 2 (§4.2): recompute every viewer's field of view from its current
    /// position and facing (§6). Running *after* the player acts and *before* the
    /// guards read it is what designs out the old one-turn sensory lag (§4.2). The
    /// player's arc is the ~180° half-disc — or the full 360° if this turn was spent
    /// waiting (§8.3) — and their sight carries the auto-peek (#121): the union
    /// with the cast from one cell ahead, which reads around adjacent corners and
    /// out of cupboard mouths. Guards carve their ~90° wedge with the **plain**
    /// cast (§6.2) — the peek is the player's alone, so a corner the player can
    /// read still breaks the guard's line (§7.6).
    fn recompute_sight(&mut self) {
        let facility = self.layout.facility();
        let sight = self.guard_sight();
        // Inside a duct the normal cone is off (§10.7): the player perceives only
        // their memory of the building and the shortened guard sense, with one live
        // window — the mouth peek from an entry cell (§6.1). Mid-duct there is no
        // live vision at all. `duct_fov` builds exactly that restricted set.
        self.player_fov = if self.in_duct() {
            self.duct_fov()
        } else {
            field_of_view_with_peek(
                facility,
                self.player,
                self.facing,
                self.player_sight_arc(),
                self.player_sight_range(),
            )
        };
        // **What a remote sees is folded in here** (§6/§11.5a/#273), at the one place
        // sight is produced, so everything downstream follows on its own: the fog lifts
        // over the corridor the drone is watching, entities there draw, the §11.5 danger
        // overlay paints the cones it can see, and tile memory accumulates it all
        // (§11.5a — once seen, remembered, which is the ability's actual payoff).
        //
        // A **union**, not a replacement: your own eyes keep working while you fly, so
        // being at the controls does not blind you at home. It is fed whether or not
        // anybody is holding the controls — a camera left in a junction watches that
        // junction while you walk the other way, and that is what the second half of the
        // window is worth (§8.2).
        //
        // What is deliberately *not* widened is the §9 guard sense: that is your body's
        // own innate channel, and leaving it on your body is what keeps a parked body a
        // real risk rather than one more thing the drone covers for you.
        if let Some(remote_fov) = self.remote_fov() {
            self.player_fov.absorb(&remote_fov);
        }
        // The debug reveal (§12.6): the player's sight is *replaced* by the whole
        // grid — the playtest build where you can see the level. It is done here, at
        // the one place sight is produced, rather than as a special case in each view
        // that reads it: everything downstream then follows from the substitution on
        // its own — the fog lifts, entities draw wherever they stand, and every guard
        // reads as **Seen** (§9.2), so the §11.5 danger overlay paints every cone.
        // Nothing switches this on in a real run; only a baked build can (#245 tokens
        // cannot carry it), and it changes only what the *player* perceives — guards
        // look with their own cones, so the facility plays exactly as it would.
        if self.debug.reveal_whole_level {
            self.player_fov = VisibleSet::everything(facility.width(), facility.height());
        }
        // Tile memory (§11.5a) accumulates here, in the same phase that produced
        // the sight — every cell the player can see now is remembered forever. A
        // duct's interior path is deliberately *not* accumulated: it lives in its own
        // layer, shown only while crawled and never remembered (§11.5a/§10.7), so the
        // crawlspace's route is given away to nobody once the player has left it.
        //
        // The crawl view (`duct_fov`) always contains the occupied cell, so honouring
        // that rule means holding that one cell back while it is an **interior** cell.
        // An **entry** is geometry (§10.7) and accumulates like any other cell. This
        // used to be prose only: the cell went in, harmlessly, because memory drove
        // nothing but *contents*. Once memory drives how geometry is drawn, a crawled
        // duct would otherwise light its interior as explored — a thread of known wall
        // tracing the shortcut across the map, which is exactly the tell §11.5a keeps
        // the path in its own layer to avoid.
        let crawled_interior = self
            .occupied_duct()
            .filter(|duct| !duct.is_entry(self.player))
            .map(|_| self.player);
        self.memory
            .absorb_except(&self.player_fov, crawled_interior);
        for guard in &mut self.guards {
            guard.look(facility, sight);
        }
    }

    /// The player's field of view while **inside a duct** (§10.7) — the restricted
    /// perception a crawlspace grants. Always the player's own cell; and, when they
    /// stand on an **entry**, the live **mouth peek**: a cast out through the mouth
    /// (§6.1), the deliberate, safe way to read the room before climbing out. A
    /// mid-duct cell has no mouth and so no live window — memory only, which is the
    /// duct's information cost (§2.3). The peek is one-sided like the cupboard's: it
    /// shows the player a guard, but the guard's own plain cone cannot see a concealed
    /// crawler back ([`concealed_from`](Self::concealed_from)).
    fn duct_fov(&self) -> VisibleSet {
        crate::vision::duct_field_of_view(
            self.layout.facility(),
            self.player,
            self.duct_entry_facing(self.player),
            self.player_sight_arc(),
            self.player_sight_range(),
        )
    }

    /// The player's sight arc this turn (§6.2): §5's ~180° half-disc, widened to the
    /// full 360° ([`FULL_SIGHT_ARC`]) by **either** a turn spent waiting (§8.3/§9.1)
    /// **or** the Vision passive (§8.3/#265).
    ///
    /// The two widenings meet here, in one expression, precisely so they cannot
    /// stack: 360° is the whole circle and there is nothing past it, so waiting
    /// while holding Vision buys the same arc it already had — the wait's other
    /// half, the widened guard sense (§9.1), is what still makes it worth a turn.
    fn player_sight_arc(&self) -> u8 {
        if self.waited || self.abilities.effect_active(Effect::EnhancedSight) {
            FULL_SIGHT_ARC
        } else {
            PLAYER_SIGHT_ARC
        }
    }

    /// The player's sight range this turn (§5/§6.1): the §5 box, or the enlarged one
    /// while the Vision passive is held (§8.3/#265). Waiting does **not** extend
    /// range — it only ever bought the arc (§9.1).
    fn player_sight_range(&self) -> u32 {
        if self.abilities.effect_active(Effect::EnhancedSight) {
            ENHANCED_SIGHT_RANGE
        } else {
            PLAYER_SIGHT_RANGE
        }
    }

    /// The index of a guard standing on `cell`, if any.
    fn guard_at(&self, cell: Cell) -> Option<usize> {
        self.guards.iter().position(|g| g.pos() == cell)
    }

    /// The index of a body lying on `cell`, if any.
    fn body_at(&self, cell: Cell) -> Option<usize> {
        self.bodies.iter().position(|b| b.cell() == cell)
    }

    /// Whether any actor occupies `cell` — the **door-crush** occupancy predicate
    /// (§10.4: a door never shuts on an actor). Actors are the player, the guards,
    /// and the bodies takedowns leave (§7.2), so a door refuses to close on a body
    /// even though a body is otherwise non-solid to movement and pathing. Movement
    /// checks read the player and guards directly, not this.
    fn occupied(&self, cell: Cell) -> bool {
        actor_occupies(self.player, &self.guards, &self.bodies, cell)
    }
}

/// Whether `id` declares `effect` in its data-driven behaviour (§8.1) — how the
/// loop asks "does activating this spawn a decoy?" without naming an ability. A
/// [`Behaviour::Coded`] ability declares nothing here; its behaviour would live
/// in code keyed on the id.
fn declares(id: AbilityId, effect: Effect) -> bool {
    matches!(id.def().behaviour(), Behaviour::Effects(effects) if effects.contains(&effect))
}

/// Whether an actor occupies `cell`, given the actor fields directly. The free
/// twin of [`State::occupied`], for callers that must borrow the actor fields apart
/// from the rest of the state (door closing borrows the layout mutably at the same
/// time). One definition of "an actor is here" — extend it, not the call sites, when
/// new actor kinds arrive. A body counts here so a door never closes on one (§10.4 —
/// doors never crush, and a body under a shut door would be unreachable). A body is
/// otherwise **non-solid** (§7.2): it blocks neither movement nor pathing, so the
/// loop's movement checks read guards and the player directly, not this predicate.
fn actor_occupies(player: Cell, guards: &[Guard], bodies: &[Body], cell: Cell) -> bool {
    player == cell
        || guards.iter().any(|g| g.pos() == cell)
        || bodies.iter().any(|b| b.cell() == cell)
}

#[cfg(test)]
mod tests;
