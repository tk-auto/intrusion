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
//! Between cover and the objective sits one **opportunistic** step with no plan of its
//! own: a comms console already adjacent is bumped, silencing guard-to-guard call-ins
//! for the rest of the level (§7.7/#405). It never detours to one — that would price
//! §7.7's switch instead of its route.
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
//! catalogue fails to compile until somebody says what it is for, so no new verb can
//! land as a silent zero in the usage histogram — and a false zero is
//! indistinguishable from a dead ability.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use intrusion_core::{
    AbilityId, AbilityState, Affordance, Cell, Direction, Facility, GuardPerception, GuardState,
    Input, State, Terrain,
};

use crate::cue::{Bid, Intent, Moment};
use crate::policy::PlayerPolicy;
use crate::profile::{Descent, Profile, CROSSING_MARGIN};

mod percept;
pub(crate) use percept::*;

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
    /// A fresh bot with nothing taken yet, playing the [`Profile::BALANCED`]
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

        // 0.2. Inside the tunnel (§4.5/§10.7/#466). Every run starts here and — if it
        // is won — ends here, and a crawlspace admits exactly one plan: crawl. Nothing
        // below this can apply, since a crawler is concealed, contact-safe, and confined
        // to the path (there is no cover to take, no guard to strike and no console to
        // reach from inside a wall), so it sits at the top rather than in the ladder.
        if state.in_duct() {
            return self.crawl(state);
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

        // 4.5. A comms console already under the bot's hand (§7.7/#405). Below cover
        // deliberately: silencing while a patrol is closing spends the turn that was
        // the escape. Above the objective just as deliberately — the bot is walking
        // anyway, and one bump buys the rest of the level.
        if let Some(input) = self.silence_radio(state) {
            return input;
        }

        // 5 & 6. Pursue the objective, or explore to find it.
        self.pursue(state, &danger, &blocked)
    }
}

impl StealthBot {
    /// Crawl (§10.7), which is the whole of what a bot inside a duct can do — and, for
    /// the exit tunnel, the opening and closing beats of every run (§4.5/#466).
    ///
    /// Which way depends on the errand, and there are only two:
    ///
    /// - **Out into the facility**, when there is still a run to play: crawl toward the
    ///   mouth and climb out of it. This is the opening of every run — the bot starts on
    ///   the way-out cell of its own tunnel — and it is also how it leaves a §10.7
    ///   shortcut it took as a route.
    /// - **Out of the building**, when the intel gate is met and the bot is in its own
    ///   tunnel: crawl to the way out and step off the board. The last step is read off
    ///   the **usable line** rather than computed here, so the bot presses exactly the
    ///   key the game offers a player (§11.4).
    ///
    /// It plans no further than that: the crawl is confined to the path (§10.7), so
    /// there is nothing to weigh and no cost field to descend. Waiting mid-crawl would
    /// be a wasted turn — it does not even widen the sense in here (§9.1/§10.7) — so
    /// the bot never does.
    fn crawl(&mut self, state: &State) -> Input {
        // Standing on the way out with the gate met: the row says `exit: leave`, and
        // pressing it is the win (§4.5). It says `exit: needs the intel` otherwise, and
        // pressing *that* would be a free no-op — so the bot turns round instead.
        if state.exit_ready() {
            if let Some(dir) = aimed_at(state, Affordance::Leave) {
                return Input::Step(dir);
            }
        }
        let Some(duct) = state.occupied_duct() else {
            return Input::Wait; // unreachable: `decide` asked because we are in one
        };
        let cells = duct.cells();
        let Some(i) = cells.iter().position(|&c| c == state.player()) else {
            return Input::Wait; // ditto: a crawler is always on its own path
        };
        // Which end to make for. Leaving the building means the way out, and it is only
        // ever the far end of the exit tunnel; anything else means the mouth.
        let leaving = state.exit_ready() && duct.way_out().is_some();
        let next = if leaving {
            cells.get(i + 1)
        } else if i > 0 {
            cells.get(i - 1)
        } else {
            None
        };
        if let Some(&next) = next {
            if let Some(dir) = Direction::between(state.player(), next) {
                return Input::Step(dir);
            }
        }
        // On the mouth: climb out onto the floor it opens into (§10.7). Prefer a cell no
        // guard is watching — the mouth peek is exactly the look this decision is for —
        // and take a watched one only if that is all there is, since sitting in the
        // crawlspace makes no progress and the run has a clock (§13.2).
        //
        // **Never back onto the path**, even when the next cell along it is walkable
        // floor: a duct's interior may overlie a room (§10.7 cross-room routing), and a
        // step onto that cell is a *crawl*, not a climb-out. Without this the bot walks
        // the first two cells of the exit tunnel for the rest of the run.
        let danger = danger_cells(state);
        let blocked = blocked_cells(state);
        let facility = state.layout().facility();
        // **Enterable, not merely routable.** Climbing out is a *step*, and the crawl's
        // confinement (§10.7) gives it no bump: the core lets a crawler off the mouth
        // only onto a cell it can actually enter, so a closed door panel beside `E` —
        // which [`routable`] admits, because a walker opens one by bumping it (§10.4) —
        // is not a way out of the tunnel. Choosing one is a refused, *free* input, and
        // since nothing about the mouth changes by pressing it again the bot pressed it
        // until the input cap: a whole run spent on turn fourteen. The freeze predates
        // #481 (seeds 231 and 288 of the 300-seed sweep hold it on `main`); moving where
        // the exit lands is what walked it onto a pinned witness seed.
        let out = |avoid_danger: bool| {
            Direction::ALL.into_iter().find(|&dir| {
                state.player().step(dir).is_some_and(|cell| {
                    enterable(facility, cell)
                        && !cells.contains(&cell)
                        // Not into a cupboard, either: it has one mouth (§10.1.6), and
                        // if that mouth is the cell we are standing on the only way out
                        // is back in here. Hiding is a decision the flee routine makes
                        // from the floor, never a way to arrive.
                        && facility.terrain(cell) != Some(Terrain::Hideout)
                        && !blocked.contains(&cell)
                        && !(avoid_danger && danger.contains(&cell))
                })
            })
        };
        out(true).or_else(|| out(false)).map_or(
            // Every way out is watched *and* occupied: hold on the entry cell and let
            // the peek keep reading the room. This is the one wait a crawl ever makes,
            // and it is the §10.7 counterplay — the deliberate pause at the mouth.
            Input::Wait,
            Input::Step,
        )
    }

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
    /// different rates (`balanced` 8, `cautious` 13, `aggressive` 2 over 100 seeds) on
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
        // **Never while phased.** The duck is a *bump* into the furniture, and while
        // Dephase is up there is no bump (§8.3): the step walks the bot inside the
        // bench instead, and a window that ends in there costs the safety eject and a
        // stun as long as the throw. So a phased bot has no duck on offer — which is
        // no loss, since the phase conceals nothing and the cover would have been the
        // point. `step_down` refuses to enter a solid on the ordinary route for the
        // same reason; this is that rule on the cover ladder.
        //
        // It became reachable at #449. With a three-turn window the phase was spent by
        // the time the crossing was walked, so a phased bot was never standing on open
        // floor with a bench beside it and a turn to fill; the fourth turn is exactly
        // that turn.
        if matches!(
            state.ability_state(AbilityId::Dephase),
            AbilityState::Active { .. }
        ) {
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
    /// The grab **is** a decision since #451 — a wait spent standing on the body —
    /// but a temperament that does not stow still cannot express that by standing
    /// still, because a body already in hand has to be put down. It has to *act*, by
    /// letting go, which is why [`body_stow_reach`](Profile::body_stow_reach) of zero
    /// still reaches this code.
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
    /// **Three turns since #451**: walk onto the body, **wait** to take hold, then
    /// leave in the direction of the cupboard. It was two — the grab rode the step
    /// *off* the cell (#187) and cost nothing — and the middle turn is the price the
    /// ticket set on making the pickup a decision. A body only counts as worth
    /// fetching when there is somewhere to *put* it: hauling one to nowhere is turns
    /// spent making the run slower and no quieter, and that is one turn truer now.
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
        // Already standing on one: **wait, and take hold** (§8.3/#451). This is the
        // whole of the bot's half of that change. The pickup used to ride the step
        // *off* the body, so this branch stepped toward the cupboard and the grab came
        // free; now it is its own spent turn, and a bot that kept stepping would walk
        // off the body and leave it lying there for ever.
        //
        // Pressing the same key core acts on, rather than modelling the rule: the wait
        // takes hold if and only if core says it would, which is the seam
        // `docs/bot-behaviour.md` §2 asks for.
        let loose = findable_bodies(state);
        if loose.contains(&player) {
            return Some(Input::Wait);
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
    ///
    /// **`None` when a guard is standing on the body.** A loose body is non-solid
    /// (§7.2), so a chaser walks straight over the one being dragged — and then the
    /// release bump is a step into an *aware* guard, which §7.2 refuses. Attempting it
    /// would spend the turn on a refusal in the one situation where the turn is the
    /// escape, and it is exactly the front strike
    /// [`every_takedown_the_bot_lands_is_a_legal_one`] forbids the bot from ever
    /// aiming. Declining hands the turn back to [`haul`](Self::haul)'s other answers —
    /// route to a shelter, or step away and try again once the guard has moved off.
    ///
    /// Only a guard the bot can **perceive** counts (§9.2/§13.2): one it cannot see or
    /// sense is not something a player could plan around either, so blocking on it
    /// would be the bot reading state it is not allowed to.
    fn let_go(&mut self, state: &State, body: Cell) -> Option<Input> {
        if state
            .guards()
            .iter()
            .any(|g| g.pos() == body && state.perceive_guard(g).is_some())
        {
            return None;
        }
        self.abandoned.insert(body);
        Direction::ALL
            .into_iter()
            .find(|&dir| state.player().step(dir) == Some(body))
            .map(Input::Step)
    }

    /// Throw the comms console's switch when it is **already adjacent** (§7.7/#405):
    /// one bump, and no guard calls another for the rest of the level.
    ///
    /// **Opportunistic, never a goal.** There is no cost-field term for the console, no
    /// frontier bias toward it and no route that prefers it — a seed where the bot ends
    /// up beside one got there on the route it was already walking. That restraint is
    /// §7.7's own: "**The cost is the route, not the switch.** One bump is cheap;
    /// getting to it is not. Placement distance is therefore the balance knob." A bot
    /// that *routed* here would price the switch instead of the route and make that knob
    /// measure the bot's pathfinding (§13.4: a profile is a temperament, not a solver).
    ///
    /// The trigger is core's own [`Affordance::SilenceRadio`] rather than a private scan
    /// of the four neighbours (`docs/bot-behaviour.md` §2: rules asked of core, never
    /// re-implemented). Core already answers "is this console still live, is it in view,
    /// would the bump land" — and being FOV-gated, going through it satisfies §11.5a for
    /// free: the console has to have been *seen*, which is the whole reason §7.7 calls
    /// it findable rather than given.
    ///
    /// It has no [`Intent`] and no cue, and belongs with the takedown rather than with
    /// the plans: a physical commitment on the cell you are standing beside, not a
    /// route.
    fn silence_radio(&self, state: &State) -> Option<Input> {
        if self.profile.comms_reach == 0 {
            return None;
        }
        aimed_at(state, Affordance::SilenceRadio).map(Input::Step)
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
    /// has exactly one plan. It leaves by the nearest **walkable and empty** neighbour
    /// — the far side it phased in for, or the side it came from if the far side has
    /// gone. Either beats the eject.
    ///
    /// **Empty matters as much as walkable.** A guard is solid (§4.3), so a step into
    /// the cell one is standing in is refused and the turn is spent still inside the
    /// wall; do that until the duration runs out and the eject fires anyway. Terrain
    /// alone was enough to keep the bot safe while patrols covered part of the level,
    /// and stopped being enough once they covered all of it (§7.5) — a guard parked on
    /// the landing cell is simply a likelier event now.
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
        let walkable = |dir: &&Direction| {
            player
                .step(**dir)
                .and_then(|cell| facility.terrain(cell))
                .is_some_and(|t| !t.blocks_movement())
        };
        let empty = |dir: &&Direction| {
            player.step(**dir).is_some_and(|cell| {
                !state.guards().iter().any(|guard| guard.pos() == cell)
                    && !state.bodies().iter().any(|body| body.cell() == cell)
            })
        };
        Direction::ALL
            .iter()
            .find(|dir| walkable(dir) && empty(dir))
            // Every way out is blocked by somebody. Take a walkable one anyway: the
            // step is refused, but next turn they may have moved, and standing still
            // by choice is strictly worse.
            .or_else(|| Direction::ALL.iter().find(walkable))
            .map(|&dir| Input::Step(dir))
    }

    /// Put this moment to every held ability's cue (§13.2/#346) and return the
    /// winning bid's input, or `None` to spend the turn stepping or waiting as
    /// usual.
    ///
    /// This is the *only* place the bot decides to press an ability key. What each
    /// ability is for lives in [`crate::cue`], one exhaustively-matched arm apiece,
    /// so a new row in the §8.1 catalogue cannot arrive dead by omission — it fails to
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
            // The takedown appetite, as the yes/no the cues need (§7.2/#316) — the same
            // knob `strike` gates on, so a temperament that declines the verb declines it
            // at every range.
            strikes: self.profile.takedown_reach > 0,
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
        let phased = matches!(
            state.ability_state(AbilityId::Dephase),
            AbilityState::Active { .. }
        );
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
            // **Never walk into a solid on the ordinary route while phased.** The field
            // seeds its goals whether or not they can be stood on — a console and the
            // exit are solid (§4.3) — and a bump into one is normally the *take*. While
            // Dephase is up it is not a bump: the step moves the player inside, and a
            // duration that expires in there costs the safety eject and a stun (§8.3).
            // Entering a solid is the crossing's business alone, and the crossing
            // checks it has the window to come out again.
            //
            // **The sprint's free second cell is part of the same step.** Run moves two
            // cells on one input (§8.3) and core walks the second one itself, so a check
            // that looks one cell ahead lets a phased sprinter be *carried* into a solid
            // it never chose — and while the phase is up there is no bump to stop it at
            // the wall. One turn later the duration expires in there and the eject fires.
            // §13.2's rule that the bot only presses what a player could press cuts both
            // ways: it must also own what the press actually does.
            if phased && self.phased_step_ends_in_a_solid(state, dir) {
                continue;
            }
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

    /// Whether pressing `Step(dir)` while phased would leave the bot standing in a
    /// solid — the cell the eject is priced off (§8.3).
    ///
    /// The subtlety is that *one press is not always one cell*. With Run up the step
    /// moves two (§8.3), and the free second cell is chosen by the rule rather than by
    /// the bot, so the question a caller has to ask is where the press **ends**, not
    /// where it begins. Phased, nothing along the way refuses it: a wall the sprint
    /// would ordinarily stop against is simply walked into.
    ///
    /// Only ever consulted while phased. Unphased, a solid ahead is a bump — a spent
    /// turn or a take (§4.4), never a stranding — and the ordinary cost field is left
    /// to judge it.
    fn phased_step_ends_in_a_solid(&self, state: &State, dir: Direction) -> bool {
        let facility = state.layout().facility();
        let solid = |cell: Cell| facility.terrain(cell).is_some_and(|t| t.blocks_movement());
        let sprinting = matches!(
            state.ability_state(AbilityId::Run),
            AbilityState::Active { .. }
        );
        let Some(next) = state.player().step(dir) else {
            return false;
        };
        // The near cell strands the bot whether or not the sprint carries it further;
        // the far one only exists as a destination while Run is up.
        match sprinting.then(|| next.step(dir)).flatten() {
            Some(beyond) => solid(next) || solid(beyond),
            None => solid(next),
        }
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
/// **One cell of solid, never two.** The crossing spends three turns whatever the
/// window is — press, in, out — and #449 widened the window to 4, so there is now a
/// turn of slack rather than none. The cue is deliberately *not* widened with it: a
/// two-cell run would spend the whole window and land its exit on the expiry turn, and
/// a duration that ends inside a solid costs a safety eject plus a stun as long as the
/// throw (§8.3) — the exact trap the cue must not walk into. The slack is what absorbs
/// a stolen turn (the re-bid [`StealthBot::pursue`] makes mid-crossing), not a deeper
/// crossing.
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

/// Whether the player can **stand on** `cell` — the stricter question [`routable`]
/// deliberately does not answer, and the core's own rule again
/// ([`Terrain::blocks_movement`], §4.3/§10.3).
///
/// A route may plan straight through a closed door panel because walking into one
/// *opens* it (§10.4); a step that has no bump behind it may not. That is the case
/// wherever the game hands the player a move and no interaction — climbing out of a
/// duct mouth (§10.7) is the one the bot meets.
fn enterable(facility: &Facility, cell: Cell) -> bool {
    facility
        .terrain(cell)
        .is_some_and(|terrain| !terrain.blocks_movement())
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
mod tests;
