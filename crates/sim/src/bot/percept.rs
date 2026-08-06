//! **What a player could tell from the board** — the bot's perception queries, and
//! nothing else (§11.5a/§13.2, `docs/bot-behaviour.md` §2).
//!
//! Every function here answers a question about the world **through the player's own
//! channels**: the fogged memory, the perceived guards and their painted cones, the
//! affordances the usable line offers. This module *is* the no-cheat boundary — the
//! policy in `bot.rs` plans over what these return, so a reviewer auditing "does the
//! bot read anything a player is not shown?" reads this file and the `Moment`
//! construction, and is done. A query that would need `State` internals beyond the
//! player-facing surface does not belong here; it belongs nowhere in the sim.

use super::*;

/// Whether a guard currently has the player, or is about to (§7.6). True when a
/// visible guard is actively hunting (chasing or investigating), or when the
/// player stands in a seen guard's cone without being concealed from it — the
/// exposure the danger overlay paints red (§11.5).
pub(crate) fn being_hunted(state: &State, danger: &HashSet<Cell>) -> bool {
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
pub(crate) fn danger_cells(state: &State) -> HashSet<Cell> {
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
pub(crate) fn witnessed_cone_cells(state: &State) -> HashSet<Cell> {
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
pub(crate) fn findable_bodies(state: &State) -> Vec<Cell> {
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
pub(crate) fn near_findable_body(bodies: &[Cell], hideout: Cell) -> bool {
    bodies
        .iter()
        .any(|&body| body.sight_distance(hideout) <= BODY_HIDE_CLEARANCE)
}

/// Cells the bot must not step onto: any guard that has already detected the player
/// — bumping an aware guard is a wasted, refused turn (§7.2), whereas an *unaware*
/// one is left out so the takedown stays available.
///
/// **Bodies are deliberately not here** (§7.2/#187/#451). They used to be, on the
/// strength of a comment calling them solid; they have not been since bodies went
/// non-solid, and routing round one costs the bot the only way to *take hold* of it —
/// the grab is a wait spent **standing on** the body, so a bot that will not stand on
/// one can never drag one (#316), and that is truer now than when the grab merely
/// rode the step away. The single exception is the door-crush rule, which is core's
/// business and not a routing question.
pub(crate) fn blocked_cells(state: &State) -> HashSet<Cell> {
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
    // A **key-gated** doorway with no key in hand (§10.4/#236): the bump is refused and
    // changes nothing, so a router that treated a locked panel as the walk-through §10.4
    // makes it would plan straight at the door and press it for the rest of the run.
    // Blocked, the bot routes round the locked room instead — and the moment a takedown
    // puts the key in its hand the cells stop being blocked and the room is a room again.
    //
    // Only the **closed** ones: what the lock refuses is the handle, never the doorway,
    // so a keyed door a guard has just walked through is exactly the slip-in the modifier
    // is built around, and the bot may take it like any other open panel.
    if !state.holds_key() {
        let regions = state.layout().regions();
        cells.extend(state.keyed_door_cells().filter(|&c| {
            regions
                .door_at(c)
                .is_some_and(|id| !regions.door(id).is_open())
        }));
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
/// walkable, and the bot can stand on it and wait to take hold (§8.3/#451). The rule
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
pub(crate) fn declined_takedowns(
    state: &State,
    blocked: &HashSet<Cell>,
    profile: &Profile,
) -> Vec<Cell> {
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
pub(crate) fn rear_strike_cells(
    state: &State,
    danger: &HashSet<Cell>,
    blocked: &HashSet<Cell>,
) -> Vec<Cell> {
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

/// The direction the usable line (§11.4) aims `want` in, or `None` when the row does
/// not offer it from where the bot stands.
///
/// The direction is an `Option` since #451 — the line carries one standing-on entry that
/// has none — so a bump affordance with no direction would be core contradicting itself.
/// This reads the direction rather than asserting it, and a shape that cannot happen
/// simply yields no press.
pub(crate) fn aimed_at(state: &State, want: Affordance) -> Option<Direction> {
    state
        .affordances()
        .into_iter()
        .find_map(|(dir, a)| (a == want).then_some(dir?))
}

/// The exit cell — the player's own tunnel, known from the start (§4.5). Found by
/// scanning the always-visible geometry for the one exit tile, so it needs no
/// fog gate: a player knows the way they came in.
pub(crate) fn exit_cell(state: &State) -> Option<Cell> {
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
pub(crate) fn known_hideouts(state: &State) -> Vec<Cell> {
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
pub(crate) fn frontier_cells(state: &State) -> Vec<Cell> {
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

/// Whether a crouch anchored on `table` would hide a player at `from` from **every**
/// one of `viewers` (§10.3) — the question the duck and the crouch-walk both ask, put
/// to the core's own geometry ([`State::crouch_would_conceal`]) rather than re-derived
/// here (§13.2: a private copy of a game rule is how the bot's metrics quietly stop
/// describing this game).
///
/// *Every* viewer, deliberately: concealment from one of two patrols is not cover, it
/// is a coin toss. With no viewers at all it is vacuously true, which is why both
/// callers check for a threat before asking.
pub(crate) fn conceals_from_all(state: &State, table: Cell, from: Cell, viewers: &[Cell]) -> bool {
    viewers
        .iter()
        .all(|&viewer| state.crouch_would_conceal(table, from, viewer))
}

/// The Manhattan distance to the nearest guard the player can perceive (seen or
/// sensed), or `None` when none is in reach — the gap the flee routine reads to
/// decide whether it can afford a turn spent activating Run.
pub(crate) fn nearest_perceived_guard(state: &State) -> Option<u32> {
    let player = state.player();
    perceived_guard_cells(state)
        .into_iter()
        .map(|cell| player.manhattan_distance(cell))
        .min()
}

/// The cells of every guard the player perceives, seen or sensed (§9.2).
pub(crate) fn perceived_guard_cells(state: &State) -> Vec<Cell> {
    state
        .guards()
        .iter()
        .filter(|g| state.perceive_guard(g).is_some())
        .map(|g| g.pos())
        .collect()
}
