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

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use intrusion_core::{
    AbilityId, AbilityState, Cell, Direction, Facility, GuardPerception, GuardState, Input, State,
    Terrain,
};

use crate::policy::PlayerPolicy;

/// A cost penalty added to a step that enters a cell a visible guard is watching
/// (§11.5). Far larger than any real path distance on the v1 footprint (a 40×40
/// facility bounds any route well under this), so a watched step is always dearer
/// than any unwatched one, while still being *comparable* — when every option is
/// watched, the least-watched, shortest route still wins.
const WATCHED_PENALTY: u64 = 1_000_000;

/// How near a perceived guard a cell must be to draw a keep-away penalty (Manhattan).
/// Inside this radius the bot steers wide; outside it, a guard is far enough to
/// ignore while routing. Also the range within which a held bot sidesteps rather
/// than standing.
const PROXIMITY_RADIUS: u32 = 5;

/// How near a perceived guard must be (Manhattan) for the bot to break off and take
/// cover before it is seen (§7.6). Wide enough to react while the guard is still out
/// of striking range, narrow enough not to hide from a patrol two rooms away.
const THREAT_RADIUS: u32 = 6;

/// The furthest a hideout may be (Manhattan) and still be worth diverting to for
/// cover. A bolthole further than this is not shelter — reaching it means marching
/// across the very patrol being dodged — so the bot routes carefully on instead.
const COVER_REACH: u32 = 8;

/// Once hidden, the bot stays put until the nearest perceived guard is beyond this
/// (Manhattan) — wider than [`THREAT_RADIUS`] so a guard loitering just outside does
/// not make it pop in and out, glimpsed on every step. Set at the sense range: come
/// out only once no guard is sensed at all.
const CLEAR_RADIUS: u32 = 8;

/// The most turns the bot will wait out a patrol from one cover stint. A guard whose
/// beat keeps it parked nearby would otherwise pin the bot in its cupboard forever;
/// past this it gives up hiding and pushes on, trading a certain timeout for a chance.
const MAX_HIDE: u32 = 12;

/// Turns of committed pursuit after leaving cover before the bot may hide again — the
/// anti-oscillation guard that keeps a patrol looping past a cupboard from trapping it
/// in an endless in-and-out (see [`StealthBot::cover_cooldown`]).
const COVER_COOLDOWN: u32 = 8;

/// The keep-away weight per unit of closeness-squared (see [`proximity_penalty`]).
/// Sized to dominate raw path distance on the v1 footprint — so the bot will take a
/// long way round to keep its distance — while staying well under [`WATCHED_PENALTY`],
/// which no amount of proximity may ever outweigh.
const PROXIMITY_UNIT: u64 = 1_000;

/// A keep-away cost for stepping onto `cell`: the closer a perceived guard, the
/// steeper it climbs (with the square of how far inside [`PROXIMITY_RADIUS`] the
/// guard sits), so the bot gives patrols a wide berth instead of brushing past a
/// cone's edge where the next sweep would catch it. Zero when no guard is near.
fn proximity_penalty(cell: Cell, guards: &[Cell]) -> u64 {
    guards
        .iter()
        .map(|&guard| cell.manhattan_distance(guard))
        .filter(|&distance| distance <= PROXIMITY_RADIUS)
        .map(|distance| {
            let closeness = u64::from(PROXIMITY_RADIUS + 1 - distance);
            closeness * closeness * PROXIMITY_UNIT
        })
        .sum()
}

/// The greedy baseline stealth bot (§13.2). Holds only what a player would carry in
/// their head across turns: which consoles it has already emptied — the game keeps
/// a taken console stamped as terrain, so without this the bot would keep routing to
/// intel it has already taken.
#[derive(Clone, Debug, Default)]
pub struct StealthBot {
    /// Consoles the bot has taken. Recorded optimistically the turn it steps onto a
    /// known untaken console: a bump into an untaken console in the field of view
    /// always takes it (§4.3/§6.2, the touching ring is always seen), so the take is
    /// certain and this never drifts from the game's own objective count.
    taken: HashSet<Cell>,
    /// Turns spent waiting in the current cover stint. Bounds how long the bot will
    /// sit in a cupboard for a patrol that will not leave: past [`MAX_HIDE`] it gives
    /// up and pushes on, so a lingering guard turns into a timeout- or capture-risk,
    /// never an endless wait. Reset to zero the moment it is not hidden.
    hide_turns: u32,
    /// Turns to press on before taking cover again, set when the bot leaves a
    /// bolthole. Without it a patrol whose beat loops past a cupboard would send the
    /// bot ducking in, stepping out, and straight back in — burning the whole input
    /// budget to a timeout without progress. The cooldown forces a stretch of actual
    /// pursuit between hides, so the loop advances instead of spinning.
    cover_cooldown: u32,
}

impl StealthBot {
    /// A fresh bot with nothing taken yet.
    pub fn new() -> Self {
        Self::default()
    }
}

impl PlayerPolicy for StealthBot {
    fn decide(&mut self, state: &State) -> Input {
        // A cover stint only counts while actually in a cupboard; stepping out of one
        // (or never being in one) resets it.
        if !state.hidden() {
            self.hide_turns = 0;
        }

        // The world's own facts, gathered once through the player's channels.
        let danger = danger_cells(state);
        let mut blocked = blocked_cells(state);
        // Never spring a takedown that would wall you in (§7.2/#170): a guard on the
        // sole mouth of a dead end is left blocked, not taken down, so the router waits
        // it out rather than dropping a body across its only way home.
        blocked.extend(self_sealing_takedowns(state, &blocked));

        // 1. Flee first: nothing else matters while a guard has you (§7.6).
        if being_hunted(state, &danger) {
            return self.flee(state, &danger, &blocked);
        }

        // 2. Not caught yet, but a patrol is closing and a bolthole is to hand: duck
        // in and let it pass rather than press the objective into its path. This is
        // where most detections are avoided — the player senses a guard as far out as
        // it could see them (both range 10, §9.1), so there is time to take cover.
        if let Some(input) = self.take_cover(state, &danger, &blocked) {
            return input;
        }

        // 3 & 4. Pursue the objective, or explore to find it.
        self.pursue(state, &danger, &blocked)
    }
}

impl StealthBot {
    /// Break contact (§7.6): open a gap with Run, make for a bolthole, and wait the
    /// hunt out from inside a hideout — the one place a guard's contact cannot reach
    /// (§4.5). Getting *to safety* is the whole job here, so it drives straight for
    /// the nearest refuge rather than keeping its polite distance from the chaser.
    fn flee(&self, state: &State, danger: &HashSet<Cell>, blocked: &HashSet<Cell>) -> Input {
        // Already hidden: the safest cell on the board. Hold still and let the
        // hunt cool (§7.6) — moving would only reveal the cupboard.
        if state.hidden() {
            return Input::Wait;
        }

        // Open a gap with Run (§8.3) — but only with room to spend the turn on it:
        // activating costs a turn standing still, which a guard already on top of
        // you turns into a capture, so run only when the nearest one is a step away.
        if state.ability_state(AbilityId::Run) == AbilityState::Ready
            && nearest_perceived_guard(state).is_none_or(|d| d > 1)
        {
            return Input::Activate(AbilityId::Run);
        }

        // Aim for the nearest known hideout to disappear into — the one place a
        // guard's contact cannot reach (§4.5).
        let boltholes = known_hideouts(state);
        if let Some(dir) = self.descend(state, &boltholes, danger, blocked, Descent::flee()) {
            return Input::Step(dir);
        }

        // No cupboard within reach: cloak instead. Camouflage is a hideout you carry
        // (§8.3) — a *still* cloaked player is undetectable — so activate it and then
        // hold, letting the hunt pass over an intruder it cannot see. The exit is
        // deliberately *not* a fallback refuge here: you cannot disappear into your own
        // tunnel, and with objectives still in the facility you cannot even step onto
        // it (§4.5), so routing there only bumps a door that never opens — a free
        // action that spends no turn and so never lets the hunt cool, stalling the run
        // out to the input cap instead of breaking contact.
        match state.ability_state(AbilityId::Camouflage) {
            AbilityState::Ready => return Input::Activate(AbilityId::Camouflage),
            AbilityState::Active { .. } => return Input::Wait,
            AbilityState::Cooling { .. } | AbilityState::Unusable => {}
        }

        // Nowhere to run to and nothing to cloak with: back away from the nearest
        // guard, off watched cells when it can, and hold only if truly cornered.
        retreat_step(state, danger, blocked).map_or(Input::Wait, Input::Step)
    }

    /// Take cover from a closing patrol before it ever sees you: when a guard is
    /// perceived within [`THREAT_RADIUS`], slip into a near hideout — or, with none to
    /// hand, cloak with Camouflage — and wait the patrol out (§7.6/§8.3/§10.3).
    /// Returns `None` when there is no near threat, or nothing to take cover with,
    /// leaving the bot to pursue as normal.
    ///
    /// Inside the cupboard it holds until the coast clears (no guard within
    /// [`CLEAR_RADIUS`]), then pursuit resumes: the "hide, let it pass, carry on" loop
    /// that is the whole point of a hideout.
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
            let clear = nearest.is_none_or(|d| d > CLEAR_RADIUS);
            if clear || self.hide_turns > MAX_HIDE {
                self.cover_cooldown = COVER_COOLDOWN;
                return None;
            }
            return Some(Input::Wait);
        }
        // Fresh out of cover: press on for a stretch before hiding again, so a patrol
        // looping past a cupboard cannot trap the bot in an in-and-out shuffle.
        if self.cover_cooldown > 0 {
            self.cover_cooldown -= 1;
            return None;
        }
        // Duck in only when a patrol is genuinely closing.
        if nearest.is_none_or(|d| d > THREAT_RADIUS) {
            return None;
        }
        // Only worth a detour to a hideout that is genuinely close by; a far one is
        // not cover, it is a march across the guard's path.
        let hideouts: Vec<Cell> = known_hideouts(state)
            .into_iter()
            .filter(|h| player.manhattan_distance(*h) <= COVER_REACH)
            .collect();
        if let Some(dir) = self.descend(state, &hideouts, danger, blocked, Descent::flee()) {
            return Some(Input::Step(dir));
        }

        // No cupboard within reach: cloak instead. Camouflage makes a *still* player
        // undetectable (§8.3) — a hideout you carry — so activate it and then hold,
        // letting the patrol pass over an intruder it cannot see. A stationary
        // cloaked player is concealed from every viewer, so `being_hunted` will not
        // fire and this keeps holding until the coast clears.
        match state.ability_state(AbilityId::Camouflage) {
            AbilityState::Ready => Some(Input::Activate(AbilityId::Camouflage)),
            AbilityState::Active { .. } => Some(Input::Wait),
            AbilityState::Cooling { .. } | AbilityState::Unusable => None,
        }
    }

    /// Pursue the objective — nearest known untaken console, then the exit — or, when
    /// no intel is known yet, explore toward the nearest frontier.
    fn pursue(&mut self, state: &State, danger: &HashSet<Cell>, blocked: &HashSet<Cell>) -> Input {
        let goals = if state.objectives_remaining() > 0 {
            let known = self.known_intel(state);
            // Nothing seen to head for: sweep the facility until the consoles show.
            if known.is_empty() {
                frontier_cells(state)
            } else {
                known
            }
        } else {
            // Every objective in hand: leave the way we came in (§4.5).
            exit_cell(state).into_iter().collect()
        };

        let Some(dir) = self.descend(state, &goals, danger, blocked, Descent::pursue()) else {
            // No safe progress. Standing still next to a patrol is how you get
            // walked into, so if one is close, sidestep to open ground; otherwise
            // hold a beat and let the cone sweep past (waiting also widens the
            // senses, §8.3/§9.1).
            return if nearest_perceived_guard(state).is_some_and(|d| d <= PROXIMITY_RADIUS) {
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
        if goals.is_empty() {
            return None;
        }
        let facility = state.layout().facility();
        let player = state.player();
        let guards = if mode.keep_clear {
            perceived_guard_cells(state)
        } else {
            Vec::new()
        };
        let field = cost_field(facility, goals, blocked, danger, &guards);

        let mut best: Option<(u64, bool, Direction)> = None;
        for dir in Direction::ALL {
            let Some(next) = player.step(dir) else {
                continue;
            };
            // Blocked cells (aware guards, bodies) are not steps; an unaware guard is
            // left routable, so a step onto one when it blocks the only way is the
            // takedown (§7.2). A goal cell is solid but seeded into the field, so a
            // console or the exit one step away reads cost 0 and is taken.
            if blocked.contains(&next) {
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

/// How [`StealthBot::descend`] weighs its options — the difference between picking
/// a careful route to an objective and bolting for cover.
#[derive(Clone, Copy)]
struct Descent {
    /// Add a keep-away cost near every perceived guard, so a route gives patrols a
    /// wide berth rather than skimming them. On when pursuing; off when fleeing,
    /// where the only thing that matters is reaching the refuge.
    keep_clear: bool,
    /// Refuse a step into a cone from a currently-safe cell (hold instead). On when
    /// pursuing — there is no rush; off when fleeing, where standing still loses.
    hold_watched: bool,
}

impl Descent {
    /// Careful routing to an objective: keep clear of guards and never walk into a
    /// cone from safety.
    fn pursue() -> Self {
        Self {
            keep_clear: true,
            hold_watched: true,
        }
    }

    /// Bolting for cover: shortest line to the refuge, cones penalised but never a
    /// reason to stand still.
    fn flee() -> Self {
        Self {
            keep_clear: false,
            hold_watched: false,
        }
    }
}

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

/// Cells the bot must not step onto: bodies (solid, §7.2) and any guard that has
/// already detected the player — bumping an aware guard is a wasted, refused turn
/// (§7.2), whereas an *unaware* one is left out so the takedown stays available.
fn blocked_cells(state: &State) -> HashSet<Cell> {
    let mut cells = HashSet::new();
    for body in state.bodies() {
        cells.insert(body.cell());
    }
    for guard in state.guards() {
        // Perceived-and-aware guards block; an unaware guard is a takedown target,
        // not an obstacle. A guard the player cannot perceive is unknown, so it
        // cannot be planned around — the bot only avoids what it can see or sense.
        if state.perceive_guard(guard).is_some() && guard.detected_player() {
            cells.insert(guard.pos());
        }
    }
    cells
}

/// Unaware guards the bot must **not** take down because the body would seal it in
/// (§7.2/#170, from the bot's side). A takedown drops the body on the guard's own
/// cell; when the player sits in a dead end — a hideout, a one-wide stub — whose
/// *only* routable way out holds that guard, springing the takedown walls the mouth
/// and strands the bot for the rest of the run (the exact §10.3 cupboard soft-lock).
/// The bot leaves such a guard blocked instead, so the router waits rather than
/// striking: the guard is unaware and the hidden bot is safe (§4.5), so the patrol
/// steps off the mouth on its own and the exit reopens.
///
/// Only a *lone* exit can be a trap — with a second way out, a sealed mouth still
/// leaves the other — so this fires solely when the player has exactly one routable,
/// unblocked neighbour and an unaware guard stands on it. A guard the player cannot
/// even perceive is not planned around (the bot avoids only what it can see or sense).
fn self_sealing_takedowns(state: &State, blocked: &HashSet<Cell>) -> Vec<Cell> {
    let facility = state.layout().facility();
    let player = state.player();
    let mut exits = Direction::ALL
        .iter()
        .filter_map(|&d| player.step(d))
        .filter(|&n| routable(facility, n) && !blocked.contains(&n));
    let (Some(mouth), None) = (exits.next(), exits.next()) else {
        return Vec::new(); // no exit, or more than one — never a single-mouth trap
    };
    let sealed = state
        .guards()
        .iter()
        .any(|g| g.pos() == mouth && state.perceive_guard(g).is_some() && !g.detected_player());
    if sealed {
        vec![mouth]
    } else {
        Vec::new()
    }
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
/// aims for.
fn known_hideouts(state: &State) -> Vec<Cell> {
    let facility = state.layout().facility();
    let memory = state.memory();
    let occupied = blocked_cells(state);
    all_cells(facility)
        .filter(|&cell| {
            facility.terrain(cell) == Some(Terrain::Hideout)
                && memory.contains(cell)
                && !occupied.contains(&cell)
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
/// to reach a goal, with a guard's cone ([`WATCHED_PENALTY`]) and its keep-away
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
                WATCHED_PENALTY
            } else {
                0
            };
            let entry = 1 + watched + proximity_penalty(neighbour, guards);
            let next = here + entry;
            if cost.get(&neighbour).is_none_or(|&old| next < old) {
                cost.insert(neighbour, next);
                heap.push(Reverse((next, neighbour.y, neighbour.x)));
            }
        }
    }
    cost
}

/// Whether the player can move *through* `cell` when routing (§10.3). Floor and open
/// doors are plain walk-through; a closed door opens on a bump, so it routes as
/// passable; a hideout is enterable. Everything solid to the player — walls, hinges,
/// tables, consoles, the exit — is not a through-cell (consoles and the exit are
/// reached as goals, not crossed).
fn routable(facility: &Facility, cell: Cell) -> bool {
    matches!(
        facility.terrain(cell),
        Some(Terrain::Floor | Terrain::DoorPanelOpen | Terrain::DoorPanelClosed | Terrain::Hideout)
    )
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
    use crate::{run_batch, run_one, RunOutcome, UsageHistogram, Verb, DEFAULT_INPUT_CAP};
    use intrusion_core::{generate_level, Direction, LevelConfig, Rng, State};

    /// Boot a real V1 level exactly as the harness does (§13.2), returning the state
    /// and the placement so a test can compare against the ground truth the bot must
    /// *not* peek at.
    fn boot(seed: u64) -> (State, intrusion_core::Placement) {
        let (layout, placement) =
            generate_level(&LevelConfig::V1, &mut Rng::new(seed)).expect("V1 generates");
        let guards = placement.guards(&layout);
        let state = State::new(
            layout,
            placement.player(),
            Direction::North,
            guards,
            placement.intel().iter().copied(),
            placement.exit(),
        );
        (state, placement)
    }

    /// §12.4: the same seed under the bot produces byte-identical rows, twice. The
    /// bot carries its own state (taken consoles, cover timers), so this pins that
    /// none of it leaks non-determinism into the run.
    #[test]
    fn the_bot_is_deterministic_per_seed() {
        for seed in [0, 7, 200] {
            let a = run_one(seed, &mut StealthBot::new(), 300).expect("generates");
            let b = run_one(seed, &mut StealthBot::new(), 300).expect("generates");
            assert_eq!(a, b, "seed {seed}: a bot run reproduces");
            assert_eq!(
                a.to_json_line(),
                b.to_json_line(),
                "seed {seed}: identical bytes"
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
    /// captures, few timeouts — and actually uses its tools (Run to flee, Camouflage
    /// and a takedown), so the ability histogram has something real to measure. These
    /// are shape assertions, deliberately loose: they check the bot *plays*, not that
    /// it plays well (§13.4 — a smoke detector, not a judge), and the exact numbers
    /// are free to move as the game is tuned.
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

        // Abilities fire, so the §13.2 histogram is not measuring a bot that never
        // acts: Run (fleeing), Camouflage (portable cover) and the takedown all show.
        let usage = records
            .iter()
            .fold(UsageHistogram::new(), |acc, r| acc.merged(&r.usage));
        for verb in [Verb::Run, Verb::Camouflage, Verb::Takedown] {
            assert!(
                usage.count(verb) > 0,
                "expected the bot to use {verb:?} at least once across the batch"
            );
        }
    }
}
