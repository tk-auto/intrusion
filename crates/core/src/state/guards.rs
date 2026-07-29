//! Phase 3: the guards act (§4.2, §7).
//!
//! The third and last phase of a spent turn, split out of [`state.rs`](super) so the
//! turn loop reads as its three phases rather than as one of them. It folds this
//! turn's sight into every guard's mind ([`Guard::sense`](crate::Guard)), witnesses a
//! §15 Q5 dive, counts the facility alert's confirmed sightings (§7.3), runs the
//! found-body (§7.2), hideout (§10.3) and decoy (§8.3) scans, and finally moves each
//! guard — where contact with the player is capture (§4.5).
//!
//! # One reading, six passes
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
    /// Per guard: whether it is **dazed** (§8.3/#240/#325) — blinded and frozen by a
    /// Confusion blast it was standing in when the flash went off, counting down its own
    /// clock. Read through [`acts`](Self::acts), never directly, so no pass can forget
    /// the skip.
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
    /// A dazed guard takes **no** part (§8.3/#240/#325): it does not sense (so its state
    /// and lead pause rather than reset, for a clean resume, §8.2), does not witness a
    /// dive, finds no body, checks no cupboard, is not drawn by a decoy, and does not
    /// move — so it cannot capture by stepping into the player (§4.5). Confusion is no
    /// shield, though: a guard the blast never caught still moves and captures normally
    /// — including one that walks into the cells it passed through afterwards — and a
    /// frozen guard's cell stays solid, so there is no walking through it either.
    fn acts(&self, index: usize) -> bool {
        !self.suppressed[index]
    }
}

impl State {
    /// Phase 3 (§4.2): the guards *sense*, then *act*.
    ///
    /// Six passes over one [`GuardSenses`] reading, in an order that is part of the
    /// design (see the module header): sense and witness, count the sighting, find
    /// bodies, check hideouts, notice the decoy, then move. The sighting count sits
    /// directly behind the sense pass because that is where the fact it reads is
    /// produced — and ahead of the movement pass, so a rung the facility reaches this
    /// turn is already shortening dwells when the guards step (§7.3/§7.5).
    pub(super) fn guard_phase(&mut self, events: &mut Vec<Event>) {
        let senses = GuardSenses::read(self);
        self.sense_guards(&senses, events);
        self.watch_sightings(&senses, events);
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

    /// Pass 2 — the facility alert's sighting window (§7.3/§7.6): a turn in which
    /// **any** guard has the player in its **certain** zone is one contact turn, and
    /// [`SIGHTING_CONTACT_TURNS`](crate::alert::SIGHTING_CONTACT_TURNS) of them inside
    /// the sliding window make one confirmed sighting — the ladder's rung-1 trigger,
    /// and, on the third, its rung-2 one.
    ///
    /// The tally is **facility-wide, not per guard**: three guards catching one turn
    /// each still counts three, because what the ladder measures is how often the
    /// facility knew where you were. A **glimpse** counts nothing.
    ///
    /// A dazed guard (§8.3/#325) takes no part: it did not look this turn, so its
    /// contact is last turn's frozen reading and counting it would have a blinded
    /// guard reporting sightings for as long as the flash held it.
    fn watch_sightings(&mut self, senses: &GuardSenses, events: &mut Vec<Event>) {
        let certain = self
            .guards
            .iter()
            .enumerate()
            .any(|(index, guard)| senses.acts(index) && guard.certain_contact());
        if let Some(trigger) = self.alert.watch(self.turn, certain) {
            // A sighting is about *where the player was seen*, which is where they
            // stand right now — so that is the cell a reinforcement is sent to search
            // (#374). By the time one has walked in from the far side of the facility
            // the player is long gone, which is the point: the net closes on a stale
            // cell (§7.6), it does not track a live one.
            self.raise_alert(trigger, senses.player, events);
        }
    }

    /// Pass 3 — the found-body scan (§7.2): a body is *found* the first time any cone
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
            // The finders are tracked, not just counted: they are already standing
            // over the body with their own search running (§7.2), so the §7.7 call
            // must send *other* guards — a finder called to the body it just found
            // would restart its own sweep.
            let mut finders = Vec::new();
            for (index, guard) in self.guards.iter_mut().enumerate() {
                if !senses.acts(index) {
                    continue;
                }
                if guard.fov().contains(at) {
                    guard.find_body(at);
                    finders.push(index);
                }
            }
            if !finders.is_empty() {
                self.bodies[body_index].mark_found();
                events.push(Event::BodyFound { at });
                // The loudest event in the game is also the loudest thing the facility
                // can learn: straight to the top of the ladder (§7.3).
                self.raise_alert(AlertTrigger::BodyFound, at, events);
                self.call_in_body(at, &finders, events);
            }
        }
    }

    /// Pass 4 — the found-a-body-nearby check (§15 Q5, second half): a found body is
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

    /// Pass 5 — the decoy scan (§8.3, #105): a guard whose cone covers the decoy — and
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

    /// **The call-in** (§7.7): guard `i` was Chasing before it decided this turn —
    /// if it has just dropped into a search, the chase ended and it reports where
    /// contact broke. One other guard ([`SIGHTING_CALL_GUARDS`]) converges on that
    /// cell and searches it, exactly as a radio dispatch does (§7.3).
    ///
    /// Three properties the design leans on, all of which fall out of *when* this
    /// fires rather than from any machinery:
    ///
    /// - **Taking the chaser down before it loses you suppresses the call.** A
    ///   guard that is gone never decides, so it never reports — §7.7's "silence it
    ///   before it reports", for free (there is no report timer to interrupt).
    /// - **The reported cell is stale by construction.** It is where the guard last
    ///   had you, which is precisely where you are *not* — you just broke contact.
    /// - **A guard on the player is never sent** (the [`nearest_respondable`] filter),
    ///   and neither is the caller itself: it has its own search to run.
    ///
    /// Off by default — this is the `sighting_lost_calls_a_guard` modifier (§12.6),
    /// and with it off the loser still searches alone, exactly as before.
    fn call_in_lost_sighting(&mut self, caller: usize, events: &mut Vec<Event>) {
        if !self.modifiers.sighting_lost_calls_a_guard {
            return;
        }
        // A chase that ended is a guard now sweeping the cell it lost you at. Any
        // other outcome (still chasing, or a cold lead given up with no search —
        // §7.1's backstop) reports nothing: there is no position worth calling.
        if self.guards[caller].state() != GuardState::Alerted {
            return;
        }
        let Some(at) = self.guards[caller].focus() else {
            return;
        };
        // The caller is Alerted and so respondable itself; exclude it explicitly,
        // or a lone guard would "call in" its own search and restart it.
        if self.call_guards_to(at, &[caller], radio::SIGHTING_CALL_GUARDS) {
            events.push(Event::CalledIn { at });
        }
    }

    /// **The body call-in** (§7.7/§7.2): a body has just been discovered by
    /// `finders`, so two guards ([`BODY_CALL_GUARDS`]) converge on it and search.
    ///
    /// Finding a body is the loudest event in the game, and the *only* way this
    /// call is louder than a sighting's is **how many come** — same seam, same
    /// arrival behaviour, a bigger count. The finders themselves are excluded: they
    /// are already on it with their own §7.2 search running, which happens with the
    /// modifier off exactly as it does with it on.
    ///
    /// Fires once per body, from the discovery scan, so a body that stays in view
    /// does not re-call every turn — and a body concealed in a hideout is never
    /// discovered (§7.2), so it never calls anyone.
    fn call_in_body(&mut self, at: Cell, finders: &[usize], events: &mut Vec<Event>) {
        if !self.modifiers.body_found_calls_two_guards {
            return;
        }
        if self.call_guards_to(at, finders, radio::BODY_CALL_GUARDS) {
            events.push(Event::BodyCalledIn { at });
        }
    }

    /// Send up to `count` guards to search `at` — the one call seam every §7.7
    /// call-in and the §7.3 radio dispatch share. Picks the nearest respondable
    /// guards ([`nearest_respondable`], which never takes one that has the live
    /// player), skipping `exclude` — whoever is already dealing with this lead.
    ///
    /// Returns whether anybody was actually sent, so the caller only reports a call
    /// that someone answered: with nobody free the call is simply unanswered, never
    /// queued or retried (§7.7).
    ///
    /// **A killed net sends nobody** (§7.7): with the radio silenced at the comms
    /// console there is no channel for a call to travel down, so both cooperation
    /// call-ins stop firing here — one gate on the one shared seam, so no future call
    /// can forget it. What does *not* stop is a guard already on its way: an errand
    /// given before the net died is **finished**, not recalled. That is the deliberate
    /// choice of the two the design leaves open, and it follows §7.7's own rule that a
    /// call, once made, is never queued or retried — there is no channel to un-send it
    /// down either. It also keeps the console honest as counterplay rather than a panic
    /// button: silencing the net stops the *next* wave, it does not erase the search
    /// already bearing down on you (§2.3 — cost is load-bearing).
    ///
    /// The guard that made the discovery still searches on its own either way — that is
    /// §7.6/§7.2 behaviour, not a call — so a silenced facility is lonelier, never
    /// blind.
    fn call_guards_to(&mut self, at: Cell, exclude: &[usize], count: usize) -> bool {
        if self.radio_silenced {
            return false;
        }
        let sent: Vec<usize> = radio::nearest_respondable(&self.guards, at, self.guards.len())
            .into_iter()
            .filter(|g| !exclude.contains(g))
            .take(count)
            .collect();
        for g in &sent {
            self.guards[*g].respond_to(at);
        }
        !sent.is_empty()
    }

    /// Pass 6 — each guard `decide`s a step (§7.5) and takes it. A guard moving into
    /// the player's cell is a capture and ends the run (§4.5). Otherwise it moves onto
    /// any cell that admits it and holds no other actor; a guard with nowhere to go, or
    /// whose step is blocked, simply holds.
    fn move_guards(&mut self, senses: &GuardSenses, events: &mut Vec<Event>) {
        // A **sealed** door is solid to every guard's route (§8.3/§7.6/#242): the guard
        // cannot work a locked handle, so it plans the long way round rather than
        // walking into a doorway that will refuse it. That detour is the whole of what
        // the ability buys, and it is handed to the router as blocked cells rather than
        // stamped into terrain, because the *player's* routing is deliberately
        // unchanged by their own lock. The same for every guard this turn — a seal is a
        // fact about the level, not about who is looking — so it is read once here
        // rather than per guard. Empty on every turn no lockdown is running, which is
        // nearly all of them.
        let sealed = self.sealed_route_blocks();
        // The §7.5 dwell rule, read once for the whole pass: the facility alert
        // shortens the pause from rung 1 up (§7.3), and every guard this turn holds
        // to the same rule.
        let dwell = self.dwell_rule();
        // The §7.3 patrol style, likewise a fact about the *level* rather than about
        // any one guard: with the comms console bumped there is no coordination left to
        // divide the building, so every Calm guard wanders the whole of it.
        let style = self.patrol_style();
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
            // now stand. The sealed doorways read above join them as obstacles.
            let mut blocked: Vec<Cell> = self
                .guards
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, g)| g.pos())
                .collect();
            blocked.extend_from_slice(&sealed);
            // §7.7: a chase that ends *this turn* is what calls it in, so the state
            // is read either side of the decision. Chasing is exactly the certain
            // zone (§7.6) — an Investigating guard only ever had a glimpse and
            // reports nothing — and `decide` is the one place a chase can run out.
            let was_chasing = self.guards[i].state() == GuardState::Chasing;
            let step = self.guards[i].decide(facility, &blocked, &mut self.rng, dwell, style);
            if was_chasing {
                self.call_in_lost_sighting(i, events);
            }
            let Some(dir) = step else {
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
                // …unless the door is **locked** (§10.4/#242). A seal refuses the
                // guards' handle and only theirs, so the walk-in open simply does not
                // happen and the guard holds this turn. Its route already plans around
                // sealed doorways, so this is the arrival the route could not avoid — a
                // destination on the doorway itself, or a door sealed under its feet —
                // and it is a *wait*, never a deadlock: the window ends, and the door
                // opens on the ordinary bump the turn after.
                if self.door_locked_at(target) {
                    continue;
                }
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
