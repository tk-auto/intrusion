//! The baseline stealth bot (§13.2–§13.4): a greedy [`PlayerPolicy`] that turns
//! the harness's replay checksums into balance signals.
//!
//! **It is a smoke detector, not a good player** (§13.4). A bot with perfect
//! information and no fear plays nothing like a human; the point is not that it
//! plays *well* but that it plays *at all* — legibly and the same way every seed —
//! so win rate, detection counts and the ability histogram measure the *game*, not
//! a hand-tuned solver. When a metric spikes, the behaviour here is simple enough
//! to trace the spike to bot or game.
//!
//! # It cheats at nothing (§13.2, §11.5a)
//!
//! The bot decides from the *same information a player is shown*, never the raw
//! [`State`] internals:
//!
//! - **Geometry** is always known — walls, floors, doors — read from
//!   [`State::layout`] (§11.5a: "geometry always"). It also knows the **exit** from
//!   the start: it is the player's own tunnel, the way they came in.
//! - **Contents** — the intel consoles — are *fogged*: unknown until seen and
//!   remembered after ([`State::memory`]). The bot cannot route to intel it has
//!   never laid eyes on; it explores to find it, exactly as a player must.
//! - **Guards** are perceived through [`State::perceive_guard`] (§9.2): a **seen**
//!   guard's cone is known and avoided (the danger overlay, §11.5); a **sensed**
//!   guard is a bare position to keep away from; one that is neither is invisible.
//!
//! # What it does, in priority order
//!
//! 1. **Flee** when hunted (§7.6): activate Run to open a gap, make for a known
//!    hideout, and hold still inside it until the hunt passes — contact cannot
//!    reach a hidden player (§4.5). With no hideout to reach, cloak with Camouflage
//!    (a hideout you carry, §8.3) and hold; the exit is never a refuge here — you
//!    cannot disappear into your own tunnel, nor even step onto it with objectives
//!    still out.
//! 2. **Pursue** the objective otherwise: route to the nearest *known* untaken
//!    console, take it, and once all intel is in hand route back to the exit —
//!    preferring cells no visible guard is watching, and holding a beat when the
//!    only step ahead crosses a cone.
//! 3. **Explore** when no intel is known yet: head for the nearest frontier — a
//!    seen cell bordering the unseen — which sweeps the facility until the consoles
//!    reveal themselves.
//!
//! It uses abilities only where they earn their place (Run to flee, a takedown to
//! clear a guard blocking the route), never a rehearsed optimal line — so the
//! histogram has something real to measure without one verb drowning the rest.
//!
//! # Which key it presses, and why (§13.2/#346)
//!
//! The bot does not decide that per ability. It names the plan it has settled on
//! ([`Intent`]) and puts the moment to **every held ability's cue** ([`crate::cue`]),
//! which answers for itself whether this is a moment it is *for* and how badly.
//! One call site here ([`StealthBot::cue`]); one exhaustively-matched arm per
//! ability there. That exhaustiveness is the point: an ability added to the §8.1
//! catalog fails to compile until somebody says what it is for, so no new verb can
//! land as a silent zero in the usage histogram — and a false zero is
//! indistinguishable from a dead ability.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use intrusion_core::{
    AbilityId, AbilityState, Cell, Direction, Facility, GuardPerception, GuardState, Input, State,
    Terrain,
};

use crate::cue::{Bid, Intent, Moment};
use crate::policy::PlayerPolicy;
use crate::profile::{Descent, Profile};

/// How far a found body's §7.6 search reaches, as a keep-away radius the bot honours when
/// choosing a bolthole (§15 Q5, the found-a-body-nearby check). It mirrors the core
/// `SEARCH_RADIUS` disc a guard sweeps around a corpse it finds and checks the cupboards
/// in — kept a local [START] because the bot reads the game as a player would, not from
/// core's constants; conservative, so the bot errs toward not hiding beside a body it left.
const BODY_HIDE_CLEARANCE: u32 = 4;

/// How the bot routes when closing on a guard's back (§7.2/§155) — the one descent
/// mode no [`Profile`] sets, because it is not a temperament: **you cannot both keep
/// your distance from a guard and walk up behind it**, so `keep_clear` is off for
/// every profile that takes this route at all. Patience stays on: a strike is never
/// worth crossing a cone to reach, and the whole play is void the moment somebody
/// sees you coming.
const APPROACH: Descent = Descent {
    keep_clear: false,
    hold_watched: true,
};

/// A keep-away cost for stepping onto `cell`: the closer a perceived guard, the
/// steeper it climbs (with the square of how far inside the profile's
/// [`proximity_radius`](Profile::proximity_radius) the guard sits), so the bot gives
/// patrols a wide berth instead of brushing past a cone's edge where the next sweep
/// would catch it. Zero when no guard is near.
fn proximity_penalty(cell: Cell, guards: &[Cell], profile: &Profile) -> u64 {
    guards
        .iter()
        .map(|&guard| cell.manhattan_distance(guard))
        .filter(|&distance| distance <= profile.proximity_radius)
        .map(|distance| {
            let closeness = u64::from(profile.proximity_radius + 1 - distance);
            closeness * closeness * profile.proximity_unit
        })
        .sum()
}

/// The greedy baseline stealth bot (§13.2), playing one [`Profile`]'s temperament.
/// Holds only what a player would carry in their head across turns: which consoles it
/// has already emptied — the game keeps a taken console stamped as terrain, so without
/// this the bot would keep routing to intel it has already taken.
#[derive(Clone, Debug, Default)]
pub struct StealthBot {
    /// The temperament this bot plays: every threshold it weighs its options by
    /// (§13.2). One policy, one row of numbers — see [`Profile`].
    profile: Profile,
    /// Consoles the bot has taken. Recorded optimistically the turn it steps onto a
    /// known untaken console: a bump into an untaken console in the field of view
    /// always takes it (§4.3/§6.2, the touching ring is always seen), so the take is
    /// certain and this never drifts from the game's own objective count.
    taken: HashSet<Cell>,
    /// Turns spent waiting in the current cover stint. Bounds how long the bot will
    /// sit in a cupboard for a patrol that will not leave: past the profile's [`max_hide`](Profile::max_hide) it gives
    /// up and pushes on, so a lingering guard turns into a timeout- or capture-risk,
    /// never an endless wait. Reset to zero the moment it is not hidden.
    hide_turns: u32,
    /// Turns to press on before taking cover again, set when the bot leaves a
    /// bolthole. Without it a patrol whose beat loops past a cupboard would send the
    /// bot ducking in, stepping out, and straight back in — burning the whole input
    /// budget to a timeout without progress. The cooldown forces a stretch of actual
    /// pursuit between hides, so the loop advances instead of spinning.
    cover_cooldown: u32,
    /// The last ability [`Bid`] that won a moment, if any — kept so a flagged seed
    /// can be traced back to the cue's own stated reason (§13.3), rather than to a
    /// bare "it pressed the key". Pure bookkeeping: no decision reads it.
    last_bid: Option<Bid>,
    /// Bodies the bot has put down and will not pick up again (§8.3), by the cell they
    /// were released in. A body it could find no route to a cupboard with is one it
    /// would otherwise fetch, fail to place, drop, and fetch again for the rest of the
    /// run — so giving up on one is remembered rather than rediscovered.
    abandoned: HashSet<Cell>,
}

impl StealthBot {
    /// A fresh bot with nothing taken yet, playing the [`Profile::BASELINE`]
    /// temperament — today's bot, so metrics stay comparable across the seam.
    pub fn new() -> Self {
        Self::default()
    }

    /// A fresh bot playing `profile`'s temperament (§13.2). The same policy as
    /// [`StealthBot::new`]; only the numbers it weighs its options by differ.
    pub fn with_profile(profile: Profile) -> Self {
        Self {
            profile,
            ..Self::default()
        }
    }

    /// The temperament this bot is playing — the name every emitted row carries,
    /// so a batch's output is attributable to the profile that produced it.
    pub fn profile(&self) -> Profile {
        self.profile
    }

    /// The last ability cue that won a moment (§13.2/#346), reason and all — the
    /// handle a §13.3 investigation reaches for when a histogram slot moves and the
    /// question is *why* the bot pressed that key. `None` until one has.
    pub fn last_bid(&self) -> Option<Bid> {
        self.last_bid
    }
}

impl PlayerPolicy for StealthBot {
    fn profile_name(&self) -> Option<&'static str> {
        Some(self.profile.name)
    }

    fn decide(&mut self, state: &State) -> Input {
        // A cover stint only counts while actually *in* cover — a cupboard, or the
        // crouch behind a bench (§10.3); standing up (or never ducking) resets it.
        // A temperament that declines the pose can never be crouched, so the second
        // clause is silent for it.
        if !state.hidden() && !state.crouched() {
            self.hide_turns = 0;
        }

        // The world's own facts, gathered once through the player's channels.
        let danger = danger_cells(state);
        let mut blocked = blocked_cells(state);
        // An unaware guard this temperament will not strike is an obstacle, not an
        // opportunity — left blocked so the router waits it out (§7.2).
        blocked.extend(declined_takedowns(state, &blocked, &self.profile));

        // 0. Hands full first: a drag halves the bot's speed and refuses to stack with
        // Run (§8.3), so the body in hand is settled before anything else is planned.
        if let Some(input) = self.haul(state, &danger, &blocked) {
            return input;
        }

        // 0.5. Phased inside a solid: get out, before anything else is considered.
        // A duration that expires in there costs a safety eject *plus* a stun as long
        // as the throw (§8.3), which is worse than anything the other branches are
        // weighing — including being seen, since the phase conceals nothing anyway.
        if let Some(input) = self.leave_the_wall(state) {
            return input;
        }

        // 1. Flee: nothing else matters while a guard has you (§7.6).
        if being_hunted(state, &danger) {
            return self.flee(state, &danger, &blocked);
        }

        // 2. A patrol with its back turned, and a temperament that wants it (§7.2).
        // Ahead of cover deliberately: the commonest safe angle in the whole game is
        // the one from *inside* a cupboard, where `take_cover` would only ever wait.
        if let Some(input) = self.strike(state, &danger, &blocked) {
            return input;
        }

        // 3. Something left on the floor worth tidying away (§7.2/§10.3).
        if let Some(input) = self.fetch(state, &danger, &blocked) {
            return input;
        }

        // 4. Not caught yet, but a patrol is closing and a bolthole is to hand: duck
        // in and let it pass rather than press the objective into its path. This is
        // where most detections are avoided — the player senses a guard as far out as
        // it could see them (both range 10, §9.1), so there is time to take cover.
        if let Some(input) = self.take_cover(state, &danger, &blocked) {
            return input;
        }

        // 5 & 6. Pursue the objective, or explore to find it.
        self.pursue(state, &danger, &blocked)
    }
}

impl StealthBot {
    /// Break contact (§7.6): open a gap with Run, make for a bolthole, and wait the
    /// hunt out from inside a hideout — the one place a guard's contact cannot reach
    /// (§4.5). Getting *to safety* is the whole job here, so it drives straight for
    /// the nearest refuge rather than keeping its polite distance from the chaser.
    fn flee(&mut self, state: &State, danger: &HashSet<Cell>, blocked: &HashSet<Cell>) -> Input {
        // Already hidden: the safest cell on the board. Hold still and let the
        // hunt cool (§7.6) — moving would only reveal the cupboard.
        if state.hidden() {
            return Input::Wait;
        }

        // Aim for the nearest known hideout to disappear into — the one place a
        // guard's contact cannot reach (§4.5) — but never one a hunter is watching
        // (diving in under an alerted cone is witnessed, and a witness flushes you right
        // back out, §15 Q5), nor one within reach of a body: a guard that finds the body
        // searches the cupboards beside it and flushes you the same way (§15 Q5). Break
        // sight first, and hide away from your own handiwork, and the cupboard is safe.
        //
        // Worked out *before* the cues, not after: an ability that stands in for a
        // cupboard (§8.3, Camouflage) has to know whether there is a real one to
        // reach, and the routing is a pure function of the state, so asking early
        // costs nothing but the Dijkstra.
        let witnessed = witnessed_cone_cells(state);
        let bodies = findable_bodies(state);
        let boltholes: Vec<Cell> = known_hideouts(state)
            .into_iter()
            .filter(|h| !witnessed.contains(h) && !near_findable_body(&bodies, *h))
            .collect();
        let refuge = self.descend(state, &boltholes, danger, blocked, self.profile.flee);

        // Ask every held ability whether this is a moment it is *for* (§13.2/#346).
        // The exit is deliberately not among the answers, nor a fallback refuge
        // below: you cannot disappear into your own tunnel, and with objectives
        // still in the facility you cannot even step onto it (§4.5), so routing
        // there only bumps a door that never opens — a free action that spends no
        // turn and so never lets the hunt cool, stalling the run out to the input
        // cap instead of breaking contact.
        // Nowhere to run to: back away from the nearest guard, off watched cells when
        // it can, and hold only if truly cornered. Settled *before* the cues, because
        // the step this plan would take is itself a fact a cue needs — an ability that
        // lands in a cell must not aim into the bot's own escape (§8.3, Decoy).
        let step = refuge.or_else(|| retreat_step(state, danger, blocked));

        if let Some(input) = self.cue(state, Intent::Flee, refuge, step, None) {
            return input;
        }
        step.map_or(Input::Wait, Input::Step)
    }

    /// Take cover from a closing patrol before it ever sees you: when a guard is
    /// perceived within the profile's [`threat_radius`](Profile::threat_radius), slip into a near hideout — or, with none to
    /// hand, cloak with Camouflage — and wait the patrol out (§7.6/§8.3/§10.3).
    /// Returns `None` when there is no near threat, or nothing to take cover with,
    /// leaving the bot to pursue as normal.
    ///
    /// Inside the cupboard it holds until the coast clears (no guard within
    /// [`clear_radius`](Profile::clear_radius)), then pursuit resumes: the "hide, let it pass, carry on" loop
    /// that is the whole point of a hideout.
    ///
    /// Below both of those sits a second, weaker refuge for the temperaments that
    /// take it: ducking behind a bench (§10.3, [`crouch`](Self::crouch)). It is **last**
    /// because it is the weakest of the three — concealment that is directional, only
    /// across the chosen furniture, and never contact-safe (§4.5) — and the pose is
    /// given up again the moment it stops covering the bot, rather than waited out.
    fn take_cover(
        &mut self,
        state: &State,
        danger: &HashSet<Cell>,
        blocked: &HashSet<Cell>,
    ) -> Option<Input> {
        let player = state.player();
        let nearest = nearest_perceived_guard(state);

        if state.hidden() {
            self.hide_turns += 1;
            // Come out once the patrol is *well* clear — a wider radius than the one
            // that sent the bot in (hysteresis), so it does not pop out into a guard
            // still on its doorstep and duck straight back, glimpsed each time. And
            // never wait forever: past the cap, give up and push on. On leaving, set
            // a cooldown so the bot makes real progress before it may hide again.
            let clear = nearest.is_none_or(|d| d > self.profile.clear_radius);
            if clear || self.hide_turns > self.profile.max_hide {
                self.cover_cooldown = self.profile.cover_cooldown;
                return None;
            }
            return Some(Input::Wait);
        }
        // Already crouched (§10.3): the turn is spent, so the stint runs out on the
        // same hysteresis and the same cap as a cupboard's — stand up once the patrol
        // is well clear, and never wait forever.
        if let Some(anchor) = state.crouched_behind() {
            self.hide_turns += 1;
            let clear = nearest.is_none_or(|d| d > self.profile.clear_radius);
            if clear || self.hide_turns > self.profile.max_hide {
                self.cover_cooldown = self.profile.cover_cooldown;
                return None;
            }
            let threats = self.nearby_threats(state);
            // The pose still hides us from everyone near: hold it, which is the one
            // thing that costs nothing and keeps it (§10.3).
            if conceals_from_all(state, anchor, player, &threats) {
                return Some(Input::Wait);
            }
            // It has stopped. A patrol has come round to *this* side of the bench, so
            // shuffle along the furniture — the crouch-walk is the only spent step
            // that does not stand you up (§10.3).
            if let Some(step) = self.crouch_walk(state, anchor, blocked, &threats) {
                return Some(step);
            }
            // Neither hides us any more. **Do not wait here**: waiting out a patrol
            // behind a bench that has stopped covering you is standing still in the
            // open, and it is how a bot that believes itself hidden gets seen. Fall
            // through to the ordinary ladder instead — a cupboard, the cloak, or a
            // different bench — and give the spent pose up.
        }
        // Fresh out of cover: press on for a stretch before hiding again, so a patrol
        // looping past a cupboard cannot trap the bot in an in-and-out shuffle.
        if self.cover_cooldown > 0 {
            self.cover_cooldown -= 1;
            return None;
        }
        // Duck in only when a patrol is genuinely closing.
        if nearest.is_none_or(|d| d > self.profile.threat_radius) {
            return None;
        }
        // Only worth a detour to a hideout that is genuinely close by; a far one is
        // not cover, it is a march across the guard's path. And never one an alerted
        // guard is watching — climbing in there is a witnessed dive the guard flushes
        // (§15 Q5); a Calm patrol's cone is fine, which is the usual case here — nor one
        // within reach of a body, which a finder would search and flush the same way.
        let witnessed = witnessed_cone_cells(state);
        let bodies = findable_bodies(state);
        let hideouts: Vec<Cell> = known_hideouts(state)
            .into_iter()
            .filter(|h| {
                player.manhattan_distance(*h) <= self.profile.cover_reach
                    && !witnessed.contains(h)
                    && !near_findable_body(&bodies, *h)
            })
            .collect();
        let refuge = self.descend(state, &hideouts, danger, blocked, self.profile.flee);

        // With the cupboard on offer known, ask the abilities. One of them may be a
        // cupboard you carry (§8.3, Camouflage) — a *still* cloaked player is
        // concealed from every viewer, so `being_hunted` will not fire and the hold
        // keeps going until the coast clears. A real cupboard still wins: the cue
        // only speaks when `refuge` is `None`.
        if let Some(input) = self.cue(state, Intent::TakeCover, refuge, refuge, None) {
            return Some(input);
        }
        // Last and weakest (§10.3): no cupboard within reach and nothing to cloak
        // with, but a bench may be underfoot. Below the other two on purpose — see
        // [`crouch`](Self::crouch) for what the trade actually is.
        refuge.map(Input::Step).or_else(|| self.crouch(state))
    }

    /// Duck behind a bench (§10.3): bump a table at the bot's elbow and be concealed
    /// from every viewer across it — the last and weakest thing the `TakeCover` intent
    /// has to offer, and nothing at all to a temperament that declines concealment
    /// ([`crouches`](Profile::crouches)).
    ///
    /// **It is not a cheaper cupboard, it is a different trade.** A cupboard is
    /// omnidirectional and contact-safe: hidden, nothing detects you and nothing can
    /// walk into you (§4.5). A bench conceals *directionally* — only from viewers
    /// across the furniture — and stops no one: a patrol that strolls onto the
    /// crouching bot captures it, believing itself unobserved right up to the moment
    /// it is caught, because [`being_hunted`] reads the very `concealed_from` the
    /// crouch defeats. What it buys instead is that it is **everywhere and free**:
    /// §10.1a stamps benches all over the facility, while a cupboard is scarce and
    /// usually the wrong side of the patrol.
    ///
    /// So it serves the **`TakeCover`** intent and no other, as that intent's floor.
    /// It is not `Flee` — you do not break contact behind a table a guard can walk
    /// through your cell to reach — and it is not `Pursue`, because it makes no
    /// progress: the crouch is worth a turn only when a patrol is closing and the turn
    /// was going to be spent hiding anyway.
    ///
    /// # A flag, not a reach
    ///
    /// From your own cell the crouch is a **reflex rather than an appetite**, which is
    /// what makes it unlike the takedown ([`strike`](Self::strike)) next door: ducking
    /// behind the table at your elbow when a patrol walks in is what anybody does,
    /// careful or impatient. So among the profiles that spend turns on cover at all
    /// there is nothing left to dial, and the profiles that crouch still do it at very
    /// different rates (`baseline` 8, `cautious` 13, `aggressive` 2 over 100 seeds) on
    /// the numbers they already carry: how near a patrol has to be before cover is
    /// worth a turn ([`threat_radius`](Profile::threat_radius)), and how far a
    /// *cupboard* is worth walking to instead ([`cover_reach`](Profile::cover_reach)).
    ///
    /// A `crouch_reach` — *how far will it walk to a bench*, the obvious sibling to
    /// [`takedown_reach`](Profile::takedown_reach) — was built and **measured out
    /// again** (#379), because a bench you walk to goes *stale*: the spot is chosen for
    /// where a guard stands now, and by the time the bot arrives it has moved and the
    /// concealing side of the furniture has flipped. Over 100 seeds a reach of 2 or
    /// more did not add crouches, it **replaced** them — from ~51 down to ~1 — as the
    /// bot spent its cover turns walking to benches it never ducked behind. What is
    /// left to say is yes or no, which is [`crouches`](Profile::crouches): `careless`
    /// declines, for the same reason it declines cupboards, and its §7.2 row is the
    /// rear blind spot and nothing else.
    ///
    /// A table is worth ducking behind only when it hides the bot from **every** guard
    /// it currently perceives nearby. Concealment from one of two patrols is not cover,
    /// it is a coin toss — and the question is put to the core's own geometry
    /// ([`State::crouch_would_conceal`]), never re-derived here, which is the rule the
    /// routing and legality predicates already keep (§13.2).
    fn crouch(&self, state: &State) -> Option<Input> {
        if !self.profile.crouches {
            return None;
        }
        let threats = self.nearby_threats(state);
        if threats.is_empty() {
            return None;
        }
        let facility = state.layout().facility();
        let player = state.player();
        // In `Direction::ALL` order, so a cell with tables on two sides ducks behind
        // the same one every time (§12.4).
        Direction::ALL
            .into_iter()
            .find(|&dir| {
                player.step(dir).is_some_and(|table| {
                    facility.terrain(table) == Some(Terrain::PartialCover)
                        && conceals_from_all(state, table, player, &threats)
                })
            })
            .map(Input::Step)
    }

    /// Shuffle along the bench to keep a pose that has stopped working (§10.3): the
    /// **crouch-walk**, the one spent action other than the duck itself that does not
    /// stand the player up.
    ///
    /// Cover is where the furniture is, not a status the crouch grants, so a patrol
    /// that walks round to the player's own side of a bench sees them. Standing up and
    /// re-ducking would cost two turns and be exposed for the first; a plain step that
    /// lands still hugging the anchored run costs one and is exposed for none. Both
    /// halves are asked of core — [`State::crouch_holds`] for whether the pose survives
    /// the step, [`State::crouch_would_conceal`] for whether the cell it lands on is
    /// hidden from every threat — so the bot never carries its own copy of §10.3.
    ///
    /// `None` when no step along the bench restores the cover, which is the caller's
    /// signal to give the pose up rather than sit still behind furniture that has
    /// stopped working.
    fn crouch_walk(
        &self,
        state: &State,
        anchor: Cell,
        blocked: &HashSet<Cell>,
        threats: &[Cell],
    ) -> Option<Input> {
        let facility = state.layout().facility();
        let player = state.player();
        Direction::ALL
            .into_iter()
            .find(|&dir| {
                player.step(dir).is_some_and(|next| {
                    routable(facility, next)
                        && !blocked.contains(&next)
                        && state.crouch_holds(anchor, next)
                        && conceals_from_all(state, anchor, next, threats)
                })
            })
            .map(Input::Step)
    }

    /// The guards worth taking cover from: every guard the bot perceives — seen or
    /// merely sensed (§9.2) — within this temperament's
    /// [`threat_radius`](Profile::threat_radius), by cell.
    ///
    /// A **sensed** guard counts here, unlike in the rear-strike scan, and the
    /// difference is real rather than an oversight: striking a back needs the guard's
    /// *facing*, which a sensed guard does not give up, while hiding across a bench
    /// needs only where it is standing — which a sensed guard does give up exactly
    /// (§9.2: "the exact cell is known, nothing about where it looks").
    fn nearby_threats(&self, state: &State) -> Vec<Cell> {
        let player = state.player();
        perceived_guard_cells(state)
            .into_iter()
            .filter(|&guard| player.manhattan_distance(guard) <= self.profile.threat_radius)
            .collect()
    }

    /// Spring a takedown, or take a step toward one (§7.2/§155) — the play a
    /// temperament with a [`takedown_reach`](Profile::takedown_reach) buys into, and
    /// nothing at all to one without.
    ///
    /// Two moments, and the first is by far the commoner. **At arm's length**: a
    /// perceived guard is adjacent and the core's own gate is open, so bump it. That
    /// covers both legal angles at once without naming either — the rear blind spot
    /// (§155 carves the three cells at a guard's back out of its cone) and concealment
    /// (§7.2 — a hidden or crouched player is concealed from every viewer, so no cone
    /// reaches them wherever they stand). **Otherwise**: walk to a back, if one is
    /// close enough to be a diversion rather than a hunt.
    ///
    /// The one thing it will not do is strike while somebody is *watching* — not the
    /// target, which by definition is not, but any other guard whose cone is live on
    /// the bot. Read through [`State::guard_detects_now`] rather than off the danger
    /// overlay, because those two differ exactly where this play lives: a cupboard cell
    /// can sit inside a cone and still be perfectly safe, since concealment beats the
    /// cone (§10.3).
    fn strike(
        &self,
        state: &State,
        danger: &HashSet<Cell>,
        blocked: &HashSet<Cell>,
    ) -> Option<Input> {
        if self.profile.takedown_reach == 0 {
            return None;
        }
        let player = state.player();

        // Nobody has eyes on the bot right now — concealment counts, so this is true
        // inside a cupboard even under a cone.
        let unwatched = state
            .guards()
            .iter()
            .all(|g| state.perceive_guard(g).is_none() || !state.guard_detects_now(g));
        if unwatched {
            for dir in Direction::ALL {
                let Some(target) = player.step(dir) else {
                    continue;
                };
                let strikeable = state.guards().iter().any(|g| {
                    g.pos() == target
                        && state.perceive_guard(g).is_some()
                        && !state.guard_detects_now(g)
                });
                if strikeable {
                    return Some(Input::Step(dir));
                }
            }
        }

        // No guard at hand: is one's back within a short walk? A sensed-only guard is
        // no use here — its facing is unknown (§9.2), so where its back is, is too.
        let spots = rear_strike_cells(state, danger, blocked);
        if spots.is_empty() {
            return None;
        }
        // Costed with **no keep-away halo**: you cannot both give a guard a wide berth
        // and walk up behind it, and the halo would price the approach out of every
        // budget. The cone penalty still applies, which is what makes "within budget"
        // mean "reachable without being seen on the way" rather than merely "near".
        let field = cost_field(
            state.layout().facility(),
            &spots,
            blocked,
            danger,
            &[],
            &self.profile,
        );
        if field
            .get(&player)
            .is_none_or(|&cost| cost > u64::from(self.profile.takedown_reach))
        {
            return None;
        }
        self.descend(state, &spots, danger, blocked, APPROACH)
            .map(Input::Step)
    }

    /// Deal with the body **in hand** (§8.3): haul it to a cupboard and stow it, or
    /// let it go. `None` with empty hands, which is the usual case.
    ///
    /// The grab itself is never a decision — stepping off a body's cell takes hold of
    /// it automatically (§8.3/#187) — so a temperament that does not stow cannot
    /// express that by standing still. It has to *act*, by letting go, which is why
    /// [`body_stow_reach`](Profile::body_stow_reach) of zero still reaches this code.
    fn haul(
        &mut self,
        state: &State,
        danger: &HashSet<Cell>,
        blocked: &HashSet<Cell>,
    ) -> Option<Input> {
        let body = state.dragging()?;
        // Hands full at half speed is no way to be hunted, and Run will not stack with
        // a drag (§8.3): drop it and run. Letting go is free (§4.4), so the escape
        // loses nothing but the input.
        if being_hunted(state, danger) {
            return self.let_go(state, body);
        }
        let shelters = self.stow_targets(state);
        if shelters.is_empty() {
            return self.let_go(state, body);
        }
        // Routed like a flight rather than a push — a body is worth hiding, not worth
        // being *seen* hiding — and never through the body itself, which is the step
        // that would drop it (see [`descend_avoiding`](Self::descend_avoiding)).
        self.descend_avoiding(
            state,
            &shelters,
            danger,
            blocked,
            self.profile.flee,
            &[body],
        )
        .map(Input::Step)
        // Nowhere to put it that does not mean walking into it: the geometry has no
        // loop back to the cupboard (a one-wide corridor has none), so this body
        // stays where it falls rather than being shuffled about for the rest of the
        // run.
        .or_else(|| self.let_go(state, body))
    }

    /// Go and pick up a body left on the floor (§7.2/§10.3), for a temperament that
    /// stows. `None` for one that does not, and none when there is nothing worth the
    /// walk.
    ///
    /// Two steps, because the grab is the step *off* a body's cell (#187): get on it,
    /// then leave in the direction of the cupboard, and the pickup rides that step for
    /// free. A body only counts as worth fetching when there is somewhere to *put* it —
    /// hauling one to nowhere is turns spent making the run slower and no quieter.
    fn fetch(
        &self,
        state: &State,
        danger: &HashSet<Cell>,
        blocked: &HashSet<Cell>,
    ) -> Option<Input> {
        if self.profile.body_stow_reach == 0 {
            return None;
        }
        let player = state.player();
        let reach = self.profile.body_stow_reach;
        let shelters = self.stow_targets(state);
        if shelters.is_empty() {
            return None;
        }
        // Already standing on one: any step from here takes hold, so make it the step
        // that starts the haul — toward the cupboard, but never **into** it. Bumping a
        // cupboard with empty hands climbs in to hide (§10.3) rather than stowing, and
        // the body would still be lying on the floor outside.
        let loose = findable_bodies(state);
        if loose.contains(&player) {
            return self
                .descend_avoiding(
                    state,
                    &shelters,
                    danger,
                    blocked,
                    self.profile.flee,
                    &shelters,
                )
                .map(Input::Step);
        }
        let worth: Vec<Cell> = loose
            .into_iter()
            .filter(|&body| !self.abandoned.contains(&body))
            .filter(|&body| player.manhattan_distance(body) <= reach)
            .filter(|&body| {
                shelters
                    .iter()
                    .any(|&h| body.manhattan_distance(h) <= reach)
            })
            .collect();
        if worth.is_empty() {
            return None;
        }
        self.descend(state, &worth, danger, blocked, self.profile.pursue)
            .map(Input::Step)
    }

    /// The cupboards a body could be stowed in from here (§10.3): known, empty and
    /// within this temperament's haul range. Empty for one that never stows.
    fn stow_targets(&self, state: &State) -> Vec<Cell> {
        if self.profile.body_stow_reach == 0 {
            return Vec::new();
        }
        let player = state.player();
        known_hideouts(state)
            .into_iter()
            .filter(|&h| player.manhattan_distance(h) <= self.profile.body_stow_reach)
            .collect()
    }

    /// Let the carried body go where it lies (§8.3): bump the cell it occupies. A
    /// dragged body always sits in the cell the bot just left, so it is always one of
    /// the four neighbours; the release is free (§4.4) and refunds nothing, because
    /// there is nothing to refund.
    ///
    /// The cell is **remembered** ([`abandoned`](Self::abandoned)), so [`fetch`](Self::fetch)
    /// does not walk straight back and pick up the body it has just decided it cannot
    /// place. Without that the two would trade the same corpse back and forth for the
    /// rest of the run, which is a livelock rather than a temperament.
    fn let_go(&mut self, state: &State, body: Cell) -> Option<Input> {
        self.abandoned.insert(body);
        Direction::ALL
            .into_iter()
            .find(|&dir| state.player().step(dir) == Some(body))
            .map(Input::Step)
    }

    /// Pursue the objective — nearest known untaken console, then the exit — or, when
    /// no intel is known yet, explore toward the nearest frontier.
    fn pursue(&mut self, state: &State, danger: &HashSet<Cell>, blocked: &HashSet<Cell>) -> Input {
        let (intent, goals) = if !state.exit_ready() {
            // No intel in hand yet: head for the nearest known console, since a single
            // objective now opens the exit (§10.2 experiment). Every profile grabs the
            // first intel it can reach and leaves — the shortest honest run. Pressing
            // on for a *second* console is a different decision, not a different
            // number, so it is deliberately not a profile field (§13.4): profiles are
            // one policy at different temperatures, never a forked `decide`.
            let known = self.known_intel(state);
            // Nothing seen to head for: sweep the facility until the consoles show.
            if known.is_empty() {
                (Intent::Explore, frontier_cells(state))
            } else {
                (Intent::Pursue, known)
            }
        } else {
            // At least one intel in hand: leave the way we came in (§4.5).
            (Intent::Pursue, exit_cell(state).into_iter().collect())
        };

        // The route this plan would walk, worked out before the cues rather than
        // after: an ability that lands in a cell needs to know where the bot is going,
        // and one that offers a *better way through* needs the router's own costs to
        // weigh itself against (§8.3). Both come off one Dijkstra, shared rather than
        // re-derived.
        let field = self.field(state, &goals, danger, blocked, self.profile.pursue);
        let step = self.step_down(state, &field, danger, blocked, self.profile.pursue, &[]);
        // **No crossing while the sprint is up.** A phased crossing is costed in
        // single steps — in at one turn, out at the next, with a turn of the window
        // spare — and Run moves two cells a step (§8.3), which overshoots the far side
        // and leaves the bot shuffling across the hole until the duration dies inside
        // it. Nothing in the catalogue forbids holding both; what the geometry forbids
        // is *crossing* on them, so the offer is withdrawn rather than the ability.
        let sprinting = matches!(
            state.ability_state(AbilityId::Run),
            AbilityState::Active { .. }
        );
        let crossing = (!sprinting)
            .then(|| {
                crossing(
                    state.layout().facility(),
                    state.memory(),
                    &field,
                    state.player(),
                )
            })
            .flatten();

        // Pushing on is a moment too, and one most of the salvaged tech is *for*
        // (§8.3: bore a shortcut, seal the doors ahead, draw a patrol off the route).
        // There is no cover on offer here, so no refuge to weigh against.
        if let Some(input) = self.cue(state, intent, None, step, crossing) {
            return input;
        }

        // The router cannot plan through a wall, so the crossing the cue bought is
        // walked here: while the phase is up, step into the solid rather than round
        // it. Getting *out* the far side is settled earlier still, before every other
        // plan (§8.3 — a duration that expires inside a solid costs the eject).
        // **Only with enough of the window left to come out the other side.** The
        // crossing is two steps, in and out, so stepping into a solid with one turn
        // left is walking into the eject on purpose (§8.3) — and the turn the bot
        // meant to spend crossing is not always the turn it gets, since anything
        // higher up the ladder (a patrol closing, a bench to duck behind) can take it
        // first. Checking the remaining duration is what makes the crossing safe
        // whoever steals the turn.
        if let AbilityState::Active { remaining } = state.ability_state(AbilityId::Dephase) {
            if remaining >= 2 {
                if let Some((dir, _)) = crossing {
                    return Input::Step(dir);
                }
            }
        }

        let Some(dir) = step else {
            // No safe progress. Standing still next to a patrol is how you get
            // walked into, so if one is close, sidestep to open ground; otherwise
            // hold a beat and let the cone sweep past (waiting also widens the
            // senses, §8.3/§9.1).
            return if nearest_perceived_guard(state)
                .is_some_and(|d| d <= self.profile.proximity_radius)
            {
                retreat_step(state, danger, blocked).map_or(Input::Wait, Input::Step)
            } else {
                Input::Wait
            };
        };

        // Stepping onto a known untaken console is the take — certain, since the
        // touching ring is always in view (§6.2). Bank it so we never route here
        // again (the emptied console stays stamped as terrain).
        if let Some(target) = state.player().step(dir) {
            if self.known_intel(state).contains(&target) {
                self.taken.insert(target);
            }
        }
        Input::Step(dir)
    }

    /// Step out of the solid the bot is phased inside, or `None` when it is not in one
    /// (which is almost always).
    ///
    /// The whole cost of Dephase lives here (§8.3): the walk-through is free while the
    /// duration lasts and brutal the moment it does not, so a bot standing in a wall
    /// has exactly one plan. It leaves by the nearest **walkable** neighbour, which is
    /// the far side it phased in for, or the side it came from if the far side has
    /// gone — either beats the eject.
    fn leave_the_wall(&self, state: &State) -> Option<Input> {
        let player = state.player();
        let facility = state.layout().facility();
        // Inside a solid only. Standing on floor, phased or not, is somebody else's
        // decision.
        if !facility
            .terrain(player)
            .is_some_and(|t| t.blocks_movement())
        {
            return None;
        }
        Direction::ALL
            .iter()
            .find(|&&dir| {
                player
                    .step(dir)
                    .and_then(|cell| facility.terrain(cell))
                    .is_some_and(|t| !t.blocks_movement())
            })
            .map(|&dir| Input::Step(dir))
    }

    /// Put this moment to every held ability's cue (§13.2/#346) and return the
    /// winning bid's input, or `None` to spend the turn stepping or waiting as
    /// usual.
    ///
    /// This is the *only* place the bot decides to press an ability key. What each
    /// ability is for lives in [`crate::cue`], one exhaustively-matched arm apiece,
    /// so a new row in the §8.1 catalog cannot arrive dead by omission — it fails to
    /// compile until somebody says.
    ///
    /// The comparison it does not make: §4.4 says the real question is "is this turn
    /// better spent activating than stepping?", and a step's worth is a cost-field
    /// delta rather than an urge. Weighing the two against each other is fuzzy, and
    /// deliberately left fuzzy — a common currency between them is a much larger
    /// change and probably a worse one.
    fn cue(
        &mut self,
        state: &State,
        intent: Intent,
        refuge: Option<Direction>,
        route: Option<Direction>,
        crossing: Option<(Direction, u64)>,
    ) -> Option<Input> {
        let bid = Moment {
            state,
            intent,
            refuge,
            nearest_guard: nearest_perceived_guard(state),
            route,
            crossing,
        }
        .best(&self.profile)?;
        self.last_bid = Some(bid);
        Some(bid.input)
    }

    /// The intel consoles the bot may head for: seen (in [`State::memory`]) and not
    /// yet emptied. This is the no-cheat gate — a console the bot has never seen is
    /// not a goal, so it cannot route to intel it does not know about (§11.5a).
    fn known_intel(&self, state: &State) -> Vec<Cell> {
        let facility = state.layout().facility();
        let memory = state.memory();
        console_cells(facility)
            .filter(|&cell| memory.contains(cell) && !self.taken.contains(&cell))
            .collect()
    }

    /// Step one cell down the [`cost_field`] toward the nearest of `goals` — always
    /// to the routable neighbour whose cost-to-goal is lowest. Because a Dijkstra
    /// potential has no local minima but its goals, following it never traps the bot
    /// in a two-cell shuffle, however the guard costs pull.
    ///
    /// The [`Descent`] mode shapes the field: `keep_clear` bakes a keep-away cost
    /// around perceived guards (careful routing) and, when `hold_watched`, a step
    /// into a cone from a currently-safe cell is refused (`None`, hold and let it
    /// pass) rather than taken. Returns `None` to hold, or when no route reaches a
    /// goal at all.
    fn descend(
        &self,
        state: &State,
        goals: &[Cell],
        danger: &HashSet<Cell>,
        blocked: &HashSet<Cell>,
        mode: Descent,
    ) -> Option<Direction> {
        self.descend_avoiding(state, goals, danger, blocked, mode, &[])
    }

    /// [`descend`](Self::descend) with a handful of cells barred from **this turn's
    /// step** while left perfectly routable in the field.
    ///
    /// The distinction is the whole point, and it exists for the drag (§8.3). A
    /// carried body is not an obstacle — it is one step behind the player and moves
    /// as the player moves, so the cell it sits on now is clear by the time a route
    /// reaches it — but stepping *onto* it right now is a bump, and a bump into the
    /// body you are carrying **lets it go** (§4.4). Bar it from the field instead and
    /// the router would declare the cupboard unreachable and give up; bar it from the
    /// step alone and the router walks the loop that the manoeuvre actually needs,
    /// because coming back to a cupboard mouth by backtracking walks into the body and
    /// coming back round a square does not.
    fn descend_avoiding(
        &self,
        state: &State,
        goals: &[Cell],
        danger: &HashSet<Cell>,
        blocked: &HashSet<Cell>,
        mode: Descent,
        avoid: &[Cell],
    ) -> Option<Direction> {
        let field = self.field(state, goals, danger, blocked, mode);
        self.step_down(state, &field, danger, blocked, mode, avoid)
    }

    /// The router's own **turns-to-goal potential**: one Dijkstra from `goals`,
    /// shaped by the [`Descent`] mode exactly as [`descend`](Self::descend) shapes it.
    ///
    /// Split out from the descent because it is a fact worth *sharing*: an ability
    /// whose whole question is "would this be a better way through?" — Dephase's
    /// crossing, Pierce Wall's bore (§8.3) — has to weigh a shortcut against the
    /// route the bot would otherwise walk, and re-deriving that inside a cue would be
    /// both a second Dijkstra and a second opinion.
    fn field(
        &self,
        state: &State,
        goals: &[Cell],
        danger: &HashSet<Cell>,
        blocked: &HashSet<Cell>,
        mode: Descent,
    ) -> HashMap<Cell, u64> {
        if goals.is_empty() {
            return HashMap::new();
        }
        let guards = if mode.keep_clear {
            perceived_guard_cells(state)
        } else {
            Vec::new()
        };
        cost_field(
            state.layout().facility(),
            goals,
            blocked,
            danger,
            &guards,
            &self.profile,
        )
    }

    /// One step down a prebuilt [`field`](Self::field) — the second half of the
    /// descent, so a caller holding a field already does not pay for it twice.
    fn step_down(
        &self,
        state: &State,
        field: &HashMap<Cell, u64>,
        danger: &HashSet<Cell>,
        blocked: &HashSet<Cell>,
        mode: Descent,
        avoid: &[Cell],
    ) -> Option<Direction> {
        let player = state.player();
        let mut best: Option<(u64, bool, Direction)> = None;
        for dir in Direction::ALL {
            let Some(next) = player.step(dir) else {
                continue;
            };
            // Blocked cells (aware guards, bodies) are not steps; an unaware guard is
            // left routable, so a step onto one when it blocks the only way is the
            // takedown (§7.2). A goal cell is solid but seeded into the field, so a
            // console or the exit one step away reads cost 0 and is taken.
            if blocked.contains(&next) || avoid.contains(&next) {
                continue;
            }
            let Some(&cost) = field.get(&next) else {
                continue;
            };
            let watched = danger.contains(&next);
            // Strict `<` keeps the first direction in `Direction::ALL` order on a
            // tie, so the choice is deterministic (§12.4).
            if best.is_none_or(|(c, _, _)| cost < c) {
                best = Some((cost, watched, dir));
            }
        }

        let (_, watched, dir) = best?;
        // Safe now, but the only step forward walks into a cone: better to wait it
        // out than to be seen. Once already watched, holding gains nothing, so move.
        if mode.hold_watched && watched && !danger.contains(&player) {
            return None;
        }
        Some(dir)
    }
}

/// The **one-cell crossing** worth phasing through from `from`, if the field knows
/// of one: a direction whose adjacent cell is solid and whose far cell is a routable
/// cell the router already wants, materially closer to the goal than standing here.
///
/// Shared between the cue that presses Dephase and the steps that walk it (§8.3), so
/// the ability can never be pressed for a crossing the policy would then decline to
/// take — the shy-cue failure #347 warns about, in its most literal form.
///
/// **One cell of solid, never two.** The duration is 3 turns: in at 1, out at 2,
/// with a turn in hand. A two-cell run would land on expiry, and a duration that ends
/// inside a solid costs a safety eject plus a stun as long as the throw (§8.3) — the
/// exact trap the cue must not walk into.
fn crossing(
    facility: &Facility,
    memory: &intrusion_core::VisibleSet,
    field: &HashMap<Cell, u64>,
    from: Cell,
) -> Option<(Direction, u64)> {
    let here = *field.get(&from)?;
    let mut best: Option<(Direction, u64)> = None;
    for dir in Direction::ALL {
        let Some(wall) = from.step(dir) else { continue };
        let Some(beyond) = wall.step(dir) else {
            continue;
        };
        // A wall to phase through, and floor the other side. Read as the player reads
        // it (§11.5a): a far side the bot has never seen is not a crossing it knows
        // about, whatever the map says.
        if !facility
            .terrain(wall)
            .is_some_and(|t| t.blocks_movement() && t.blocks_pathing())
        {
            continue;
        }
        if !memory.contains(beyond) {
            continue;
        }
        // **Walkable, not merely wanted.** The field seeds its goals whether or not
        // they can be stood on — a console and the exit are solid (§4.3) — so a wall
        // backing a console has a far side with a cost and no floor. Crossing into it
        // would leave Dephase to expire inside a solid, which is the eject (§8.3),
        // and would have Pierce Wall open a pocket while calling it a route.
        if !facility
            .terrain(beyond)
            .is_some_and(|t| !t.blocks_movement())
        {
            continue;
        }
        let Some(&there) = field.get(&beyond) else {
            continue;
        };
        // Worth the turn only if the router's own cost falls by more than the crossing
        // costs it: one turn to press and two to walk through. The margin is [START].
        let Some(saving) = here.checked_sub(there) else {
            continue;
        };
        if saving < CROSSING_MARGIN {
            continue;
        }
        if best.is_none_or(|(_, held)| saving > held) {
            best = Some((dir, saving));
        }
    }
    best
}

/// How much the router's own cost must fall before a phased crossing is worth it
/// **[START]** — twice the three turns it spends (one to press, two to walk through),
/// so a shortcut has to be a real one and not a tie.
pub(crate) const CROSSING_MARGIN: u64 = 6;

/// Whether a guard currently has the player, or is about to (§7.6). True when a
/// visible guard is actively hunting (chasing or investigating), or when the
/// player stands in a seen guard's cone without being concealed from it — the
/// exposure the danger overlay paints red (§11.5).
fn being_hunted(state: &State, danger: &HashSet<Cell>) -> bool {
    let player = state.player();
    for guard in state.guards() {
        if state.perceive_guard(guard) != Some(GuardPerception::Seen) {
            continue;
        }
        if matches!(
            guard.state(),
            GuardState::Chasing | GuardState::Investigating
        ) {
            return true;
        }
    }
    // Exposed: a seen guard's cone is on the player's own cell and no concealment
    // (hideout, crouch, camouflage) is breaking the line (§11.5, §10.3).
    danger.contains(&player)
        && state.guards().iter().any(|guard| {
            state.perceive_guard(guard) == Some(GuardPerception::Seen)
                && guard.fov().contains(player)
                && !state.concealed_from(guard.pos())
        })
}

/// The **danger overlay** as the player sees it (§11.5): every cell watched by a
/// guard the player can *see*. A sensed-only guard projects no cone (§9.2), so its
/// watch is unknown and never enters this set — exactly what the renderer paints.
fn danger_cells(state: &State) -> HashSet<Cell> {
    let mut cells = HashSet::new();
    for guard in state.guards() {
        if state.perceive_guard(guard) == Some(GuardPerception::Seen) {
            cells.extend(guard.fov().cells());
        }
    }
    cells
}

/// Cupboards the bot must **not** dive into this turn: any hideout cell watched by a
/// guard it can see that is **alerted** (any non-Calm mood). Climbing into a cupboard
/// under such a cone is *witnessed*, and a witness flushes the hidden player straight
/// back out (§15 Q5) — so a cupboard in a hunter's cone is a trap, not a refuge, and a
/// player-honest bot must read that off the danger overlay and route elsewhere. A
/// **Calm** patrol's cone does not make a cupboard a trap (a Calm guard never checks),
/// so its watch is deliberately excluded — that keeps the ordinary "duck past a patrol"
/// cover play (`take_cover`) working. A sensed-only guard projects no cone (§9.2), so
/// its unknown watch never enters this set, exactly as the overlay paints nothing.
fn witnessed_cone_cells(state: &State) -> HashSet<Cell> {
    let mut cells = HashSet::new();
    for guard in state.guards() {
        if state.perceive_guard(guard) == Some(GuardPerception::Seen)
            && guard.state() != GuardState::Calm
        {
            cells.extend(guard.fov().cells());
        }
    }
    cells
}

/// The bodies out on the floor a guard could still **find** (§7.2): every body except
/// the stowed ones (inside a locked cupboard — *gone*, never found). A found body throws
/// its finder into a §7.6 search that checks the cupboards within `SEARCH_RADIUS` of it
/// (§15 Q5), so the flee routines keep clear of hiding within [`BODY_HIDE_CLEARANCE`] of
/// one of these — a body known to the bot is one it dropped and can see on the map.
fn findable_bodies(state: &State) -> Vec<Cell> {
    let facility = state.layout().facility();
    state
        .bodies()
        .iter()
        .map(|body| body.cell())
        .filter(|&at| facility.terrain(at) != Some(Terrain::Hideout))
        .collect()
}

/// Whether diving into the cupboard at `hideout` risks a found-body flush (§15 Q5): true
/// when a findable body lies within [`BODY_HIDE_CLEARANCE`] of it, so a guard that
/// stumbles on that body would search — and open — the cupboard. This is the danger the
/// player reads off the corpse they left, not off a cone (§2.2), so a player-honest bot
/// reads it the same way and hides somewhere the body cannot reach.
fn near_findable_body(bodies: &[Cell], hideout: Cell) -> bool {
    bodies
        .iter()
        .any(|&body| body.sight_distance(hideout) <= BODY_HIDE_CLEARANCE)
}

/// Cells the bot must not step onto: any guard that has already detected the player
/// — bumping an aware guard is a wasted, refused turn (§7.2), whereas an *unaware*
/// one is left out so the takedown stays available.
///
/// **Bodies are deliberately not here** (§7.2/#187). They used to be, on the strength
/// of a comment calling them solid; they have not been since bodies went
/// pickup-on-walk, and routing round one cost the bot the only way to *take hold* of
/// it — the grab is the step **off** a body's cell, so a bot that will not stand on a
/// body can never drag one (#316). The single exception is the door-crush rule, which
/// is core's business and not a routing question.
fn blocked_cells(state: &State) -> HashSet<Cell> {
    let mut cells = HashSet::new();
    for guard in state.guards() {
        // A guard blocks when bumping it would be *refused* (§7.2): it is perceived
        // and its **live** cone would detect the player — the same gate the takedown
        // reads ([`State::guard_detects_now`]), so a guard that stepped adjacent
        // facing the bot this turn is an obstacle, not a free takedown, even before
        // its awareness latch catches up (#183). An unaware guard whose cone is off
        // the player stays a takedown target, left unblocked. A guard the player
        // cannot perceive is unknown, so it cannot be planned around — the bot only
        // avoids what it can see or sense.
        if state.perceive_guard(guard).is_some() && state.guard_detects_now(guard) {
            cells.insert(guard.pos());
        }
    }
    cells
}

/// Unaware guards this temperament **declines** to take down, left blocked so the
/// router waits the patrol out rather than bumping it.
///
/// This was #170's soft-lock guard rail: a takedown drops the body on the guard's own
/// cell, so springing one from a dead end — a cupboard, a one-wide stub — whose *only*
/// way out held that guard walled the mouth and stranded the bot for the run. **That
/// hazard no longer exists.** #187 made a loose body non-solid: the mouth stays
/// walkable, and stepping over it on the way out takes hold of it (§8.3). The rule
/// outlived its reason by four days and went on suppressing every takedown the bot was
/// ever offered — all of them this exact shape, a hidden bot with a patrol on its
/// cupboard door (#316).
///
/// What is left at that mouth is not a hazard but a **choice**: strike the patrol on
/// your doorstep from concealment (§7.2 — a hidden player is concealed from every
/// viewer, so the gate is open), or sit still and let it pass. A choice is a
/// temperament, so it is [`takedown_reach`](Profile::takedown_reach) that answers:
/// zero declines and keeps the old, measured behaviour exactly; anything else leaves
/// the guard available and lets [`strike`](StealthBot::strike) decide.
///
/// Only a *lone* exit ever reached this rule, and that is kept — with a second way
/// out, a declining bot was never going to bump the guard anyway — so it fires solely
/// when the player has exactly one routable, unblocked neighbour and an unaware guard
/// stands on it. A guard the player cannot even perceive is not planned around (the
/// bot avoids only what it can see or sense).
fn declined_takedowns(state: &State, blocked: &HashSet<Cell>, profile: &Profile) -> Vec<Cell> {
    if profile.takedown_reach > 0 {
        return Vec::new(); // this temperament wants the strike
    }
    let facility = state.layout().facility();
    let player = state.player();
    let mut exits = Direction::ALL
        .iter()
        .filter_map(|&d| player.step(d))
        .filter(|&n| routable(facility, n) && !blocked.contains(&n));
    let (Some(mouth), None) = (exits.next(), exits.next()) else {
        return Vec::new(); // no exit, or more than one — never a single-mouth trap
    };
    let sealed = state.guards().iter().any(|g| {
        g.pos() == mouth && state.perceive_guard(g).is_some() && !state.guard_detects_now(g)
    });
    if sealed {
        vec![mouth]
    } else {
        Vec::new()
    }
}

/// The cells a takedown can be **walked to** and sprung from (§7.2/§155): for each
/// guard the player can *see*, the one orthogonal cell in its rear blind spot.
///
/// §155 carves three cells out of a guard's cone — directly behind and the two rear
/// diagonals — but movement is four-way ([`Direction::ALL`]), so only the orthogonal
/// one can be struck *from*: reaching either diagonal and bumping the guard is not a
/// move the game has. A **sensed-only** guard is excluded rather than guessed at: its
/// facing is unknown (§9.2), so where its back is, is unknown too, and a bot that
/// guessed would be walking into cones it cannot see to measure a game it is not
/// playing (§11.5a).
///
/// Filtered to cells that are routable, not blocked, and **not watched by anyone** —
/// standing in one guard's blind spot inside another's cone is not a safe strike, it
/// is a detection with extra steps.
fn rear_strike_cells(state: &State, danger: &HashSet<Cell>, blocked: &HashSet<Cell>) -> Vec<Cell> {
    let facility = state.layout().facility();
    state
        .guards()
        .iter()
        .filter(|g| state.perceive_guard(g) == Some(GuardPerception::Seen))
        .filter_map(|g| g.pos().step(g.facing().opposite()))
        .filter(|&spot| {
            routable(facility, spot) && !danger.contains(&spot) && !blocked.contains(&spot)
        })
        .collect()
}

/// The exit cell — the player's own tunnel, known from the start (§4.5). Found by
/// scanning the always-visible geometry for the one exit tile, so it needs no
/// fog gate: a player knows the way they came in.
fn exit_cell(state: &State) -> Option<Cell> {
    let facility = state.layout().facility();
    all_cells(facility).find(|&cell| facility.terrain(cell) == Some(Terrain::Exit))
}

/// The empty hideouts the bot has seen (§10.3): remembered cupboards ([`State::memory`])
/// not currently holding a guard or body. These are the boltholes the flee routine
/// aims for, and the cupboards a haul stows a body into.
///
/// The body check is stated here rather than inherited from [`blocked_cells`], which no
/// longer carries one (§7.2/#187 — a loose body is not solid). A cupboard is the one
/// place a body still refuses entry, because a stowed body **locks** it: it stops being
/// a hideout at all, so it is neither a bolthole nor somewhere to put a second body.
fn known_hideouts(state: &State) -> Vec<Cell> {
    let facility = state.layout().facility();
    let memory = state.memory();
    let occupied = blocked_cells(state);
    all_cells(facility)
        .filter(|&cell| {
            facility.terrain(cell) == Some(Terrain::Hideout)
                && memory.contains(cell)
                && !occupied.contains(&cell)
                && !state.bodies().iter().any(|body| body.cell() == cell)
        })
        .collect()
}

/// The exploration frontier: every routable cell that borders one the player has
/// never seen (outside [`State::memory`]). Heading for the nearest sweeps the
/// facility's unseen ground into view, which is how the consoles get found.
fn frontier_cells(state: &State) -> Vec<Cell> {
    let facility = state.layout().facility();
    let memory = state.memory();
    all_cells(facility)
        .filter(|&cell| {
            routable(facility, cell)
                && facility
                    .neighbours(cell)
                    .any(|neighbour| !memory.contains(neighbour))
        })
        .collect()
}

/// Back away from the nearest perceived guard: the reachable neighbour that puts the
/// most distance between the player and the closest guard, off watched cells where
/// possible. The last resort when no hideout is within reach.
fn retreat_step(
    state: &State,
    danger: &HashSet<Cell>,
    blocked: &HashSet<Cell>,
) -> Option<Direction> {
    let facility = state.layout().facility();
    let player = state.player();
    let guards = perceived_guard_cells(state);

    let mut best: Option<(bool, u32, Direction)> = None;
    for dir in Direction::ALL {
        let Some(next) = player.step(dir) else {
            continue;
        };
        if !routable(facility, next) || blocked.contains(&next) {
            continue;
        }
        let watched = danger.contains(&next);
        let clearance = guards
            .iter()
            .map(|g| next.manhattan_distance(*g))
            .min()
            .unwrap_or(u32::MAX);
        // Prefer an unwatched cell, then the one that opens the widest gap; ties
        // keep the first `Direction::ALL` order, so the retreat is deterministic.
        let key = (!watched, clearance, dir);
        if best.is_none_or(|(w, c, _)| (key.0, key.1) > (w, c)) {
            best = Some(key);
        }
    }
    best.map(|(_, _, dir)| dir)
}

/// Whether a crouch anchored on `table` would hide a player at `from` from **every**
/// one of `viewers` (§10.3) — the question the duck and the crouch-walk both ask, put
/// to the core's own geometry ([`State::crouch_would_conceal`]) rather than re-derived
/// here (§13.2: a private copy of a game rule is how the bot's metrics quietly stop
/// describing this game).
///
/// *Every* viewer, deliberately: concealment from one of two patrols is not cover, it
/// is a coin toss. With no viewers at all it is vacuously true, which is why both
/// callers check for a threat before asking.
fn conceals_from_all(state: &State, table: Cell, from: Cell, viewers: &[Cell]) -> bool {
    viewers
        .iter()
        .all(|&viewer| state.crouch_would_conceal(table, from, viewer))
}

/// The Manhattan distance to the nearest guard the player can perceive (seen or
/// sensed), or `None` when none is in reach — the gap the flee routine reads to
/// decide whether it can afford a turn spent activating Run.
fn nearest_perceived_guard(state: &State) -> Option<u32> {
    let player = state.player();
    perceived_guard_cells(state)
        .into_iter()
        .map(|cell| player.manhattan_distance(cell))
        .min()
}

/// The cells of every guard the player perceives, seen or sensed (§9.2).
fn perceived_guard_cells(state: &State) -> Vec<Cell> {
    state
        .guards()
        .iter()
        .filter(|g| state.perceive_guard(g).is_some())
        .map(|g| g.pos())
        .collect()
}

/// A Dijkstra cost-field from `goals` outward: each routable cell's least total cost
/// to reach a goal, with a guard's cone ([`Profile::watched_penalty`]) and its keep-away
/// halo ([`proximity_penalty`]) folded into the cost of *entering* each cell. Following
/// this field downhill is the bot's routing — it threads the cheapest safe way to a
/// goal, and, being a true potential, offers no local minimum to get stuck in.
///
/// The goal cells seed the field at 0 even when solid — a console or the exit is a
/// cell you *bump*, reached though not entered (§4.3) — while expansion only ever
/// steps *through* routable cells, never `blocked` ones. The heap is ordered by
/// `(cost, y, x)`, so ties resolve in a fixed cell order and the field is
/// reproducible (§12.4).
fn cost_field(
    facility: &Facility,
    goals: &[Cell],
    blocked: &HashSet<Cell>,
    danger: &HashSet<Cell>,
    guards: &[Cell],
    profile: &Profile,
) -> HashMap<Cell, u64> {
    let mut cost: HashMap<Cell, u64> = HashMap::new();
    // Min-heap on cost, tie-broken by cell order for determinism.
    let mut heap: BinaryHeap<Reverse<(u64, u32, u32)>> = BinaryHeap::new();
    let mut seeds: Vec<Cell> = goals.to_vec();
    seeds.sort_unstable_by_key(|c| (c.y, c.x));
    seeds.dedup();
    for goal in seeds {
        if cost.insert(goal, 0).is_none() {
            heap.push(Reverse((0, goal.y, goal.x)));
        }
    }
    while let Some(Reverse((here, y, x))) = heap.pop() {
        let cell = Cell::new(x, y);
        if here > cost[&cell] {
            continue; // a cheaper route to `cell` was already settled
        }
        for dir in Direction::ALL {
            let Some(neighbour) = cell.step(dir) else {
                continue;
            };
            if !routable(facility, neighbour) || blocked.contains(&neighbour) {
                continue;
            }
            // The price of *entering* the neighbour: one step, plus a cone's weight
            // and the keep-away halo of any nearby guard.
            let watched = if danger.contains(&neighbour) {
                profile.watched_penalty
            } else {
                0
            };
            let entry = 1 + watched + proximity_penalty(neighbour, guards, profile);
            let next = here + entry;
            if cost.get(&neighbour).is_none_or(|&old| next < old) {
                cost.insert(neighbour, next);
                heap.push(Reverse((next, neighbour.y, neighbour.x)));
            }
        }
    }
    cost
}

/// Whether the player can move *through* `cell` when routing — the core's own rule
/// ([`Terrain::routes_player`], §10.3), never a second copy of it here.
///
/// The bot plans on the player's channels so its metrics describe *this* game
/// (§13.2/§13.4); a private terrain table is exactly how that quietly stops being
/// true. This wrapper adds only the off-grid case: a cell with no terrain is outside
/// the facility, so nothing routes through it.
fn routable(facility: &Facility, cell: Cell) -> bool {
    facility.terrain(cell).is_some_and(Terrain::routes_player)
}

/// Every in-bounds cell of the facility, in row-major order — the deterministic
/// sweep the terrain scans (exit, consoles, hideouts, frontier) share.
fn all_cells(facility: &Facility) -> impl Iterator<Item = Cell> + '_ {
    let (width, height) = (facility.width(), facility.height());
    (0..height).flat_map(move |y| (0..width).map(move |x| Cell::new(x, y)))
}

/// The console cells stamped into the facility (§10.3) — the intel terminals, taken
/// or not. The bot gates these through [`State::memory`] to know which it has seen.
fn console_cells(facility: &Facility) -> impl Iterator<Item = Cell> + '_ {
    all_cells(facility).filter(|&cell| facility.terrain(cell) == Some(Terrain::Console))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::boot;
    use crate::{run_batch, run_one, RunOutcome, UsageHistogram, Verb, DEFAULT_INPUT_CAP};
    use intrusion_core::{Loadout, Outcome};

    /// #276: the bot routes by **the core's rule**, never a table of its own.
    ///
    /// It used to hold a private `matches!` allow-list — which meant a new
    /// [`Terrain`] compiled silently as unroutable, and it had already fallen a
    /// variant behind (§10.7 duct entries). The bot plans on the player's own
    /// channels so its metrics describe *this* game (§13.2/§13.4); a second terrain
    /// table is exactly how that quietly stops being true.
    ///
    /// Swept over a whole generated facility, so it runs against the §10.3 table as
    /// generation actually stamps it. Reintroducing a local allow-list here would
    /// have to match the core's answer on every cell of a real level.
    #[test]
    fn the_bot_routes_by_the_cores_rule_not_its_own() {
        let (state, _) = boot(4242);
        let f = state.layout().facility();
        let mut seen: Vec<Terrain> = Vec::new();
        for y in 0..f.height() {
            for x in 0..f.width() {
                let cell = Cell::new(x, y);
                let t = f.terrain(cell).expect("every in-bounds cell has terrain");
                if !seen.contains(&t) {
                    seen.push(t);
                }
                assert_eq!(
                    routable(f, cell),
                    t.routes_player(),
                    "{t:?} at {cell:?}: the bot's routing must be the core's",
                );
            }
        }
        // The sweep only means something if it met the interesting kinds — a level of
        // nothing but floor and wall would pass vacuously.
        for t in [
            Terrain::Floor,
            Terrain::Wall,
            Terrain::DoorHinge,
            Terrain::DoorPanelClosed,
            Terrain::Hideout,
            Terrain::PartialCover,
            Terrain::Console,
            // The comms console (§7.7) is a *distinct* kind from the intel console, so
            // the bot's objective scan can never mistake one for the other — and its
            // routing must agree with the core's on it like any other solid usable.
            Terrain::CommsConsole,
            Terrain::Exit,
        ] {
            assert!(seen.contains(&t), "seed 4242 stamps no {t:?} to check");
        }
        // The wrapper's own contribution: off-grid is not routable, whatever the
        // terrain table says.
        assert!(
            !routable(f, Cell::new(9_999, 9_999)),
            "a cell outside the facility routes nowhere",
        );
    }

    /// §10.7, stated deliberately rather than left to silence: the bot **cannot**
    /// route through a duct entry, even though the player can enter one.
    ///
    /// Climbing in is a mode change into the crawlspace — movement confined to the
    /// duct's recorded path, perception degraded — not a step a plain floor route can
    /// take, and the bot has no crawl policy at all. Teaching it to use ducts is its
    /// own piece of work; this test is what makes the current answer a decision
    /// rather than the old allow-list's silence.
    #[test]
    fn the_bot_does_not_route_through_a_duct_entry() {
        // Sweep seeds until one generates a duct — not every level carries one.
        let entry = (0..40).find_map(|seed| {
            let (state, _) = boot(seed);
            let entry = state.layout().ducts().first()?.entries()[0];
            Some((state, entry))
        });
        let Some((state, entry)) = entry else {
            panic!("no seed in 0..40 generated a duct to check");
        };
        let f = state.layout().facility();
        assert_eq!(
            f.terrain(entry),
            Some(Terrain::DuctEntry),
            "an entry cell is stamped as one",
        );
        assert!(
            !routable(f, entry),
            "a duct is not a through-route for the bot (§10.7)",
        );
        assert!(
            !Terrain::DuctEntry.routes_player(),
            "and the core says the same, so the two cannot drift apart",
        );
    }

    /// §12.4: the same `(seed, profile)` under the bot produces byte-identical
    /// rows, twice. The bot carries its own state (taken consoles, cover timers),
    /// so this pins that none of it leaks non-determinism into the run — and it
    /// sweeps **every** shipped profile (#198), not just the baseline, since a
    /// temperament is only a regression instrument if it reproduces.
    #[test]
    fn the_bot_is_deterministic_per_seed_and_profile() {
        for profile in Profile::ALL {
            for seed in [0, 7, 200] {
                let play = || {
                    run_one(seed, &mut StealthBot::with_profile(profile), 300).expect("generates")
                };
                let (a, b) = (play(), play());
                assert_eq!(a, b, "{} seed {seed}: a bot run reproduces", profile.name);
                assert_eq!(
                    a.to_json_line(),
                    b.to_json_line(),
                    "{} seed {seed}: identical bytes",
                    profile.name,
                );
                assert_eq!(
                    a.profile,
                    Some(profile.name),
                    "seed {seed}: the row names the temperament that played it",
                );
            }
        }
    }

    /// #198's behaviour-preservation clause: the [`Profile::BASELINE`] row of
    /// numbers **is** the constants the bot carried before the seam existed, so
    /// every metric captured under it stays comparable with the batches that came
    /// before. Asserted as byte-identical rows between the default bot and one
    /// explicitly given the baseline profile, over a spread of seeds — the numbers
    /// are pinned by the profile literal, and this pins that the policy actually
    /// reads them rather than a stray leftover constant.
    #[test]
    fn the_baseline_profile_is_the_default_bot() {
        assert_eq!(StealthBot::new().profile(), Profile::BASELINE);
        for seed in 30..40 {
            let default = run_one(seed, &mut StealthBot::new(), 300).expect("generates");
            let explicit = run_one(seed, &mut StealthBot::with_profile(Profile::BASELINE), 300)
                .expect("generates");
            assert_eq!(
                default.to_json_line(),
                explicit.to_json_line(),
                "seed {seed}: the baseline profile must reproduce today's bot",
            );
        }
    }

    /// **#346's behaviour-preservation clause, pinned rather than eyeballed.**
    ///
    /// Lifting Run and Camouflage out of hard-coded `match` arms and into cues
    /// ([`crate::cue`]) is worth nothing if it moved the bot: the whole reason the
    /// seam lands behaviour-preserving is so that the *interesting* diffs — the
    /// verbs the bot has never pressed (#347) — arrive one at a time with their own
    /// measurable delta. A seam that quietly retuned the two cues it replaced would
    /// make every one of those deltas unattributable.
    ///
    /// So this pins the runs themselves, per profile: the ending, its turn count,
    /// and the **exact sequence of ability activations** the bot issued, spelled in
    /// the replay script's letters (§12.4). Nothing else in the suite covers this —
    /// the batch under the sim's bare loadout never presses Camouflage at all (it is
    /// not held), so the cloak cue's rewrite would have gone unmeasured. The loadout
    /// here grants it, and grants **Decoy** alongside it: an ability with no cue yet
    /// must be inert, not merely unused, and holding one is how that is asserted.
    ///
    /// The numbers are `[START]` in the sense that any deliberate change to the bot
    /// moves them — that is what makes them useful. Update them *with* the change
    /// and say what moved, never to make a red test green.
    ///
    /// **A change to *generation* moves them too**, and #361 did: a cupboard now
    /// needs solid back diagonals, so these twelve seeds build different facilities
    /// and the bot's identical policy meets different levels in them. Rows were
    /// regenerated there — the cue seam itself is untouched, which is why the
    /// *shape* of the batch (endings mixed, the cloak pressed) is what carries the
    /// assertion when the levels underneath it move.
    ///
    /// **#347 moved every profile**, and that is the ticket landing rather than a
    /// regression: the batch grants Decoy, so writing its cue is *supposed* to show
    /// up here as `d` presses and the runs they change. Read the diff as the cue's
    /// first evidence — `baseline 3` was the batch's lone stall (`playing 1000`) and
    /// now finishes, while several runs that won now lose. Neither is a verdict:
    /// twelve seeds are a pin, not a balance signal (§13.4), and the measurement that
    /// carries the ticket is the 100-seed with/without batch recorded in
    /// `docs/stats/abilities/decoy.md`. Its rows were regenerated *on top of* #379's
    /// crouch below, so the batch carries both changes at once.
    ///
    /// **#379 moved exactly one row of the forty-eight** — `cautious 5`, from
    /// `won 217 cr` to `won 343 crr` — and that narrowness is the finding rather than
    /// luck. The crouch (§10.3) is the one behaviour here that *every* profile carries,
    /// so unlike #316 there is no reach of zero making the avoidance-first blocks
    /// unchanged by construction; they are unchanged because the pose is rare and
    /// cheap. On seed 5 the cautious bot ducks behind a bench instead of pressing on,
    /// the patrol passes differently, and it wins later by a longer route. Note the
    /// crouch spells no letter either: ducking is a bump (§4.3), like the takedown and
    /// the grab, so it shows in the turn counts and not in the script.
    ///
    /// **#311 moved seven rows of the forty-eight**, and every one of them is the
    /// facility alert ladder (§7.3) doing what it was built to do: from rung 1 a Calm
    /// guard's patrol dwell drops from 3–7 turns to 1–3, so on any seed where the bot
    /// was seen or left a post silent, the patrols it walks past are on a different
    /// rhythm from that turn on. The bot's policy is untouched — it does not read the
    /// rung at all (#198) — so these are the same decisions meeting a facility that
    /// reacts. Net one win fewer (28 → 27), with the movement in both directions:
    /// `baseline 11` turns a 648-turn loss into a 253-turn win, while `cautious 11`
    /// and `aggressive 9` lose runs they used to win. Twelve seeds are a pin, not a
    /// balance signal (§13.4); the 100-seed batch is what judges the ladder.
    ///
    /// **#316 moved the striking half and left the rest alone**, which is the whole
    /// point of putting the takedown behind [`Profile::takedown_reach`]. The
    /// `baseline` and `cautious` blocks below are **byte-for-byte what they were
    /// before that ticket** — the strongest form of its "the cautious baseline is
    /// unchanged" criterion, since a profile with a reach of zero declines the verb
    /// and never reaches a line of the new code. `aggressive` moved because it now
    /// takes the strikes it walks past, and `careless` is new. Note the script letters
    /// spell *activations* only: a takedown, a grab and a stow are steps (§7.2/§8.3),
    /// so they leave no letter here and are asserted in
    /// [`the_striking_profiles_work_the_body_chain`] instead.
    #[test]
    fn the_cue_seam_reproduces_the_hardcoded_bots_runs() {
        const PINNED: [&str; 48] = [
            "baseline 0 won 63 ",
            "baseline 1 won 355 rrrrdcr",
            "baseline 2 lost 220 r",
            "baseline 3 won 241 rdr",
            "baseline 4 won 56 ",
            "baseline 5 lost 119 rd",
            "baseline 6 won 137 ",
            "baseline 7 won 96 ",
            "baseline 8 lost 51 ",
            "baseline 9 lost 61 rdr",
            "baseline 10 lost 40 rc",
            "baseline 11 won 253 crrcrrrr",
            "cautious 0 won 67 ",
            "cautious 1 lost 165 rdrdr",
            "cautious 2 lost 529 rrrrdrdr",
            "cautious 3 lost 50 r",
            "cautious 4 won 154 rdcrd",
            "cautious 5 won 363 crdrr",
            "cautious 6 won 157 ",
            "cautious 7 won 96 ",
            "cautious 8 lost 88 r",
            "cautious 9 lost 242 rdrrd",
            "cautious 10 lost 94 rd",
            "cautious 11 lost 724 crdrrdrrrrdrdrd",
            "aggressive 0 won 63 c",
            "aggressive 1 won 179 rrdc",
            "aggressive 2 won 224 ",
            "aggressive 3 lost 118 rdr",
            "aggressive 4 won 60 ",
            "aggressive 5 lost 212 rrcrdrrc",
            "aggressive 6 lost 77 r",
            "aggressive 7 won 100 ",
            "aggressive 8 won 99 ",
            "aggressive 9 lost 249 rdrrdrrr",
            "aggressive 10 lost 33 rc",
            "aggressive 11 lost 28 rdc",
            "careless 0 won 66 c",
            "careless 1 won 179 rrdc",
            "careless 2 won 215 c",
            "careless 3 won 222 rdrccr",
            "careless 4 lost 91 crcrd",
            "careless 5 won 256 rrcrdrrrcr",
            "careless 6 lost 77 r",
            "careless 7 won 100 ",
            "careless 8 won 99 ",
            "careless 9 won 144 rdcrcrr",
            "careless 10 lost 33 rc",
            "careless 11 won 245 rcrdcr",
        ];

        let mut played = Vec::new();
        // The activation letters alone, kept apart from the formatted rows so the
        // cloak check below counts presses and not the letters of a profile's name.
        let mut activations: Vec<String> = Vec::new();
        for profile in Profile::ALL {
            for seed in 0..12 {
                let (state, _) = boot(seed);
                let mut state = state.with_loadout(
                    intrusion_core::Loadout::innate()
                        .with(AbilityId::Camouflage)
                        .with(AbilityId::Decoy),
                );
                let mut bot = StealthBot::with_profile(profile);
                let mut pressed = String::new();
                for _ in 0..DEFAULT_INPUT_CAP {
                    if state.outcome() != Outcome::Playing {
                        break;
                    }
                    let input = bot.decide(&state);
                    if let Input::Activate(id) = input {
                        pressed.push(id.script_letter());
                    }
                    state.step(input);
                }
                let ending = match state.outcome() {
                    Outcome::Playing => "playing",
                    Outcome::Won => "won",
                    Outcome::Lost => "lost",
                };
                played.push(format!(
                    "{} {seed} {ending} {} {pressed}",
                    profile.name,
                    state.turn(),
                ));
                activations.push(pressed);
            }
        }
        assert_eq!(
            played, PINNED,
            "the cue seam changed how the bot plays — see this test's doc comment",
        );

        // The pin only means something if the cloak cue is actually exercised by it:
        // a batch that never presses Camouflage would pin the Run cue alone and call
        // the rewrite proven.
        let cloak = AbilityId::Camouflage.script_letter();
        let cloaked = activations
            .iter()
            .filter(|pressed| pressed.contains(cloak))
            .count();
        assert!(
            cloaked >= 3,
            "only {cloaked} pinned runs press the cloak — this batch would not \
             catch a change to its cue",
        );

        // Same demand of the decoy (#347): the loadout grants it, so a pin where
        // nobody presses it would be pinning the fake's *absence* and calling the cue
        // covered.
        let fake = AbilityId::Decoy.script_letter();
        let decoyed = activations
            .iter()
            .filter(|pressed| pressed.contains(fake))
            .count();
        assert!(
            decoyed >= 3,
            "only {decoyed} pinned runs press the decoy — this batch would not \
             catch a change to its cue",
        );
    }

    /// **Every decoy the bot presses is pressed at a guard that has lost it** — the
    /// §8.3 rule *"draws Investigating, not Chasing"*, which #347 names as a cue bug
    /// rather than a tuning question, checked over real play instead of a fixture.
    ///
    /// Two halves, because the rule has two: nobody's cone may be live on the player
    /// at the moment of the press (a guard that has you is coming to the real
    /// intruder, and the fake beside you competes with the genuine article), and
    /// somebody must actually be searching, or the fake is bought for a facility that
    /// is not looking for anybody.
    #[test]
    fn every_decoy_the_bot_drops_is_dropped_at_a_search() {
        let mut dropped = 0;
        for seed in 0..40 {
            let (state, _) = boot(seed);
            let mut state = state.with_loadout(Loadout::innate().with(AbilityId::Decoy));
            let mut bot = StealthBot::with_profile(Profile::BASELINE);
            for _ in 0..DEFAULT_INPUT_CAP {
                if state.outcome() != Outcome::Playing {
                    break;
                }
                let input = bot.decide(&state);
                if input == Input::Activate(AbilityId::Decoy) {
                    assert!(
                        !state.guards().iter().any(|g| state.guard_detects_now(g)),
                        "seed {seed}: dropped a decoy while a guard had the player — \
                         a decoy draws Investigating, never Chasing (§8.3)",
                    );
                    assert!(
                        state
                            .guards()
                            .iter()
                            .any(|g| state.perceive_guard(g).is_some()
                                && matches!(
                                    g.state(),
                                    GuardState::Alerted
                                        | GuardState::Investigating
                                        | GuardState::Responding
                                )),
                        "seed {seed}: dropped a decoy with nobody searching — there \
                         was no hunt to redirect (§8.3)",
                    );
                    dropped += 1;
                }
                state.step(input);
            }
        }
        assert!(
            dropped > 0,
            "no decoy in 40 seeds — this test would prove nothing",
        );
    }

    /// **Every autodoors press is a press with a door on the way out** — §8.3's *"a
    /// door in your path… shuts behind you"*, which is the whole flight tool (§7.6).
    /// A press on open floor would spend the turn and a 40-turn cooldown on a window
    /// that closes nothing, so the cue's job is exactly this precondition.
    #[test]
    fn every_autodoors_press_has_a_door_on_the_route() {
        let mut pressed = 0;
        for seed in 0..40 {
            let (state, _) = boot(seed);
            let mut state = state.with_loadout(Loadout::innate().with(AbilityId::Autodoors));
            let mut bot = StealthBot::with_profile(Profile::BASELINE);
            for _ in 0..DEFAULT_INPUT_CAP {
                if state.outcome() != Outcome::Playing {
                    break;
                }
                let input = bot.decide(&state);
                if input == Input::Activate(AbilityId::Autodoors) {
                    // The cue bids off the step the *plan* would take, which cannot be
                    // read back off the state — so assert the fact that makes the
                    // press worth its turn: a door is adjacent to be walked through.
                    let doors = Direction::ALL
                        .iter()
                        .filter_map(|&dir| state.player().step(dir))
                        .filter(|&cell| {
                            matches!(
                                state.layout().facility().terrain(cell),
                                Some(Terrain::DoorPanelClosed | Terrain::DoorPanelOpen)
                            )
                        })
                        .count();
                    assert!(
                        doors > 0,
                        "seed {seed}: opened the autodoors with no door to walk \
                         through — the window would shut nothing (§8.3)",
                    );
                    pressed += 1;
                }
                state.step(input);
            }
        }
        assert!(
            pressed > 0,
            "no autodoors in 40 seeds — this test would prove nothing",
        );
    }

    /// **Every confusion is fired in a panic, at somebody it actually catches** —
    /// §8.3's *"a costed panic-buy of time, not a kill"*. Two facts per press: the bot
    /// was being hunted, and at least one guard stood inside the clamped blast.
    ///
    /// The second is core's own precondition (a firing that catches nobody is
    /// `Unusable`, §4.4's free no-op), asserted here anyway — it is the difference
    /// between a cue that reads the blast and one that presses hopefully and lets the
    /// refusal absorb it.
    #[test]
    fn every_confusion_is_fired_at_a_guard_it_catches() {
        let mut fired = 0;
        for seed in 0..40 {
            let (state, _) = boot(seed);
            let mut state = state.with_loadout(Loadout::innate().with(AbilityId::Confusion));
            let mut bot = StealthBot::with_profile(Profile::BASELINE);
            for _ in 0..DEFAULT_INPUT_CAP {
                if state.outcome() != Outcome::Playing {
                    break;
                }
                let danger = danger_cells(&state);
                let hunted = being_hunted(&state, &danger);
                let input = bot.decide(&state);
                if input == Input::Activate(AbilityId::Confusion) {
                    assert!(
                        hunted,
                        "seed {seed}: fired confusion without being hunted — the \
                         longest cooldown in the catalog is a panic-buy (§8.3)",
                    );
                    let blast = state.confusion_blast();
                    assert!(
                        state.guards().iter().any(|g| blast.contains(g.pos())),
                        "seed {seed}: fired confusion at nobody — the blast catches \
                         no guard and the press is a free no-op (§8.3/§4.4)",
                    );
                    fired += 1;
                }
                state.step(input);
            }
        }
        assert!(
            fired > 0,
            "no confusion in 40 seeds — this test would prove nothing",
        );
    }

    /// **The bot never gets caught in a wall it phased into** — the risk #347 names
    /// as Dephase's own: a duration that expires inside a solid costs a safety eject
    /// plus a stun as long as the throw was deep (§8.3), and a bot that phases
    /// casually will find it.
    ///
    /// The eject is the *only* thing in the game that stuns the player
    /// (`phase_eject_stun`, core's `state/abilities.rs`), which makes `stunned() == 0`
    /// over a batch an exact statement that it never fired — a much sharper assertion
    /// than counting crossings. Two policies hold it up together: the cue only wants a
    /// **one-cell** crossing (in at turn 1, out at 2, of a 3-turn duration), and
    /// leaving the wall outranks every other plan while the bot is in one.
    #[test]
    fn the_bot_is_never_ejected_from_a_wall_it_phased_into() {
        let mut crossings = 0;
        for seed in 0..40 {
            for profile in Profile::ALL {
                let (state, _) = boot(seed);
                let mut state = state.with_loadout(Loadout::innate().with(AbilityId::Dephase));
                let mut bot = StealthBot::with_profile(profile);
                for _ in 0..DEFAULT_INPUT_CAP {
                    if state.outcome() != Outcome::Playing {
                        break;
                    }
                    let input = bot.decide(&state);
                    if input == Input::Activate(AbilityId::Dephase) {
                        crossings += 1;
                    }
                    state.step(input);
                    assert_eq!(
                        state.stunned(),
                        0,
                        "seed {seed} ({}): stunned, so a phase expired inside a solid \
                         — the safety eject is the one thing Dephase's cue must never \
                         walk into (§8.3)",
                        profile.name,
                    );
                }
            }
        }
        assert!(
            crossings > 0,
            "no phase in 40 seeds × 4 profiles — this test would prove nothing",
        );
    }

    /// **Every bore opens a route, never a pocket, and never as an escape** — the
    /// discipline #347 asks of Pierce Wall's cue, since a budget of three spent on
    /// the first legal wall *"makes the histogram look healthy while measuring
    /// nothing"*.
    ///
    /// Two checks per press, both re-derived from the state rather than taken from
    /// the cue's word for it: the bot was not being hunted (a hole conceals nothing,
    /// §8.3), and the cell beyond the bored wall is floor the bot has already seen —
    /// which is what makes it a way *through* rather than the one-cell pocket a
    /// two-thick wall would open.
    #[test]
    fn every_bore_opens_a_route_the_bot_has_seen() {
        let mut bored = 0;
        for seed in 0..40 {
            for profile in Profile::ALL {
                let (state, _) = boot(seed);
                let mut state = state.with_loadout(Loadout::innate().with(AbilityId::PierceWall));
                let mut bot = StealthBot::with_profile(profile);
                for _ in 0..DEFAULT_INPUT_CAP {
                    if state.outcome() != Outcome::Playing {
                        break;
                    }
                    let danger = danger_cells(&state);
                    let hunted = being_hunted(&state, &danger);
                    let input = bot.decide(&state);
                    if input == Input::Activate(AbilityId::PierceWall) {
                        assert!(
                            !hunted,
                            "seed {seed} ({}): bored a wall while hunted — a hole is \
                             not a cupboard and conceals nothing (§8.3)",
                            profile.name,
                        );
                        let target = state.bore_target().expect("the press must be legal");
                        let dir = Direction::ALL
                            .iter()
                            .find(|&&d| state.player().step(d) == Some(target))
                            .expect("the bore target is one of the four neighbours");
                        let beyond = target.step(*dir).expect("a bore is never the shell");
                        assert!(
                            state.memory().contains(beyond)
                                && state
                                    .layout()
                                    .facility()
                                    .terrain(beyond)
                                    .is_some_and(|t| !t.blocks_movement()),
                            "seed {seed} ({}): bored into {beyond:?}, which is not \
                             seen floor — that is a pocket, not a route (§8.3)",
                            profile.name,
                        );
                        bored += 1;
                    }
                    state.step(input);
                }
            }
        }
        assert!(
            bored > 0,
            "no bore in 40 seeds × 4 profiles — this test would prove nothing",
        );
    }

    /// **Every lockdown seals something, and never the door the bot is about to walk
    /// through** — §8.3's own warning that *"a lockdown fired across a route you still
    /// have to travel is a real mistake"*, since your own lock is never refused but
    /// bumping it open costs the turn and leaves the door standing open.
    #[test]
    fn every_lockdown_seals_doors_that_are_not_on_the_way_out() {
        let mut sealed = 0;
        for seed in 0..40 {
            let (state, _) = boot(seed);
            let mut state = state.with_loadout(Loadout::innate().with(AbilityId::Lockdown));
            let mut bot = StealthBot::with_profile(Profile::BASELINE);
            for _ in 0..DEFAULT_INPUT_CAP {
                if state.outcome() != Outcome::Playing {
                    break;
                }
                let input = bot.decide(&state);
                if input == Input::Activate(AbilityId::Lockdown) {
                    assert!(
                        !state.lockdown_doors().is_empty(),
                        "seed {seed}: fired a lockdown with no door in reach — a \
                         window bought to seal nothing (§8.3)",
                    );
                    // The cue bids off the step the *plan* would take, which cannot be
                    // read back off the state. What can: the bot is not standing in a
                    // doorway it is mid-way through, which is the same mistake in its
                    // sharpest form.
                    assert!(
                        !matches!(
                            state.layout().facility().terrain(state.player()),
                            Some(Terrain::DoorPanelOpen)
                        ),
                        "seed {seed}: sealed the doors while standing in a doorway — \
                         that is a route the bot still has to travel (§8.3)",
                    );
                    sealed += 1;
                }
                state.step(input);
            }
        }
        assert!(
            sealed > 0,
            "no lockdown in 40 seeds — this test would prove nothing",
        );
    }

    /// The profiles are **distinguishable** over a batch — a shape assertion, never
    /// a leaderboard (§13.4). Three directions the temperaments are built to differ
    /// in, checked over the same seeds so the facility is held fixed:
    ///
    /// - **`cautious` is seen less often, per turn it plays.** Rate, not raw count:
    ///   waiting a sweep out costs turns, so the careful temperament racks up a
    ///   longer run and would lose a raw-total comparison it is actually winning.
    ///   `aggressive` routes with `hold_watched: false` — it walks a watched cell
    ///   rather than waiting — and gives a third of the berth, so being seen more
    ///   often is the *cost* of its temperament, not a verdict on it.
    /// - **`cautious` spends a bigger share of its turns waiting.** The direct
    ///   signature of "ducks into cover early and waits long" against "hides late
    ///   and briefly".
    /// - **The two play differently at all** — their usage histograms are not the
    ///   same numbers. Identical histograms would mean the profile seam changed
    ///   nothing, and the batch would be measuring one bot twice.
    ///
    /// Deliberately loose and direction-only, in the spirit of the mixed-outcome
    /// test: never "cautious wins more" (§13.4 — a profile is a temperament, not a
    /// better player, and on this batch the win rates are close enough that either
    /// could lead).
    #[test]
    fn the_profiles_play_the_same_seeds_differently() {
        let seeds = 30..70;
        let batch = |profile: Profile| {
            crate::Summary::of(
                &run_batch(seeds.clone(), DEFAULT_INPUT_CAP, move |_| {
                    StealthBot::with_profile(profile)
                })
                .expect("generates"),
            )
        };
        let cautious = batch(Profile::CAUTIOUS);
        let aggressive = batch(Profile::AGGRESSIVE);

        let seen_per_turn = |s: &crate::Summary| s.detections as f64 / s.total_turns as f64;
        assert!(
            seen_per_turn(&cautious) <= seen_per_turn(&aggressive),
            "the cautious profile was seen more often per turn than the aggressive \
             one ({:.4} vs {:.4}) — the temperaments are not what they claim",
            seen_per_turn(&cautious),
            seen_per_turn(&aggressive),
        );

        let waiting =
            |s: &crate::Summary| f64::from(s.usage.count(Verb::Wait)) / s.total_turns as f64;
        assert!(
            waiting(&cautious) > waiting(&aggressive),
            "the cautious profile did not wait more than the aggressive one \
             ({:.4} vs {:.4}) — cover patience is its whole signature",
            waiting(&cautious),
            waiting(&aggressive),
        );

        assert_ne!(
            cautious.usage, aggressive.usage,
            "the two temperaments spent their turns identically — the profile \
             seam is not reaching the policy",
        );
    }

    /// **#316: the §13.2 takedown and bodies rows have a live source.**
    ///
    /// Both rows read a flat zero on every batch ever captured, so nothing in the
    /// harness exercised §7.2 takedowns, §8.3 dragging, §10.3 stowing or §7.3's radio
    /// clock — the most-churned code in the repo, and a regression anywhere in it
    /// would have moved no metric at all. This is the test that stops that being true
    /// again, and it asserts the split the temperaments were built around:
    ///
    /// - the **declining** profiles land exactly zero, which is `takedown_reach: 0`
    ///   working rather than an opportunity that never came (§13.3 — a cautious bot
    ///   reporting no takedowns is correct behaviour, not a defect);
    /// - the **striking** ones land some, from the core's own gate and no other —
    ///   the bot never re-implements a precondition, it bumps and the game rules;
    /// - `aggressive` **grabs** bodies, so the drag half of the chain runs, and
    ///   **stows** some of them, so §10.3's deposit-and-lock runs too;
    /// - `careless` gets bodies **found**, which is the first exercise §7.3's clock
    ///   has ever had. That is why it exists: a stowed body is beyond every cone, so
    ///   the tidier the temperament the flatter this row reads.
    ///
    /// Since **#381** the stow has its own §13.2 slot, so every one of these is read
    /// straight off the histogram — the hand-rolled `BodyStored` counter this test used
    /// to carry was the tell that the metric was missing.
    ///
    /// Loose and direction-only (§13.4), like every other shape assertion here: the
    /// counts are free to move, the zero-versus-nonzero split is not.
    #[test]
    fn the_striking_profiles_work_the_body_chain() {
        let batch = |profile: Profile| {
            let records = run_batch(0..60, DEFAULT_INPUT_CAP, move |_| {
                StealthBot::with_profile(profile)
            })
            .expect("generates");
            let usage = records
                .iter()
                .fold(UsageHistogram::new(), |acc, r| acc.merged(&r.usage));
            let takedowns: u32 = records.iter().map(|r| r.takedowns).sum();
            let found: u32 = records.iter().map(|r| r.bodies_found).sum();
            (takedowns, found, usage)
        };

        for profile in [Profile::BASELINE, Profile::CAUTIOUS] {
            let (takedowns, found, usage) = batch(profile);
            assert_eq!(
                (
                    takedowns,
                    found,
                    usage.count(Verb::Takedown),
                    usage.count(Verb::Drag),
                    usage.count(Verb::Stow)
                ),
                (0, 0, 0, 0, 0),
                "{}: a profile that declines the verb must land none of it",
                profile.name,
            );
        }

        let (strikes, _, aggressive) = batch(Profile::AGGRESSIVE);
        assert!(
            strikes > 0,
            "aggressive landed no takedown over 60 seeds — the §13.2 row is dead again",
        );
        assert_eq!(
            aggressive.count(Verb::Takedown),
            strikes,
            "every takedown must reach the histogram as well as the metric",
        );
        assert!(
            aggressive.count(Verb::Drag) > 0,
            "aggressive never took hold of a body — §8.3's drag is still unexercised",
        );

        let (strikes, found, careless) = batch(Profile::CARELESS);
        assert!(strikes > 0, "careless landed no takedown over 60 seeds");
        assert!(
            found > 0,
            "no body careless left on the floor was ever found — §7.3's clock still \
             has nothing to react to",
        );

        // The temperaments' actual split is **stowing**, not grabbing. Taking hold is
        // not a decision — stepping off a body's cell grabs it whether you meant to or
        // not (§8.3/#187) — so `careless` racks up grabs it immediately undoes, and a
        // `Drag` count says nothing about temperament on its own. Putting a body
        // *away* is the decision, and since #381 it has its own §13.2 slot, so the
        // split is **read off the histogram** rather than replayed and counted by hand.
        assert!(
            aggressive.count(Verb::Stow) > 0,
            "aggressive never stowed a body — §10.3's deposit-and-lock is unexercised",
        );
        assert_eq!(
            careless.count(Verb::Stow),
            0,
            "careless stowed a body — a reach of zero must mean it never tidies up, \
             or `bodies_found` loses the source this profile exists to be",
        );
    }

    /// **#379: §10.3's partial cover has a live source, on every temperament.**
    ///
    /// Before this, `crouched_behind` was `None` for every turn of every run under
    /// every profile — so §10.1a's benches and the exact run geometry `cover.rs`
    /// computes for them were **inert in the harness**: a regression in
    /// core's `cover::run_conceals` or `cover::run_hugs` would have moved no metric at
    /// all. This is the test that stops that being true again, and it asserts three
    /// things a false zero could not fake:
    ///
    /// - every profile that **takes cover at all** ducks, because from your own cell
    ///   the crouch is a reflex rather than an appetite (see [`StealthBot::crouch`]) —
    ///   so a zero on one of those is a broken policy, not a temperament;
    /// - and `careless`, which declines concealment outright
    ///   ([`Profile::crouches`]), lands exactly none — the decline working rather than
    ///   an opportunity that never came, exactly as its `takedowns: 0` siblings do;
    /// - the **histogram** sees each duck, so the §13.2 row and the events agree;
    /// - the pose is entered from where the bot stands and **not left immediately**:
    ///   crouched turns outnumber the ducks that started them, which is the "spent a
    ///   turn for nothing" failure the ticket names.
    ///
    /// Direction-only and loose (§13.4) — the counts are free to move, the
    /// zero-versus-nonzero is not.
    #[test]
    fn every_profile_ducks_behind_a_bench() {
        for profile in Profile::ALL {
            let (mut ducks, mut crouched_turns) = (0u32, 0u32);
            for seed in 0..60 {
                let (mut state, _) = boot(seed);
                let mut bot = StealthBot::with_profile(profile);
                for _ in 0..DEFAULT_INPUT_CAP {
                    if state.outcome() != Outcome::Playing {
                        break;
                    }
                    let before = state.turn();
                    ducks += state
                        .step(bot.decide(&state))
                        .iter()
                        .filter(|e| matches!(e, intrusion_core::Event::Crouched { .. }))
                        .count() as u32;
                    if state.turn() > before && state.crouched() {
                        crouched_turns += 1;
                    }
                }
            }
            if !profile.crouches {
                assert_eq!(
                    (ducks, crouched_turns),
                    (0, 0),
                    "{}: a temperament that declines concealment must never crouch",
                    profile.name,
                );
                continue;
            }
            assert!(
                ducks > 0,
                "{}: never ducked behind a bench over 60 seeds — §10.3 is inert again",
                profile.name,
            );
            assert!(
                crouched_turns > ducks,
                "{}: {crouched_turns} crouched turns from {ducks} ducks — the pose is \
                 being dropped the turn after it is taken, so the turn bought nothing",
                profile.name,
            );
            // The same runs through the harness, so the §13.2 row cannot drift from
            // the events the policy actually produced.
            let records = run_batch(0..60, DEFAULT_INPUT_CAP, move |_| {
                StealthBot::with_profile(profile)
            })
            .expect("generates");
            let histogram = records
                .iter()
                .fold(UsageHistogram::new(), |acc, r| acc.merged(&r.usage))
                .count(Verb::Crouch);
            assert!(
                histogram > 0,
                "{}: the crouch never reached the usage histogram",
                profile.name,
            );
        }
    }

    /// The bench geometry is **genuinely entered**, not merely brushed past (#379).
    ///
    /// The acceptance criterion in its own words: a test that fails if
    /// `cover::run_conceals` regressed. It asserts the payoff rather than the call —
    /// over a batch there are crouched turns where the player stands **inside a
    /// guard's live cone** and is concealed from it anyway, which is exactly what the
    /// run geometry is for and what nothing in the harness could previously produce.
    /// A `run_conceals` that returned `false` would leave the bot standing up (and the
    /// count at zero); one that returned `true` everywhere would break the core's own
    /// directional tests next door.
    ///
    /// Read off `cautious`, which crouches most: it holds out for a cupboard, so the
    /// bench is what it settles for when a patrol arrives and none is near.
    #[test]
    fn the_bots_crouch_beats_a_live_cone() {
        let mut beat_a_cone = 0;
        for seed in 0..60 {
            let (mut state, _) = boot(seed);
            let mut bot = StealthBot::with_profile(Profile::CAUTIOUS);
            for _ in 0..DEFAULT_INPUT_CAP {
                if state.outcome() != Outcome::Playing {
                    break;
                }
                state.step(bot.decide(&state));
                if state.crouched() {
                    beat_a_cone += state
                        .guards()
                        .iter()
                        .filter(|g| {
                            g.fov().contains(state.player()) && state.concealed_from(g.pos())
                        })
                        .count()
                        .min(1);
                }
            }
        }
        assert!(
            beat_a_cone > 0,
            "no crouched turn ever had a cone on the player it was concealed from — \
             the bench's run geometry is not being exercised",
        );
    }

    /// The **crouch-walk** (§10.3/#379): the bot shuffles along the bench rather than
    /// standing up when a patrol comes round to its side of the furniture, and the
    /// pose survives the step.
    ///
    /// This is the half of `cover.rs` the duck alone never reaches — `run_hugs`, the
    /// rule that a plain step landing still against the run keeps the pose, corners
    /// included. Counted as "the anchor was there before the step and is still there
    /// after, and the turn produced a `Moved`", which is precisely the turn loop's own
    /// `crouch_walked` predicate observed from outside.
    #[test]
    fn the_bot_crouch_walks_along_the_bench() {
        let mut walks = 0;
        for profile in [Profile::CAUTIOUS, Profile::BASELINE] {
            for seed in 0..60 {
                let (mut state, _) = boot(seed);
                let mut bot = StealthBot::with_profile(profile);
                for _ in 0..DEFAULT_INPUT_CAP {
                    if state.outcome() != Outcome::Playing {
                        break;
                    }
                    let anchor = state.crouched_behind();
                    let events = state.step(bot.decide(&state));
                    let moved = events
                        .iter()
                        .any(|e| matches!(e, intrusion_core::Event::Moved { .. }));
                    if anchor.is_some() && moved && state.crouched_behind() == anchor {
                        walks += 1;
                    }
                }
            }
        }
        assert!(
            walks > 0,
            "the bot never crouch-walked — §10.3's `run_hugs` is unexercised, so the \
             bot only ever ducks and waits",
        );
    }

    /// The strike is **legitimate**, seed by seed, not merely counted (#316/#183).
    ///
    /// A takedown is legal from a guard's rear blind spot or under concealment and
    /// nowhere else (§7.2/§155), and the front strike the old bot could sneak through
    /// must not come back. Rather than trust the bot's own gate, this replays every
    /// strike a batch lands and checks the *core's* answer at the moment of the bump:
    /// the guard must not have detected the player, which is exactly the predicate
    /// [`State::guard_detects_now`] settles and the one §7.2 refuses a bump against.
    ///
    /// **The concealed strikes it lands are cupboard ones, never bench ones** (#379),
    /// and that is geometry rather than a shy policy: concealment across a bench needs
    /// the viewer on the far side of the furniture, which puts it at least two cells
    /// away, while a takedown needs it orthogonally adjacent. The two conditions
    /// cannot both hold — proven exhaustively in core's
    /// `cover::an_adjacent_viewer_is_never_concealed_by_a_bench` — so there is no
    /// crouch strike here to assert legal, and a batch reporting zero of them is the
    /// right answer rather than a gap to close.
    #[test]
    fn every_takedown_the_bot_lands_is_a_legal_one() {
        let mut struck = 0;
        for seed in 0..40 {
            let (state, _) = boot(seed);
            let mut state = state;
            let mut bot = StealthBot::with_profile(Profile::CARELESS);
            for _ in 0..DEFAULT_INPUT_CAP {
                if state.outcome() != Outcome::Playing {
                    break;
                }
                let input = bot.decide(&state);
                // A step into a guard is the takedown attempt (§7.2) — check the gate
                // the core will read, before it reads it.
                if let Input::Step(dir) = input {
                    if let Some(target) = state.player().step(dir) {
                        for guard in state.guards() {
                            if guard.pos() == target {
                                assert!(
                                    !state.guard_detects_now(guard),
                                    "seed {seed}: struck a guard that had the player — \
                                     a front strike is refused by §7.2 and must never \
                                     be attempted",
                                );
                                struck += 1;
                            }
                        }
                    }
                }
                state.step(input);
            }
        }
        assert!(
            struck > 0,
            "no strike happened in 40 seeds — this test would prove nothing",
        );
    }

    /// Every shipped profile still **plays the game** (§13.4), not just the
    /// baseline: over a batch each one reaches real endings rather than stalling
    /// out en masse. A temperament whose numbers livelock the bot would quietly
    /// turn its rows into a measurement of the bot instead of the game (§13.3),
    /// which is exactly what this catches. Loose, like the baseline's own
    /// mixed-outcome test: the exact counts are free to move.
    #[test]
    fn every_profile_finishes_its_runs() {
        let runs = 40;
        for profile in Profile::ALL {
            let records = run_batch(30..30 + runs, DEFAULT_INPUT_CAP, move |_| {
                StealthBot::with_profile(profile)
            })
            .expect("generates");
            let count = |o: RunOutcome| records.iter().filter(|r| r.outcome == o).count();
            let timeouts = count(RunOutcome::Timeout);
            assert!(
                timeouts <= runs as usize / 5,
                "{}: too many timeouts ({timeouts}/{runs}) — this temperament \
                 stalls rather than plays",
                profile.name,
            );
            assert!(
                count(RunOutcome::Win) >= 1 && count(RunOutcome::Capture) >= 1,
                "{}: a degenerate outcome profile ({} wins, {} captures)",
                profile.name,
                count(RunOutcome::Win),
                count(RunOutcome::Capture),
            );
        }
    }

    /// Regression (#171): the endless stalls #165 tipped the bot into now *finish*.
    /// The close-behind/automatic doors (§10.4) reshaped guard coverage enough to
    /// surface two self-inflicted stalls, both of which spent the whole input budget
    /// without the run ending:
    ///
    /// - **Marching onto its own exit.** Hunted with no reachable hideout, the flee
    ///   routine used to fall back on the exit cell; with objectives still out, a step
    ///   onto the exit is a refused, *free* bump (§4.5), so the turn never advanced and
    ///   the hunt never cooled (seeds 30, 43). It now cloaks or retreats instead.
    /// - **Sealing itself into a cupboard.** Waiting out a guard parked on a hideout's
    ///   only mouth, the bot would eventually push on, take the guard down, and drop
    ///   the body across that mouth — the §7.2/#170 soft-lock (seeds 33, 34, 44, 58,
    ///   64, 65). It now leaves such a guard be and waits for the patrol to step off.
    ///
    /// The second stall could no longer happen either way: #187 made a loose body
    /// non-solid, so a body across a mouth stops nobody (#316). The seeds are kept
    /// under the **baseline**, which still declines the strike, so this stays a
    /// regression test for the stalls rather than becoming a test of the new play.
    ///
    /// Each seed must reach a real end (win or capture), never the input cap.
    #[test]
    fn the_close_behind_door_stalls_now_finish() {
        for seed in [30, 43, 33, 34, 44, 58, 64, 65] {
            let record =
                run_one(seed, &mut StealthBot::new(), DEFAULT_INPUT_CAP).expect("generates");
            assert_ne!(
                record.outcome,
                RunOutcome::Timeout,
                "seed {seed}: the bot should play the run to an end, not stall out",
            );
        }
    }

    /// The **no-cheat** guarantee (§11.5a, the ticket's asserted case): the bot cannot
    /// route to intel it has never seen. At level start the player sees only their own
    /// room, so a console in another room is fogged — outside `memory` — and must not
    /// be a goal. The exit, by contrast, is the player's own tunnel and is known from
    /// the off.
    #[test]
    fn the_bot_cannot_route_to_unseen_intel() {
        let (state, placement) = boot(0);
        let bot = StealthBot::new();

        // The exit is always known — the way the player came in.
        assert_eq!(
            exit_cell(&state),
            Some(placement.exit()),
            "the exit is known from the start"
        );

        // Every console the bot would head for is one it has actually seen.
        let known = bot.known_intel(&state);
        for &console in &known {
            assert!(
                state.memory().contains(console),
                "known intel {console:?} must have been seen"
            );
        }

        // There is at least one placed console the player has not seen yet, and the
        // bot does not treat it as a goal — it cannot route to what it has never seen.
        let unseen: Vec<Cell> = placement
            .intel()
            .iter()
            .copied()
            .filter(|&c| !state.memory().contains(c))
            .collect();
        assert!(
            !unseen.is_empty(),
            "the start room should not reveal every console at turn zero"
        );
        for console in unseen {
            assert!(
                !known.contains(&console),
                "unseen intel {console:?} must not be a goal"
            );
        }
    }

    /// The ticket's batch smoke test (§13.2–§13.4): over a batch of generated seeds
    /// the bot finishes runs with a **mixed** outcome profile — some wins, some
    /// captures, few timeouts — and actually uses its innate escape (Run to flee), so
    /// the ability histogram has something real to measure. These are shape
    /// assertions, deliberately loose: they check the bot *plays*, not that it plays
    /// well (§13.4 — a smoke detector, not a judge), and the exact numbers are free to
    /// move as the game is tuned.
    ///
    /// The sim baseline holds the **innate-only** loadout (§8.3) — it plays *bare*, no
    /// salvaged tech — so only Run is asserted here. Camouflage and the other tech are
    /// not in the loadout to fire (a level must be winnable with no tech is the
    /// baseline this measures); a run that wants to weigh a specific tech grants it
    /// back and asserts on that.
    ///
    /// The **takedown** is deliberately not required either, and under this profile
    /// it must read exactly zero. It lands only from a guard's rear blind spot or
    /// under concealment (§7.2/§155, gated live since #183), and the baseline is an
    /// avoidance-first temperament that declines the verb outright
    /// ([`Profile::takedown_reach`] of zero, #316) — mandating it here would measure a
    /// contrived hunt rather than the game (§13.3). Deliberate rear-takedown play now
    /// exists; it lives in the striking profiles, and
    /// [`the_striking_profiles_work_the_body_chain`] is where it is asserted.
    #[test]
    fn over_a_batch_the_outcome_profile_is_mixed() {
        let runs = 40;
        let records =
            run_batch(30..30 + runs, DEFAULT_INPUT_CAP, |_| StealthBot::new()).expect("generates");
        let count = |o: RunOutcome| records.iter().filter(|r| r.outcome == o).count();
        let wins = count(RunOutcome::Win);
        let captures = count(RunOutcome::Capture);
        let timeouts = count(RunOutcome::Timeout);

        assert!(wins >= 1, "expected some wins, got {wins}");
        assert!(captures >= 1, "expected some captures, got {captures}");
        // "Few" timeouts: the bot should almost always *finish* a run one way or the
        // other, never stall out en masse (the whole point over a hand-player).
        assert!(
            timeouts <= runs as usize / 5,
            "too many timeouts: {timeouts}/{runs} — the bot is stalling, not playing"
        );

        // The innate escape fires, so the §13.2 histogram is not measuring a bot that
        // never acts: Run (fleeing) shows. Tech is out of the bare loadout, so it is
        // not asserted — nor the takedown (see the doc comment above).
        let usage = records
            .iter()
            .fold(UsageHistogram::new(), |acc, r| acc.merged(&r.usage));
        assert!(
            usage.count(Verb::Run) > 0,
            "the bot never used its one innate escape — the histogram is measuring \
             a bot that does not play",
        );
    }

    /// **The bot still plays the game while holding Pierce Wall** (§13.2/#303).
    ///
    /// The ability is unusable from most cells by design — its precondition is
    /// *exactly one adjacent wall* — which is precisely the shape that could make a
    /// naive policy hammer a key that never fires and stall the run out to the input
    /// cap. This grants it and plays a batch through the ordinary loop: the outcome
    /// profile stays mixed, so nothing livelocks.
    ///
    /// What it does **not** yet show is the ability being *used*: since #346 the
    /// bot asks every held ability's cue, but Pierce Wall's is the one that still
    /// declines every moment, so it never presses this key and the histogram slot
    /// honestly reads zero. Writing that cue is #347's job — a bot that pressed it
    /// at random would make the histogram measure the bot rather than the game
    /// (§13.3), which is the one thing the sim exists to avoid.
    #[test]
    fn the_bot_plays_identically_while_holding_pierce_wall() {
        /// Play `state` to a decision and report the inputs issued and how it ended.
        fn play(mut state: State) -> (Vec<Input>, Outcome, u32) {
            let mut bot = StealthBot::new();
            let mut issued = Vec::new();
            for _ in 0..DEFAULT_INPUT_CAP {
                if state.outcome() != Outcome::Playing {
                    break;
                }
                let input = bot.decide(&state);
                issued.push(input);
                state.step(input);
            }
            (issued, state.outcome(), state.turn())
        }

        let mut decided = 0;
        for seed in 30..50 {
            let (bare, _) = boot(seed);
            let (armed, _) = boot(seed);
            let armed =
                armed.with_loadout(intrusion_core::Loadout::innate().with(AbilityId::PierceWall));
            // Held, read off the bar's own roster rather than off the ability's
            // state: since #345 that state is **contextual**, so a fresh run standing
            // anywhere but square against one wall reads `Unusable` — which is the
            // ability working as designed, not a loadout that failed to take.
            assert!(
                armed
                    .ability_statuses()
                    .iter()
                    .any(|s| s.id == AbilityId::PierceWall),
                "seed {seed}: the run holds the ability",
            );

            let bare = play(bare);
            let armed = play(armed);
            assert_eq!(
                bare, armed,
                "seed {seed}: holding the ability changed the run"
            );
            decided += u32::from(armed.1 != Outcome::Playing);
        }
        assert!(
            decided >= 15,
            "only {decided}/20 runs reached a decision — the baseline is stalling, \
             so this test would prove nothing",
        );
    }
}
