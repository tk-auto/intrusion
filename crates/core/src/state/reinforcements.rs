//! Rungs 2 and 3 of the alert ladder: guards walk into the facility (§7.3/#374).
//!
//! [`alert`](crate::alert) built the ladder and gave rung 1 its teeth — a shortened
//! patrol dwell. Rungs 2 and 3 had none: they announced an escalation and did nothing,
//! which is §2.3's worst row all over again. This module is what they do. Rung 2 sends
//! **one** guard in, rung 3 sends **two** more, cumulatively — a run driven 0 → 3
//! gains **three** — and rung 3 is the top, which is the ceiling that stops
//! reinforcements spiralling however loud the run gets.
//!
//! This **reverses** the old "explicitly out" line the ladder ticket carried (*"spawning
//! new guards mid-level — nothing in the design supports it; the guard count is a
//! generation knob"*). The reversal is deliberate and is written into §7.3, not left as
//! a disagreement between the doc and the code.
//!
//! # The rule that decides whether this reads as escalation or as cheating
//!
//! **Never in view.** The arrival cell is outside the player's field of view and never
//! adjacent to them ([`admits_arrival`](State::admits_arrival)). An arrival the player
//! *witnesses* is a guard materialising out of nothing, which no amount of fiction
//! repairs — so if no cell in the facility can honour that this turn, **nobody
//! arrives**. Breaking the rule is worse than missing the reinforcement.
//!
//! They come in at the far end: the §10.5 region whose nearest cell is furthest from
//! the player, which is the region the fiction can carry ("they came in from outside").
//! And they **walk** from there. Nothing teleports to the trigger.
//!
//! What the player's **guard sense** (§9.1) does about it is deliberately *not* gated.
//! A reinforcement that arrives inside the sense box shows up as a new dot, and that is
//! fine: the sense is position-only information the player has earned, not a witnessed
//! materialisation, and gating on it would be unworkable anyway — a turn spent waiting
//! widens the sense to a 41×41 box (§9.1), which on the v1 footprint is the whole map,
//! so no cell would qualify and reinforcements would simply stop arriving whenever the
//! player waited.
//!
//! # What they do when they get here
//!
//! They head for the **trigger cell** — the body that was found, the console that was
//! tampered with, the player's last known cell — and **search** it (§7.6), exactly as a
//! §7.3 radio dispatch or a §7.7 call-in does. That is the important restraint: *the
//! net closing on a stale cell* is what §7.6 asks for; more guards tracking the
//! player's live position is the un-fun chase (§7.6's trap). **Reinforcements search,
//! they do not hunt.**
//!
//! When the errand ends they patrol from wherever they finished, with a region beat
//! like any guard (§7.5/§10.5) — the beat grown **after** the incumbents', so the
//! newcomer fans out into ground they do not already hold rather than grinding the
//! same wing.
//!
//! **The beat is cut when the errand ends, not when the guard lands.** A reinforcement
//! arrives at the far end of the map ([`arrival_cell`](State::arrival_cell) picks the
//! region furthest from the player, and that answer is stable across a run), so a beat
//! grown at landing would tether every reinforcement of the run to the same arrival
//! room — it would walk the whole facility back to it the moment its watch expired.
//! Growing it at the release instead anchors it on where it actually finished, which
//! is what §7.3 has always claimed happens. Until then the newcomer carries no beat and
//! has no Calm territory, which costs nothing: it is on an errand the whole time.
//!
//! # Why arrivals are queued rather than spawned where they are decided
//!
//! Phase 3 resolves its whole-state readings **once**, up front, into per-guard vectors
//! indexed by position in [`State::guards`] — an invariant its module header states
//! plainly: *no guard is added or removed between the passes*. A body found in pass 3
//! raises the ladder, so spawning there and then would grow that vector's subject
//! mid-phase and leave every later pass reading past the end of its snapshot.
//!
//! So an escalation **queues** its arrivals ([`queue_reinforcements`](State::queue_reinforcements))
//! and the turn lands them once the guards have finished acting
//! ([`land_reinforcements`](State::land_reinforcements)). The guard enters at the end of
//! the turn it was called for, which is also the honest fiction, and the queue is drained
//! every turn — never carried — so there is no backlog to reason about.

use super::*;
use crate::beat::coordinated_beat_cells;
use crate::generate::shuffle;
use crate::radio::RadioClock;

/// Guards that walk in on reaching **rung 2** (§7.3 **[START]**).
///
/// §10.2's `--guards` sweep reads roughly **8–10 points of win rate per guard**, so
/// this is a coarse knob by construction: one guard is already a large swing, and the
/// tuning surface for the escalation is how hard a rung is to *reach* (the §7.3
/// thresholds the sim sweeps) rather than these magnitudes.
pub(crate) const RUNG_TWO_REINFORCEMENTS: usize = 1;

/// Guards that walk in on reaching **rung 3** (§7.3 **[START]**), on top of rung 2's.
/// A run driven 0 → 3 therefore gains [`RUNG_TWO_REINFORCEMENTS`] + this = three, and
/// **rung 3 is the top** — that ceiling is what keeps a loud run from spiralling.
pub(crate) const RUNG_THREE_REINFORCEMENTS: usize = 2;

/// How much longer than the straight line a walk across the facility is allowed to be
/// when sizing a reinforcement's lead (**[START] = 2**).
///
/// The lead has to outlast the journey (see
/// [`Guard::respond_across`](crate::Guard::respond_across)), and the journey is a route
/// around walls and doors rather than a straight line, so the Manhattan distance is a
/// floor rather than an estimate. Doubling it is the allowance; it is deliberately
/// generous, because the cost of overshooting is a guard that searches slightly longer
/// than it needed to, while the cost of undershooting is the errand quietly evaporating
/// — a reinforcement that walks halfway and forgets why (§2.3's inert system, in
/// miniature).
const ROUTE_ALLOWANCE: u32 = 2;

/// The lead a reinforcement arriving at `from` needs to reach `to` and search it
/// (§7.3/§7.6/#374) — never shorter than the ordinary
/// [`ALERT_DURATION`](crate::guard::ALERT_DURATION), so a short errand behaves exactly
/// like any other dispatch.
fn errand_lead(from: Cell, to: Cell) -> u32 {
    let journey = from.manhattan_distance(to) * ROUTE_ALLOWANCE;
    crate::guard::ALERT_DURATION.max(journey + crate::guard::SEARCH_DURATION)
}

/// How many guards `rung` sends in on its own under `tuning` (§7.3). Rung 1 sends
/// none — being noticed costs you the calm patrol dwell and nothing else — and there
/// is no rung 4.
fn reinforcements_for(rung: u32, tuning: AlertTuning) -> usize {
    match rung {
        2 => tuning.rung_two_reinforcements,
        3 => tuning.rung_three_reinforcements,
        _ => 0,
    }
}

impl State {
    /// Queue the guards an escalation from `from` to `to` sends in (§7.3), each on an
    /// errand to `at` — the cell the escalation was *about*.
    ///
    /// Summing the rungs actually **crossed** is what makes a jump straight to 3 send
    /// all three, and — because the ladder is monotone and this is called only from the
    /// one place the rung rises ([`raise_alert`](State::raise_alert)) — it is also what
    /// makes "no rung sends guards twice" true by construction rather than by a flag
    /// somebody has to remember to set.
    pub(super) fn queue_reinforcements(&mut self, from: u32, to: u32, at: Cell) {
        let tuning = self.alert.tuning();
        let sending: usize = (from + 1..=to)
            .map(|rung| reinforcements_for(rung, tuning))
            .sum();
        self.pending_reinforcements
            .extend(std::iter::repeat_n(at, sending));
    }

    /// Land every queued arrival (§7.3/#374): pick a cell the player cannot see, spawn
    /// a guard there, and send it to search the cell that called it.
    ///
    /// Run at the end of the world phases, once the guards have acted — see the module
    /// header for why. The queue is emptied whether or not each arrival finds a cell: a
    /// call that cannot be answered out of sight is simply unanswered, never queued for
    /// a later turn, which is the same rule §7.7 gives an unanswered call-in.
    pub(super) fn land_reinforcements(&mut self, events: &mut Vec<Event>) {
        let pending = std::mem::take(&mut self.pending_reinforcements);
        for errand in pending {
            let Some(cell) = self.arrival_cell() else {
                continue;
            };
            // The radio clock comes off the run's own stream (§12.4), like the ones
            // placement draws — never a fresh source — so a seed reproduces not only
            // *that* a reinforcement arrived but the whole schedule it arrived with.
            let clock = RadioClock::draw(&mut self.rng);
            // No beat yet — it is cut when the errand ends, around wherever the guard
            // finished ([`settle_new_beats`]). See the module header for why landing is
            // the wrong moment.
            let mut guard = Guard::patrolling(cell).with_radio_clock(clock);
            // The errand: walk to the trigger cell and search it (§7.6), exactly as a
            // dispatch does. It searches — it does not hunt (§7.6's trap) — and its
            // lead is sized to the trip, or §7.1's cold-lead backstop would strand it
            // halfway across the map having looked at nothing.
            guard.respond_across(errand, errand_lead(cell, errand));
            self.guards.push(guard);
            events.push(Event::ReinforcementArrived { at: cell });
        }
    }

    /// **Recut the level's partition** (§7.5/§10.5) when the guard set has changed —
    /// which, in a run, means a reinforcement has come to rest and needs ground of its
    /// own (§7.3/#374).
    ///
    /// Every beat is regrown together from every guard's **live position**, so the
    /// incumbents' territories give way to make room rather than the newcomer
    /// squeezing into whatever is left. That is the half of §7.3's escalation that was
    /// missing: with beats a partition rather than a cover ([`crate::beat`]), a guard
    /// walking in raises coverage **density** — the same ground, watched by more
    /// people, each with less of it — instead of merely adding a body to a level whose
    /// territories never noticed.
    ///
    /// **Why the recut happens when the newcomer settles, not when it lands.** Its
    /// beat is anchored where it stands, and while it is on its errand it is standing
    /// wherever the walk has got to — the arrival room, or a corridor between. Cutting
    /// then would tether it to a cell it is only passing through, which is the defect
    /// #398 removed. So a guard still Responding or Alerted is left out of the anchor
    /// set entirely and keeps whatever beat it had.
    ///
    /// Called once per turn, but it does work only while a Calm guard has no beat —
    /// the single turn an errand releases. Beat growth must stay a rare call: the
    /// anchors move every turn, so a per-turn regrow would re-cut every territory under
    /// every guard and patrols would visibly churn (§7.5/[`crate::beat`]).
    ///
    /// A guard's `inspected` memory is deliberately **carried across** the recut. Ground
    /// it kept is ground it has genuinely looked at, and the new ground reads as
    /// uninspected and gets swept first — which is exactly the behaviour wanted from a
    /// territory that just changed shape. A live `destination` that the recut moved out
    /// of the guard's beat is dropped at the next
    /// [`repick_patrol_target`](crate::Guard), not here.
    pub(super) fn recut_beats(&mut self) {
        if !self
            .guards
            .iter()
            .any(|g| !g.has_beat() && g.state() == GuardState::Calm)
        {
            return;
        }
        // Only guards that are actually patrolling take part: a guard mid-errand is
        // somewhere incidental, so anchoring on it would cut a beat around a corridor.
        let settling: Vec<usize> = self
            .guards
            .iter()
            .enumerate()
            .filter(|(_, g)| g.state() == GuardState::Calm)
            .map(|(index, _)| index)
            .collect();
        let anchors: Vec<Cell> = settling.iter().map(|&i| self.guards[i].pos()).collect();

        let beats = coordinated_beat_cells(self.layout.regions(), self.layout.facility(), &anchors);
        for (&index, beat) in settling.iter().zip(beats) {
            self.guards[index].set_beat(beat);
        }
    }

    /// A cell a reinforcement may walk in on, or `None` when the facility offers none
    /// out of sight this turn (§7.3/#374).
    ///
    /// The **furthest region wins**: of the §10.5 regions holding an eligible cell, the
    /// one whose nearest cell is furthest from the player — measured over the region's
    /// whole cell set, so a long region that merely *reaches* toward the player is
    /// judged by how close it gets. Ties break on region order, which is deterministic.
    /// Within it the cell is drawn from the **run's own stream** (§12.4), so the same
    /// seed and inputs put the same guard on the same cell on the same turn.
    ///
    /// A layout with **no regions at all** — a hand-built fixture, never a generated
    /// level — falls back to the eligible cells furthest from the player. The fiction
    /// degrades from "they came in from the far side of the building" to plain
    /// distance; the "never in view" rule does not degrade at all, because it is
    /// [`admits_arrival`](Self::admits_arrival) that holds it and every path goes
    /// through there.
    fn arrival_cell(&mut self) -> Option<Cell> {
        let player = self.player;
        let regions = self.layout.regions();
        let furthest = regions
            .regions()
            .filter_map(|(_, region)| {
                let eligible: Vec<Cell> = region
                    .cells()
                    .iter()
                    .copied()
                    .filter(|&cell| self.admits_arrival(cell))
                    .collect();
                let nearest = region
                    .cells()
                    .iter()
                    .map(|c| c.manhattan_distance(player))
                    .min()?;
                (!eligible.is_empty()).then_some((nearest, eligible))
            })
            // `max_by_key` keeps the *later* of equal keys, and regions iterate in a
            // fixed order, so a tie resolves the same way every run.
            .max_by_key(|&(nearest, _)| nearest)
            .map(|(_, eligible)| eligible);

        let mut eligible = match furthest {
            Some(eligible) => eligible,
            None => self.furthest_eligible_cells(),
        };
        if eligible.is_empty() {
            return None;
        }
        shuffle(&mut eligible, &mut self.rng);
        eligible.first().copied()
    }

    /// Every eligible cell at the greatest distance from the player — the region-less
    /// fallback described in [`arrival_cell`](Self::arrival_cell). Empty when the
    /// facility has nowhere out of sight to admit anybody.
    fn furthest_eligible_cells(&self) -> Vec<Cell> {
        let facility = self.layout.facility();
        let eligible: Vec<(u32, Cell)> = (0..facility.height())
            .flat_map(|y| (0..facility.width()).map(move |x| Cell::new(x, y)))
            .filter(|&cell| self.admits_arrival(cell))
            .map(|cell| (cell.manhattan_distance(self.player), cell))
            .collect();
        let Some(&furthest) = eligible.iter().map(|(d, _)| d).max() else {
            return Vec::new();
        };
        eligible
            .into_iter()
            .filter(|&(d, _)| d == furthest)
            .map(|(_, cell)| cell)
            .collect()
    }

    /// Whether `cell` can take an arriving guard (§7.3/#374) — the "never in view" rule
    /// and the ordinary facts about a cell an actor can stand on.
    ///
    /// **Plain floor only**, so nobody walks in on top of a console, a cupboard or a
    /// doorway; empty of actors, since a guard is solid (§4.3) and a body would be
    /// stood on; clear of the decoy, which a guard arriving would trample for no reason
    /// (§8.3). And then the rule this whole feature lives or dies by: **outside the
    /// player's field of view, and not adjacent to them** — adjacency in every
    /// direction, diagonals included, because a guard appearing at arm's length reads
    /// as materialising whether or not the cone happens to cover it.
    fn admits_arrival(&self, cell: Cell) -> bool {
        let facility = self.layout.facility();
        if facility.terrain(cell) != Some(Terrain::Floor) {
            return false;
        }
        if !facility.can_enter(cell, ACTOR_FILL) || self.occupied(cell) || self.decoy == Some(cell)
        {
            return false;
        }
        if self.player_fov.contains(cell) {
            return false;
        }
        let (dx, dy) = (
            cell.x.abs_diff(self.player.x),
            cell.y.abs_diff(self.player.y),
        );
        dx.max(dy) > 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §7.3 **[START]**: the two counts are named, and the ladder they add up to is the
    /// design's — nothing at rung 1, one at rung 2, two more at rung 3, three in total,
    /// and no rung 4. Pinned so a later tune is a visible edit (§10.2's ~9 points of win
    /// rate per guard is why these move deliberately or not at all).
    #[test]
    fn the_reinforcement_counts_are_pinned() {
        assert_eq!(RUNG_TWO_REINFORCEMENTS, 1, "rung 2 sends one guard");
        assert_eq!(RUNG_THREE_REINFORCEMENTS, 2, "rung 3 sends two more");
        let shipped = AlertTuning::default();
        assert_eq!(
            reinforcements_for(0, shipped),
            0,
            "rung 0 is a quiet facility"
        );
        assert_eq!(reinforcements_for(1, shipped), 0, "rung 1 sends nobody");
        assert_eq!(reinforcements_for(2, shipped), RUNG_TWO_REINFORCEMENTS);
        assert_eq!(reinforcements_for(3, shipped), RUNG_THREE_REINFORCEMENTS);
        assert_eq!(
            reinforcements_for(crate::TOP_RUNG + 1, shipped),
            0,
            "there is no rung 4",
        );
        assert_eq!(
            (1..=crate::TOP_RUNG)
                .map(|r| reinforcements_for(r, shipped))
                .sum::<usize>(),
            3,
            "however loud it gets, the facility gains at most three",
        );
    }
}
