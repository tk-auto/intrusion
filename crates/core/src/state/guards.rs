//! Phase 3: the guards act (§4.2, §7).
//!
//! The third and last phase of a spent turn, split out of [`state.rs`](super) so the
//! turn loop reads as its three phases rather than as one of them. It folds this
//! turn's sight into every guard's mind ([`Guard::sense`](crate::Guard)), witnesses a
//! §15 Q5 dive, runs the found-body (§7.2), hideout (§10.3) and decoy (§8.3) scans,
//! and finally moves each guard — where contact with the player is capture (§4.5).
//!
//! # One reading, five passes
//!
//! Each pass needs answers that are queries over the *whole* state — is the player
//! concealed from this guard, can the player see it, is it confused — and each takes
//! the guards mutably, so the answers cannot be asked mid-loop. They are resolved
//! once into [`GuardSenses`] before any guard is touched, and every pass reads that
//! same snapshot. Nothing in phase 3 can invalidate it: the player does not move, no
//! ability window opens or closes, and each guard is consulted before it steps.
//!
//! That is a rule, not a coincidence, and it is why the movement pass reads
//! [`GuardSenses::acts`] rather than re-asking
//! [`guard_confused`](State::guard_confused). Two readings of the same fact that
//! merely *happen* to agree are the shape of #199 and #200 — a stale flag and a live
//! gate disagreeing about the same guard — so there is one reading here, and the
//! passes share it.
//!
//! The order of the passes is load-bearing and must not be shuffled: a guard that can
//! see you ignores a decoy, and a body found this turn starts the search the hideout
//! check then reads.

use super::*;

/// The whole-state readings phase 3 resolves **once**, up front (§4.2).
///
/// Every field is a fact about the world as the phase *opens*. The per-guard vectors
/// are indexed by position in [`State::guards`], which is stable for the phase — no
/// guard is added or removed between the passes.
struct GuardSenses {
    /// Where the player stands. Fixed for the phase: the player acted in phase 1.
    player: Cell,
    /// The cupboard the player climbed into **this turn**, or `None` — the entry-turn
    /// signal the §15 Q5 witness check reads (§10.3).
    entered: Option<Cell>,
    /// The cell the player now stands hidden in — `None` on open floor or in a duct.
    /// A stored witness that no longer matches it is stale and dropped.
    hidden_cell: Option<Cell>,
    /// Per guard: whether the player is [`concealed_from`](State::concealed_from) it
    /// (§10.3) — the cupboard's and the crouch's payoff.
    concealed: Vec<bool>,
    /// Per guard: whether the player can *see* it ([`GuardPerception::Seen`], §9.2).
    ///
    /// A guard may only witness a dive it was in a position for the player to *read*:
    /// one whose cell is in the player's own FOV, so its cone is painted on the danger
    /// overlay (§11.5) and the player saw it cover the cupboard. A guard the player
    /// cannot see cannot flush them — that would be the "captured by something you
    /// couldn't sense" unfairness §2.2 forbids. This is also exactly the channel the
    /// §13.2 bot plans on (`perceive_guard`), so core and bot never disagree.
    seen: Vec<bool>,
    /// Per guard: whether it is **confused** (§8.3/#240) — blinded and frozen by an
    /// active Confusion within [`CONFUSION_RADIUS`] of the player. Read through
    /// [`acts`](Self::acts), never directly, so no pass can forget the skip.
    suppressed: Vec<bool>,
    /// The `guards_always_search_hideouts` modifier (§12.6), read once off the one
    /// resolved config value (§12.3) and handed to each guard's own check.
    always_search: bool,
}

impl GuardSenses {
    /// Read the phase's snapshot. Every query here is `&self` over the whole state,
    /// which is exactly why it must happen before the passes take the guards mutably.
    fn read(state: &State) -> Self {
        let player = state.player;
        Self {
            player,
            entered: state.entered_hideout,
            hidden_cell: state.hidden().then_some(player),
            concealed: state
                .guards
                .iter()
                .map(|guard| state.concealed_from(guard.pos()))
                .collect(),
            seen: state
                .guards
                .iter()
                .map(|guard| state.perceive_guard(guard) == Some(GuardPerception::Seen))
                .collect(),
            suppressed: state
                .guards
                .iter()
                .map(|guard| state.guard_confused(guard))
                .collect(),
            always_search: state.modifiers.guards_always_search_hideouts,
        }
    }

    /// Whether guard `index` takes part in phase 3 at all.
    ///
    /// A confused guard takes **no** part (§8.3/#240): it does not sense (so its state
    /// and lead pause rather than reset, for a clean resume, §8.2), does not witness a
    /// dive, finds no body, checks no cupboard, is not drawn by a decoy, and does not
    /// move — so it cannot capture by stepping into the player (§4.5). Confusion is no
    /// shield, though: a guard *outside* the bubble still moves and captures normally,
    /// and a frozen guard's cell stays solid — there is no walking through it.
    fn acts(&self, index: usize) -> bool {
        !self.suppressed[index]
    }
}

impl State {
    /// Phase 3 (§4.2): the guards *sense*, then *act*.
    ///
    /// Five passes over one [`GuardSenses`] reading, in an order that is part of the
    /// design (see the module header): sense and witness, find bodies, check hideouts,
    /// notice the decoy, then move.
    pub(super) fn guard_phase(&mut self, events: &mut Vec<Event>) {
        let senses = GuardSenses::read(self);
        self.sense_guards(&senses, events);
        self.find_bodies(&senses, events);
        self.check_hideouts(&senses);
        self.check_decoy(&senses);
        self.move_guards(&senses, events);
    }

    /// Pass 1 — every guard takes in this turn's information ([`Guard::sense`], §7.6):
    /// it sees the player from the cone phase 2 just recomputed, and a player in its
    /// cone flips it to Chasing (certain zone) or Investigating (glimpse zone).
    /// Detection is vision alone (§9 **[SETTLED]** — guards do not hear), and a player
    /// concealed from that guard is not seen — the cupboard's payoff (§10.3/§7.6).
    ///
    /// Folded in here too: the §15 Q5 **witness**. On the entry turn, a guard the
    /// player can see that is *alerted* (any non-Calm mood) and whose cone covers the
    /// cupboard saw the player climb in and may flush them — it re-engages the alcove
    /// as a live lead. A Calm patrol whose cone merely grazes the entry is not
    /// alerted, so it never checks, and the cupboard stays a safe room against it
    /// (§10.3).
    fn sense_guards(&mut self, senses: &GuardSenses, events: &mut Vec<Event>) {
        let mut spotters = Vec::new();
        for (index, guard) in self.guards.iter_mut().enumerate() {
            if !senses.acts(index) {
                continue;
            }
            // Awareness is per-turn, so the pre-sense reading is last turn's: a
            // guard aware now that was not aware then has *freshly* found the
            // player — the transition [`Event::Detected`] reports, and the §13.2
            // sim counts. A held chase re-detects every turn and stays silent.
            let was_aware = guard.detected_player();
            guard.sense(senses.player, senses.concealed[index]);
            if guard.detected_player() && !was_aware {
                events.push(Event::Detected { by: guard.pos() });
                // Record the fresh spot for the one-beat spot flash (§11.5, #222).
                // Every fresh detection is recorded; the renderer paints only the
                // ones the player cannot *see* — a seen guard's cone already paints.
                spotters.push(index);
            }
            if let Some(cell) = senses.entered {
                if senses.seen[index]
                    && guard.state() != GuardState::Calm
                    && guard.fov().contains(cell)
                {
                    guard.flush_hideout(cell);
                }
            } else if guard.witnessed_hideout().is_some()
                && guard.witnessed_hideout() != senses.hidden_cell
            {
                // The player has left the cell this guard was flushing (climbed out, or
                // moved to another cupboard): the witness is stale, so drop it.
                guard.forget_hideout();
            }
        }
        // Hand this turn's fresh spots to the renderer (#222), replacing the set the
        // step head cleared. Empty on a turn nobody freshly detected the player.
        self.spotters = spotters;
    }

    /// Pass 2 — the found-body scan (§7.2): a body is *found* the first time any cone
    /// covers it. A body does not block sight, so the cones just recomputed decide.
    /// Every guard seeing it reacts ([`Guard::find_body`]: the harder alert, and the
    /// search unless the live player has it busy); the loudest event in the game fires
    /// exactly once per body.
    fn find_bodies(&mut self, senses: &GuardSenses, events: &mut Vec<Event>) {
        for body_index in 0..self.bodies.len() {
            if self.bodies[body_index].found() {
                continue;
            }
            let at = self.bodies[body_index].cell();
            // A body in a hideout is *gone* (§7.2): the cupboard conceals it
            // completely, like the player it was built for — no cone finds it.
            // (It still misses its radio pings — that confusion is the §7.3
            // payoff, delivered by the radio ticket.)
            if self.layout.facility().terrain(at) == Some(Terrain::Hideout) {
                continue;
            }
            let mut seen = false;
            for (index, guard) in self.guards.iter_mut().enumerate() {
                if !senses.acts(index) {
                    continue;
                }
                if guard.fov().contains(at) {
                    guard.find_body(at);
                    seen = true;
                }
            }
            if seen {
                self.bodies[body_index].mark_found();
                events.push(Event::BodyFound { at });
            }
        }
    }

    /// Pass 3 — the found-a-body-nearby check (§15 Q5, second half): a found body is
    /// loud evidence the intruder is close (§7.2), so a guard searching the area
    /// around one checks the cupboards inside its sweep — an occupied hideout within
    /// `SEARCH_RADIUS` of the body it is searching is flushed, reusing the witness
    /// capture gate (#185).
    ///
    /// Only a **body** search checks: a guard that merely lost a chase still leaves
    /// the cupboard the safe wait-out it is (§10.3), the "hold still, watch the cone
    /// sweep past" payoff. Re-evaluated every turn, so a player who dives into the
    /// searched area *during* the sweep is checked too — the readable mistake is
    /// hiding within the search a body you left triggered (§2.2). A stowed body is
    /// never found (skipped in [`find_bodies`](Self::find_bodies)), so it never starts
    /// a search and never reaches here.
    ///
    /// The `guards_always_search_hideouts` modifier (§12.6) widens this to *any*
    /// active search: with it on, a lost-chase sweep over the cupboard you dived into
    /// flushes it too, not only a body search — a harder setting that turns off the
    /// §7.6 wait-out.
    fn check_hideouts(&mut self, senses: &GuardSenses) {
        let Some(cell) = senses.hidden_cell else {
            return;
        };
        for (index, guard) in self.guards.iter_mut().enumerate() {
            if !senses.acts(index) {
                continue;
            }
            if guard.checks_hideout_at(cell, senses.always_search) {
                guard.check_hideout(cell);
            }
        }
    }

    /// Pass 4 — the decoy scan (§8.3, #105): a guard whose cone covers the decoy — and
    /// whose look did *not* detect the player this turn — turns to Investigate it. The
    /// precedence is the whole point: a guard that can see you ignores the fake;
    /// decoys work on guards that have lost you.
    fn check_decoy(&mut self, senses: &GuardSenses) {
        let Some(decoy) = self.decoy else {
            return;
        };
        for (index, guard) in self.guards.iter_mut().enumerate() {
            if !senses.acts(index) {
                continue;
            }
            if !guard.detected_player() && guard.fov().contains(decoy) {
                guard.investigate_decoy(decoy);
            }
        }
    }

    /// Pass 5 — each guard `decide`s a step (§7.5) and takes it. A guard moving into
    /// the player's cell is a capture and ends the run (§4.5). Otherwise it moves onto
    /// any cell that admits it and holds no other actor; a guard with nowhere to go, or
    /// whose step is blocked, simply holds.
    fn move_guards(&mut self, senses: &GuardSenses, events: &mut Vec<Event>) {
        for i in 0..self.guards.len() {
            if self.outcome != Outcome::Playing {
                return;
            }
            if !senses.acts(i) {
                continue;
            }
            let facility = self.layout.facility();
            // Guards are solid to each other and path *around* a colleague (§7.8):
            // the decider routes only through cells no other guard holds. A body is
            // **not** an obstacle (§7.2 — non-solid), so a guard routes and steps
            // straight over one, which is what stops a body in a chokepoint from
            // freezing an investigation or a patrol (#182). Positions are read fresh
            // here, so a guard sees where the colleagues that already moved this turn
            // now stand.
            let blocked: Vec<Cell> = self
                .guards
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, g)| g.pos())
                .collect();
            let Some(dir) =
                self.guards[i].decide(facility, &blocked, &mut self.rng, self.dwell_chance)
            else {
                continue;
            };
            let Some(target) = self.guards[i].pos().step(dir) else {
                continue;
            };

            if target == self.player {
                // Inside a duct the player is in the crawlspace, not on the floor the
                // guard walks (§10.7): a duct changes *nothing* guard-facing, so the
                // guard steps over the cell as if empty — neither capturing the
                // concealed crawler nor blocked by them. Fall through to the move below.
                if !self.in_duct() {
                    // On the floor, contact is capture (§4.5) — unless the player is in
                    // an occupied cupboard, a solid cell the patrol routes *around*
                    // (§10.3/§7.6): the guard cannot enter, so it holds this turn. This
                    // is the "hold still, watch the cone sweep past" payoff — with one
                    // exception (§15 Q5): a guard that *witnessed* the player climb into
                    // this cupboard while alerted flushes them out, so its contact is a
                    // capture like any other. Every other guard is still refused.
                    if self.hidden() && self.guards[i].witnessed_hideout() != Some(target) {
                        continue;
                    }
                    self.guards[i].place_at(target);
                    self.outcome = Outcome::Lost;
                    events.push(Event::Captured { by: target });
                    return;
                }
            }
            // A closed door does not stop a guard: its route runs straight through
            // (§10.3's deliberate closed-panel rule), and the walk-in is the bump
            // that opens it (§10.4) — the door is the guard's whole action this
            // turn; it steps through on a later one. Guard traffic opens the facility
            // up over a level; the Calm close-behind below is the counter-pressure
            // (§10.4/#146), not a symmetric one — an open door still spreads.
            if self.layout.facility().terrain(target) == Some(Terrain::DoorPanelClosed) {
                self.operate_door(target);
                events.push(Event::DoorOpened {
                    at: target,
                    by_player: false,
                });
                continue;
            }
            // A guard moves onto a cell the terrain admits and no *guard* holds. Its
            // own cell is a step behind `target`, so the mover is never in the way;
            // the player's cell was captured above, so target is never the player
            // here. A body is non-solid (§7.2), so a guard may step onto a body's
            // cell — the body just lies underneath it (#182). `advance_to` refreshes
            // the moved guard's cone at once, so the sight a frame shows never lags
            // the position it shows (§11.5); the next phase 2 recomputes everything.
            if self.layout.facility().can_enter(target, ACTOR_FILL)
                && self.guard_at(target).is_none()
            {
                let from = self.guards[i].pos();
                let facility = self.layout.facility();
                self.guards[i].advance_to(target, dir, facility);
                // A guard arriving on the decoy's cell tramples it (§8.3):
                // walking into the "intruder" is how the fake is found out.
                self.stomp_decoy(target, events);
                // §10.4/#146: a Calm guard that has just stepped clear of a doorway
                // sometimes closes it behind itself. The guard is now off the panel
                // (it stands on `target`), so the crush check sees only anyone *else*
                // still in the throat — the player included — and refuses on them.
                if self.guards[i].closes_doors() {
                    if let Some(door) = self.door_exited(from, target) {
                        if self.rolls_a_close() && self.close_behind_door(door) {
                            events.push(Event::DoorClosed {
                                at: from,
                                by_player: false,
                            });
                        }
                    }
                }
            }
        }
    }
}
