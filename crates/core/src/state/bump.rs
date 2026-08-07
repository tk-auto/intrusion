//! **The §4.3 interaction ladder** — what one bump means, in one priority order
//! shared by execution and prediction (#548).
//!
//! [`BumpKind`] is the classification, [`State::bump_kind`] produces it,
//! [`State::resolve_step`] performs it, and [`BumpKind::affordance`] labels it for
//! the usable line — one source of truth, so the label a player reads and the
//! effect a step has can never drift apart (§11.4). The satellites here are the
//! ladder's own: the walk arm ([`walk_into`](State::walk_into)), the facing checks
//! a hinge peek and a duct entry ask, the way-out classification, and the salvage
//! classifiers the crate arms read. `state.rs` keeps the player phase that calls
//! in; `traversal` keeps the #57 auto-slide it already carved off this same seam.

use super::*;

/// What bumping an orthogonally adjacent cell would do (§4.3) — the interaction a
/// cell offers, in the one priority order shared by execution and prediction. This
/// is the single source of truth [`State::bump_kind`] produces; `resolve_step`
/// performs the effect and `affordances` labels it, so the usable line can never
/// drift from the bump (§11.4). Purely a classification: it carries the target's
/// interaction, never a mutation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum BumpKind {
    /// A guard; `aware` is whether it detected the player this turn (§7.2).
    /// Unaware, the bump is the takedown; aware, it is a free no-op.
    Guard { aware: bool },
    /// The body currently being dragged: bumping it lets go — free (§4.4).
    /// Taking hold is not a bump — a body is non-solid and you grab it by
    /// walking over it and off its cell (§8.3, the [`BumpKind::Move`] arm).
    BodyRelease,
    /// An empty cupboard while dragging a body (§7.2/§10.3): the bump stows the
    /// body inside and locks the cupboard — it is no longer a hideout. A spent
    /// turn; the player stays put and their hands come free.
    DepositBody,
    /// The **exit**, answered by the run's intel gate (§4.5/§12.6, `ready` is
    /// [`exit_ready`](State::exit_ready)) — win vs. refused. Two bumps reach it since
    /// #466, and only one of them can win:
    ///
    /// - the step **off the board** from the tunnel's border cell, the one target that
    ///   is not a cell of the grid, classified by
    ///   [`way_out_kind`](State::way_out_kind) rather than by
    ///   [`bump_kind`](State::bump_kind), which only ever looks at cells;
    /// - the mouth `E` bumped short of the gate, which refuses at the near end rather
    ///   than sending the player down a crawl that will refuse at the far one. A bump
    ///   on `E` *with* the gate met is [`EnterExitDuct`](BumpKind::EnterExitDuct): the
    ///   climb in, not the win.
    Exit { ready: bool },
    /// The exit `E` bumped from the facility side **with the intel gate met**
    /// (§4.5/§10.7/#466): the inner mouth of the player's own tunnel, climbed into
    /// exactly as any duct entry is. Distinct from [`EnterDuct`](BumpKind::EnterDuct)
    /// only in what the usable line calls it — one of the two is a shortcut and the
    /// other is the way home, and the row that tells you what a bump does should not
    /// read the same for both. Short of the gate the same cell is
    /// [`Exit { ready: false }`](BumpKind::Exit) — the refusal.
    EnterExitDuct,
    /// An objective console still holding its intel.
    Intel,
    /// An **equipment cache** still holding its tech (§2.2/§8.3/§14 v3/#209): bumping it
    /// salvages the ability inside — usable from this turn on, and carried by the run
    /// into every facility after this one. A spent turn, like taking intel. Once opened
    /// the same cell classifies as [`Solid`](BumpKind::Solid): there is nothing left in
    /// it, so a second bump is a free no-op (§4.4) and the usable line offers nothing —
    /// the spent console's rule over the spent crate.
    Salvage,
    /// A cache holding tech the run already carries, whose **per-level budget it can
    /// refill** (§8.2/§8.3/#302/#266): the bump takes the crate and puts the uses back
    /// to what the level granted. A spent turn, like any other pickup — this is the one
    /// duplicate that pays out, and the only thing anywhere that moves a budget upward.
    SalvageRecharge,
    /// A cache holding tech the run **already carries** (§8.3/#209): the bump refuses,
    /// free, like a refused exit — and it is named apart from [`Solid`](BumpKind::Solid)
    /// for the reason [`HingeHeld`](BumpKind::HingeHeld) is: the cell is anything but
    /// inert, and the usable line owes the player the *reason* rather than a silence
    /// they would have to work out by pressing.
    SalvageRefused,
    /// A cache the run has **no room for** (§8.3/#266): the bump opens the
    /// [exchange](crate::Exchange) — the crate offers its tech and the run picks which
    /// of the four to drop. **Free**, like the refusal it replaces: opening an offer
    /// takes nothing and changes nothing (§4.4), and it is the trade that spends the
    /// turn, one turn, exactly as a plain [`Salvage`](BumpKind::Salvage) does.
    SalvageSwap,
    /// The comms console while the radio net is still live (§7.3/§7.7): bumping it
    /// silences the net for the rest of the level — a spent turn. Once silenced the
    /// same cell classifies as [`Solid`](BumpKind::Solid): there is nothing left to
    /// switch off, so a second bump is a free no-op (§4.4) and the usable line offers
    /// nothing.
    SilenceRadio,
    /// A door cell whose bump is a real door action (open, close, or a crush-refused
    /// close). Both handles of a closed door open it — a panel and, since #148, a
    /// hinge. Only an **open panel** is *not* a door action: it classifies as
    /// [`BumpKind::Move`], the walk-through, exactly as the bump resolves.
    Door { action: DoorAction },
    /// A **key-gated** doorway the player cannot work (§10.4/#236): the locked-room
    /// modifier's lock, bumped from the corridor side without a key. A free no-op —
    /// nothing moves, so nothing is charged (§4.4) — and named apart from
    /// [`Solid`](BumpKind::Solid) so the usable line can say *locked* rather than go
    /// quiet on a doorway that is plainly a doorway.
    ///
    /// Classified **before** every other door arm, so a lock is a lock: it outranks the
    /// ordinary open, the #148 frame bump and the Autodoors walk-through alike (§8.3 —
    /// the ability opens doors, it does not pick locks). An **open** keyed door is not
    /// here at all: what the lock refuses is the handle, never the doorway (§10.4), so
    /// a keyed door standing open is walked through like any other.
    DoorLocked,
    /// The **hinge of a door the player's immediately preceding action opened**
    /// (#320): the close is withheld for exactly that one action, so the frame is a
    /// **dead bump** — offered to the #57 lateral shift, which rounds the player onto
    /// the open panel — instead of shutting what they just opened. Named apart from
    /// [`Solid`](BumpKind::Solid) because the cell is anything but: the door is open,
    /// the close is merely one action away, and the §11.4 usable line must go quiet on
    /// it rather than promise a *close door* the bump will not deliver. Any other
    /// hinge bump on an open door — one already open, one a guard opened, one whose
    /// mark has expired — is the ordinary [`Door`](BumpKind::Door) close (§10.4).
    HingeHeld,
    /// A closed door **panel** the player walks straight through because the
    /// **Autodoors** ability is active (§8.3/§7.6): the door opens as they step into
    /// it — no manual bump — and is armed to shut behind them. Movement, not a
    /// standalone door op, so it obeys the drag half-speed and reuses the plain
    /// [`Move`](BumpKind::Move) step; classified apart only so the executor can open
    /// the door and arm the close first. Offered only when the opened cell is
    /// walkable — a hinge, which stays solid, is a plain [`Door`](BumpKind::Door).
    AutoDoor,
    /// An empty concealment cupboard to climb into (§10.3).
    Hide,
    /// A cupboard that cannot be entered — already holding an actor, or **locked**
    /// by a body stowed inside (§7.2): a free no-op bump either way.
    HideoutBlocked,
    /// A duct entry to climb into from its mouth (§10.7): bumping it enters the
    /// crawlspace, a spent turn. Offered only when the player is not already in a
    /// duct and not dragging a body (a body cannot follow into the walls).
    EnterDuct,
    /// A crawl one cell along the duct the player is inside (§10.7): a spent turn
    /// that moves them to the adjacent path cell. Not an interaction offered on the
    /// usable line — it is movement, like [`BumpKind::Move`].
    DuctCrawl,
    /// A partial-cover table not already crouched behind (§10.3).
    Crouch,
    /// The table already crouched behind — a free bump.
    CrouchHeld,
    /// Plain enterable floor — a normal move.
    Move,
    /// Anything else solid — a wall, a pillar: a free bump (§4.4). A closed hinge is
    /// *not* here anymore — since #148 it opens the door ([`BumpKind::Door`]).
    Solid,
}

impl BumpKind {
    /// The §11.4 usable-line label for this interaction, or `None` when a bump does
    /// nothing worth offering — a guard, a solid cell, a held pose, or a close that
    /// would be refused (doors never crush, so it is never promised).
    pub(super) fn affordance(self) -> Option<Affordance> {
        match self {
            BumpKind::Guard { aware: false } => Some(Affordance::Takedown),
            BumpKind::BodyRelease => Some(Affordance::ReleaseBody),
            BumpKind::DepositBody => Some(Affordance::StoreBody),
            BumpKind::Exit { ready: true } => Some(Affordance::Leave),
            BumpKind::Exit { ready: false } => Some(Affordance::ExitRefused),
            BumpKind::Intel => Some(Affordance::TakeIntel),
            BumpKind::Salvage => Some(Affordance::SalvageTech),
            BumpKind::SalvageRefused => Some(Affordance::SalvageCarried),
            BumpKind::SalvageRecharge => Some(Affordance::SalvageRecharge),
            BumpKind::SalvageSwap => Some(Affordance::SalvageSwap),
            BumpKind::SilenceRadio => Some(Affordance::SilenceRadio),
            BumpKind::Door {
                action: DoorAction::Opened,
            }
            // Autodoors opens as you step through — the usable line still reads
            // "open", truthfully, since that is what the step does to the door.
            | BumpKind::AutoDoor => Some(Affordance::OpenDoor),
            BumpKind::Door {
                action: DoorAction::Closed,
            } => Some(Affordance::CloseDoor),
            BumpKind::DoorLocked => Some(Affordance::LockedDoor),
            BumpKind::Hide => Some(Affordance::Hide),
            BumpKind::EnterDuct => Some(Affordance::EnterDuct),
            BumpKind::EnterExitDuct => Some(Affordance::EnterExit),
            BumpKind::Crouch => Some(Affordance::Crouch),
            // A crawl is movement, not an offered interaction (§10.7) — like a plain
            // Move it shows nothing on the usable line.
            BumpKind::Guard { aware: true }
            | BumpKind::Door {
                action: DoorAction::Obstructed,
            }
            // The withheld frame (#320) offers nothing: while the suppression stands
            // the bump will not close the door, so the line must not say it will.
            | BumpKind::HingeHeld
            | BumpKind::HideoutBlocked
            | BumpKind::DuctCrawl
            | BumpKind::CrouchHeld
            | BumpKind::Move
            | BumpKind::Solid => None,
        }
    }
}

impl State {
    /// Resolve a step into a move or a bump (§4.3), pushing the event and reporting
    /// whether the turn was spent.
    pub(super) fn resolve_step(&mut self, dir: Direction, events: &mut Vec<Event>) -> bool {
        // A step **off the board** is resolved first, because it has no target cell to
        // classify. On the exit tunnel's way-out cell, aimed outward, it is the win
        // check (§4.5/#466); everywhere else it is the free mis-input a wall bump has
        // always been (§4.3) — the border is solid wall, and there is nothing beyond it.
        //
        // One ladder decides what a bump does — the same `bump_kind` the usable line
        // reads — so execution and prediction can never disagree (§11.4). The match
        // below performs the effect; the classification and its priority live in one
        // place, and the off-board step gets the one classifier that can answer for a
        // target which is not a cell. It yields only `Exit`, whose arms read no target,
        // so the player's own cell stands in for one.
        let (target, kind) = match self.player.step(dir) {
            Some(cell) if self.layout.facility().in_bounds(cell) => (cell, self.bump_kind(cell)),
            _ => match self.way_out_kind(dir) {
                Some(kind) => (self.player, kind),
                None => return false,
            },
        };

        // The half-speed drag (§8.3): a step that would *move* the player while a
        // haul debt is owed pays the debt instead — the turn is spent hauling the
        // body along, and nothing moves. Interactions (doors, grabs, the exit) are
        // not movement and stay full price; free bumps stay free.
        if self.dragging.is_some()
            && self.drag_debt
            && matches!(kind, BumpKind::Move | BumpKind::Hide | BumpKind::AutoDoor)
        {
            self.drag_debt = false;
            return true;
        }

        match kind {
            // The takedown (§7.2): adjacent, against a guard that has not detected
            // the player this turn, costing the full turn. Permanent — the guard
            // is gone, and what remains is the body, which is the real cost. No
            // cooldown and no range: the constraints *are* the cost.
            BumpKind::Guard { aware: false } => {
                let i = self
                    .guard_at(target)
                    .expect("bump_kind classified a guard here");
                let guard = self.guards.remove(i);
                // The body inherits the downed guard's radio cadence (§7.3): the
                // clock that was silent while it lived starts ticking now, its
                // first missed ping a full period out. Where it falls is control's
                // last fix on the guard, and so where a responder will be sent.
                self.bodies
                    .push(Body::new(target, guard.radio_clock(), self.turn));
                events.push(Event::TakenDown { at: target });
                self.take_key_from_guard(target, events);
                true
            }
            // An aware guard has you in its cone (§7.2's gate): the bump is a
            // free no-op — no half-takedown, no shove.
            BumpKind::Guard { aware: true } => {
                events.push(Event::Bumped { into: target });
                false
            }
            // Letting the body go where it lies (§8.3): free, the §4.4 toggle-off
            // exception — and it refunds nothing, there is nothing to refund.
            BumpKind::BodyRelease => {
                self.dragging = None;
                self.drag_debt = false;
                events.push(Event::BodyReleased { at: target });
                false
            }
            // Stowing the dragged body into a cupboard (§7.2/§10.3): the body slides
            // inside and is *gone* — no cone finds it (the found-body scan skips a
            // hideout cell) — and the cupboard is now **locked**, no longer a
            // hideout. The player does not move (they stow it from the mouth); the
            // turn is spent (an interaction, not free) and their hands come free.
            BumpKind::DepositBody => {
                let i = self
                    .dragging
                    .expect("bump_kind classified DepositBody only while dragging");
                self.bodies[i].move_to(target);
                self.dragging = None;
                self.drag_debt = false;
                events.push(Event::BodyStored { at: target });
                true
            }
            // The way out (§4.5/#466): a step off the board from the tunnel's border
            // cell. Win if the intel gate is met, else refuse — a refused exit changes
            // nothing and is free (§4.5), and the refusal is the same one it always was;
            // only the cell you are standing on when you read it has moved.
            BumpKind::Exit { ready: true } => {
                self.outcome = Outcome::Won;
                events.push(Event::Won);
                true
            }
            BumpKind::Exit { ready: false } => {
                events.push(Event::ExitRefused {
                    still_needed: self.intel_needed_to_exit(),
                    gate: self.modifiers().intel_to_exit,
                });
                false
            }
            // An objective console: take the intel.
            BumpKind::Intel => {
                let obj = self
                    .objectives
                    .iter_mut()
                    .find(|o| o.cell == target && !o.taken)
                    .expect("bump_kind classified an untaken console here");
                obj.taken = true;
                // Both counts are read *after* the take, and they are not the same
                // number: what is still out is the tally, what the gate still wants is
                // the requirement (#310/§4.5).
                events.push(Event::IntelTaken {
                    remaining: self.objectives_remaining(),
                    still_needed: self.intel_needed_to_exit(),
                });
                // A console tampered with while the facility already knows you are in
                // it steps the alert ladder to rung 2 (§7.3): control now knows what
                // you came for. At rung 0 it triggers nothing at all — that is the
                // reward for staying unseen, and the reason a clean raid is quiet.
                if let Some(trigger) = self.alert.console_tampered() {
                    // The console is what control learned about, so it is where any
                    // reinforcement the rung sends is sent to search (#374) — a cell
                    // the player is standing next to *now* and will have left by the
                    // time anyone walks in, which is exactly §7.6's stale lead.
                    self.raise_alert(trigger, target, events);
                }
                true
            }
            // An equipment cache (§2.2/§8.3/§14 v3/#209): salvage the tech in it. The
            // ability joins the deck **now**, not at the end of the raid — §14 v3 asks
            // for a power curve, and one that only paid out after you had left would
            // make the detour a deposit rather than a find. It joins the *run* too: the
            // campaign layer reads it off the verdict ([`RunStats::salvaged`]) and folds
            // it into the loadout every later facility boots with (§2.2).
            //
            // Deliberately **quiet**. A crate is not a terminal control room reports on,
            // so opening one raises no alert (§7.3) — the console's `console_tampered`
            // step has no counterpart here. What it costs is the detour and the turn,
            // which is the §2.3 price the placement rule (`PLAYER_CACHE_MIN_DISTANCE`)
            // is there to make real.
            BumpKind::Salvage => {
                let cache = self
                    .caches
                    .iter_mut()
                    .find(|c| c.cell == target && !c.taken)
                    .expect("bump_kind classified an unopened cache here");
                cache.taken = true;
                let id = cache.holds;
                self.abilities.grant(id);
                events.push(Event::TechSalvaged { id });
                true
            }
            // A crate holding tech the run already carries, with a budget to put back
            // (§8.2/#302/#266). The crate is spent and the uses go to what the level
            // granted — the one payout a duplicate has, and the one thing that ever
            // moves a budget upward. A spent turn, priced exactly as the salvage it is a
            // version of: the detour, and the turn.
            BumpKind::SalvageRecharge => {
                let id = self.live_cache_at(target);
                if let Some(cache) = self.caches.iter_mut().find(|c| c.cell == target) {
                    cache.taken = true;
                }
                self.abilities.recharge(id);
                events.push(Event::UsesRecharged {
                    id,
                    uses: self.abilities.uses_left(id).unwrap_or(0),
                });
                self.waited = false;
                self.crouched_behind = None;
                self.drag_debt = false;
                true
            }
            // A crate holding tech the run already carries (§8.3/#209). **Free** (§4.4)
            // and the crate is left unopened, so nothing is spent and nothing is lost —
            // the refused exit's shape, and for the same reason: a bump that changes
            // nothing must cost nothing. It is the luck of a facility stocked before
            // anyone knew who was coming, and there is no decision in it: a second copy
            // of what you hold would change nothing whichever way you answered.
            BumpKind::SalvageRefused => {
                events.push(Event::SalvageRefused {
                    id: self.live_cache_at(target),
                });
                false
            }
            // A crate the run has no room for (§8.3/#266): the crate **offers**, and the
            // run is now standing at the exchange. Free, exactly as the refusal it
            // replaces was — the crate is still shut, the loadout is untouched and no
            // turn has been spent. What has changed is that the game is waiting: until
            // the offer is answered, [`player_phase`](Self::player_phase) takes nothing
            // but the discard.
            //
            // Opening it is **idempotent** in the way that matters: the offer names the
            // crate, so a second bump on the same crate re-states the same offer rather
            // than stacking a second one.
            BumpKind::SalvageSwap => {
                let id = self.live_cache_at(target);
                self.exchange = Some(Exchange::new(id, target));
                events.push(Event::ExchangeOffered { id });
                false
            }
            // The comms console (§7.3/§7.7): one bump kills the radio net for the rest
            // of the level. A spent turn — the counterplay costs the detour that got
            // you here plus this turn, and nothing else, because the flag is one-way
            // and there is no upkeep to pay.
            BumpKind::SilenceRadio => {
                self.radio_silenced = true;
                events.push(Event::CommsSilenced { at: target });
                true
            }
            // A door (§4.3, §10.4): opening or closing spends the turn. An obstructed
            // close changed nothing — free; doors never crush.
            BumpKind::Door { action } => match action {
                DoorAction::Opened => {
                    self.operate_door(target);
                    // Remember a *hinge* open (#320) so the very next bump on this
                    // frame slides past the door instead of shutting it again. Only
                    // the hinge open is marked: opening from a panel leaves the player
                    // facing an open panel they walk through, never a frame they could
                    // bump next, so there is nothing there to catch on.
                    self.door_just_opened = self.hinge_door_at(target);
                    // Frame bump (#148): opening from a *hinge* turns the player to
                    // face along the door line toward the panels, so the recomputed
                    // FOV + #121 peek leans through the doorway from beside it — the
                    // "crack the door and peek from cover" move, reading the room
                    // without ever standing in the new sightline. A §5 exception, on
                    // the same footing as the #89 hideout-entry auto-face. Opening
                    // from a panel is not a hinge bump and leaves facing to §5 (an
                    // open door is not a move).
                    if let Some(peek) = self.hinge_peek_facing(target) {
                        self.facing = peek;
                    }
                    events.push(Event::DoorOpened {
                        at: target,
                        by_player: true,
                    });
                    true
                }
                DoorAction::Closed => {
                    self.operate_door(target);
                    true
                }
                DoorAction::Obstructed => false,
            },
            // A door the key gate refuses (§10.4/#236): free, like every other bump that
            // changes nothing (§4.4). The `Bumped` event is the same one a wall raises —
            // the usable line has already said *locked*, one row down, every turn the
            // player stood there, so a message of its own would be repeating a fact they
            // read before they pressed.
            BumpKind::DoorLocked => {
                events.push(Event::Bumped { into: target });
                false
            }
            // Autodoors (§8.3/§7.6): the closed door in the player's path opens as
            // they step into it — no manual bump — and the same step carries them
            // through onto the panel, saving the open-turn. The door is armed to
            // swing shut behind them once the throat clears (the world phase's
            // [`close_armed_autodoors`]). The move itself is an ordinary
            // [`walk_into`], so dragging, facing (§5) and a Run sprint compose exactly
            // as they do through an already-open door.
            BumpKind::AutoDoor => {
                self.operate_door(target); // the door was closed, so this opens it
                self.arm_autodoor_close(target);
                events.push(Event::DoorOpened {
                    at: target,
                    by_player: true,
                });
                self.walk_into(dir, target, events);
                true
            }
            // A hideout: bump the empty cupboard to climb in (§4.3, §10.3). Unlike
            // floor, you do not drift onto it — entering is a *decision*. It moves you
            // into the cell, spends the turn, and conceals you ([`hidden`](Self::hidden));
            // climbing out is an ordinary step off, no special case. Its whole cost is
            // time: while you hide you make no progress and the clock keeps ticking (§2.3).
            BumpKind::Hide => {
                self.haul_body_to(self.player);
                self.player = target;
                self.stomp_decoy(target, events);
                // The §5 exception for the hideout interaction (§7.6/§10.3): entry
                // faces *out* of the cupboard, back toward the corridor — the
                // opposite of the entry bump, which points into the wall the hideout
                // is recessed in. So the ~180° half-disc (§6.2, arc 3) watches the
                // flight path the instant you hide, not the wall behind you, and you
                // get the "hold still, watch the cone sweep" moment without wasting a
                // turn re-aiming (there is no turn-in-place, §5). This is *not* a
                // general turn-in-place: only the Hide entry sets a meaningful facing;
                // climbing back out is an ordinary step whose facing follows its own
                // direction (see `BumpKind::Move`).
                self.facing = dir.opposite();
                // Mark the entry so phase 3 can decide which guards *witnessed* it
                // (§15 Q5): an alerted guard whose cone covers this cell saw the dive.
                self.entered_hideout = Some(target);
                events.push(Event::EnteredHideout { at: target });
                true
            }
            // A duct entry: climb in from the mouth (§4.3, §10.7). Like the hideout
            // this is a *decision*, not a drift — it moves the player onto the entry
            // cell, spends the turn, and conceals them. Facing is set out the mouth
            // (back the way you came, `dir.opposite()`) so the entry-cell peek (§6.1)
            // leans through the mouth to read the room before you climb out. No body
            // can be in hand here (`bump_kind` refuses EnterDuct while dragging), so
            // there is nothing to haul.
            // The exit `E` from the facility side (§4.5/#466): climbing into the tunnel
            // you dug. Mechanically the duct entry above — the same spent turn, the same
            // concealment, the same mouth peek — so it shares its arm. What differs is
            // only what the usable line called it on the way in (`exit: enter`).
            BumpKind::EnterDuct | BumpKind::EnterExitDuct => {
                self.player = target;
                // Record *which* duct we climbed into — the entry belongs to exactly
                // one (§10.7), and from here "in a duct" is this stored index, not the
                // cell (an interior cell may overlie floor the player could also walk).
                self.in_duct = self.layout.duct_index_containing(target);
                // Facing out the mouth, read from the duct itself rather than from the
                // bump, so the stored facing and the peek that is cast from it can never
                // disagree: for a recessed entry that is `dir.opposite()` anyway, and for
                // the exit `E` — whose mouth is the whole room it comes up in — it is the
                // tunnel's own axis, the way a crawl arrives. The fallback covers a
                // hand-built duct with no derivable mouth.
                self.facing = self.duct_entry_facing(target).unwrap_or(dir.opposite());
                events.push(Event::EnteredDuct {
                    at: target,
                    own_tunnel: matches!(kind, BumpKind::EnterExitDuct),
                });
                true
            }
            // A crawl one cell along the duct (§10.7): a spent turn that moves the
            // player to the adjacent path cell, staying concealed. Landing on an
            // entry re-aims facing out its mouth (the peek); a mid-duct cell just
            // faces the crawl direction. Guards and bodies can never be inside a
            // duct, so none of the move-arm side effects (haul, decoy stomp, the
            // Run extra step) apply.
            BumpKind::DuctCrawl => {
                self.player = target;
                self.facing = self.duct_entry_facing(target).unwrap_or(dir);
                events.push(Event::DuctCrawled { to: target });
                true
            }
            // A table: bump it to crouch behind it (§4.3, §10.3). Ducking is a
            // *decision*, aimed at a specific table — and it anchors the crouch to
            // that table's whole run (the §10.1a bench), which is what conceals.
            // The player does not move; the tables stay solid furniture.
            BumpKind::Crouch => {
                self.crouched_behind = Some(target);
                events.push(Event::Crouched { behind: target });
                true
            }
            // Plain movement into a cell that admits the player — floor, an open
            // doorway, or a cell holding a *loose* body (non-solid, §7.2: you walk
            // over it).
            BumpKind::Move => {
                self.walk_into(dir, target, events);
                true
            }
            // A cupboard already holding an actor or locked by a stowed body, the
            // table already crouched behind, the frame of the door just opened
            // (#320), or anything else solid (a wall, a pillar): a free bump (§4.4).
            // A closed hinge is no longer here — it opens the door now (#148,
            // `BumpKind::Door`). Before it no-ops, the traversal experiment (#57) gets
            // a chance to read the dead bump as an unambiguous sidestep and slide one
            // cell past it; only if it declines does the bump fall through to the free
            // wall-bump it has always been — and a declined slide never closes the
            // just-opened door in the same breath, which is the whole point of #320.
            BumpKind::HingeHeld
            | BumpKind::HideoutBlocked
            | BumpKind::CrouchHeld
            | BumpKind::Solid => {
                if self.try_lateral_shift(dir, events) {
                    true
                } else {
                    events.push(Event::Bumped { into: target });
                    false
                }
            }
        }
    }

    /// Move the player one cell into `target` — the plain-move executor (§4.3/§5)
    /// shared by [`BumpKind::Move`] and, once the door is open, [`BumpKind::AutoDoor`].
    /// It hauls a dragged body into the vacated cell, faces the step (§5), takes hold
    /// of a body stepped off (§8.3), stomps a decoy underfoot, and lets a Run sprint
    /// add its extra cell — every consequence of a step, in one place so a walk
    /// through an auto-opened door behaves exactly as one through an already-open one.
    pub(super) fn walk_into(&mut self, dir: Direction, target: Cell, events: &mut Vec<Event>) {
        // A plain move while inside a duct is the climb-out at a mouth (§10.7) — the
        // only Move the confinement in `bump_kind` admits — so leaving the crawlspace
        // clears the stored state. (A phase-out, the one other way a Move fires from a
        // duct cell, ends the crawl just the same.)
        self.in_duct = None;
        // And a step onto the facility's floor is the run beginning in earnest
        // (§4.5/#466): from here the usable line offers the way out, which it holds back
        // while the player has still never left the tunnel they started in.
        self.entered_the_facility = true;
        let vacated = self.player;
        self.haul_body_to(vacated);
        self.player = target;
        self.facing = dir; // facing follows the last successful step (§5)
        events.push(Event::Moved { to: target });
        // **Stepping off a body no longer takes hold** (§8.3/#451). It used to, and the
        // grab landed on the one turn you least wanted it: you could not cross a body
        // at all without picking it up, and the drag that followed cost half speed —
        // so the accident happened mid-escape, over the guard you had just put down.
        // Taking hold is now a **wait** spent standing on the body ([`take_body`]),
        // which is a turn you chose to spend. Walking over one is walking over one.
        //
        // [`take_body`]: Self::take_body
        //
        // Stepping onto your own decoy kills it (§8.3) — anything's step does, the
        // maker's included; a sprint checks its second cell too.
        self.stomp_decoy(target, events);
        self.run_extra_step(dir, events);
    }

    /// The facing a **frame bump** (#148) turns the player to: from the bumped
    /// `hinge`, the direction toward the door's panels — the cell one step *into*
    /// the doorway. Facing along the door line, the ~180° half-disc and its #121
    /// peek lean through the opening, so the player reads the room they just cracked
    /// from beside it (§6, §10.4). `None` when `target` is not a hinge — a panel
    /// open (or any non-door cell) leaves facing to §5, so the caller applies this
    /// only for the hinge case.
    pub(super) fn hinge_peek_facing(&self, target: Cell) -> Option<Direction> {
        let door = self.layout.regions().door(self.hinge_door_at(target)?);
        // A door is a straight line of hinges around panels, so exactly one panel is
        // orthogonally adjacent to each end hinge: that neighbour is the way in.
        let panel = door
            .panels()
            .iter()
            .copied()
            .find(|&p| target.manhattan_distance(p) == 1)?;
        Direction::between(target, panel)
    }

    /// The direction out the **mouth** of the duct entry at `cell` (§10.7), or `None`
    /// if `cell` is not a duct entry. An entry has exactly one floor neighbour — the
    /// mouth — by its recessed geometry (§10.1.6), so this is the direction the
    /// entry-cell peek (§6.1) leans through and the facing a player landing on the
    /// entry takes, so the ~180° window watches the room before they climb out.
    pub(super) fn duct_entry_facing(&self, cell: Cell) -> Option<Direction> {
        // `cell` is always on the duct the player is currently crawling (this is only
        // ever called for the occupied duct's cells), so read the mouth from that duct
        // rather than searching by position — an interior cell may overlie floor that
        // belongs to no duct's geometry.
        let duct = self.occupied_duct()?;
        if !duct.is_entry(cell) {
            return None;
        }
        // The exit `E` (§4.5/#466) is the one entry with no *recessed* mouth: it is a
        // cell of the room it comes up in, so it may have floor on several sides and
        // "the single floor neighbour" has no answer. Its mouth is the tunnel's own
        // **axis** — you come up looking the way the tunnel points, which is also the
        // direction a crawl arrives from, so the peek reads the room ahead of you.
        if duct.way_out().is_some() {
            return Direction::between(duct.cells()[1], cell);
        }
        let facility = self.layout.facility();
        let mouth = facility
            .neighbours(cell)
            .find(|&n| facility.can_enter(n, ACTOR_FILL))?;
        Direction::between(cell, mouth)
    }

    /// What a step **off the board** would do (§4.5/§10.7/#466) — the classifier for
    /// the one target that is not a cell, and the twin of [`bump_kind`](Self::bump_kind)
    /// that both [`resolve_step`](Self::resolve_step) (which executes) and
    /// [`affordances`](Self::affordances) (which labels the §11.4 usable line) read, so
    /// the row can no more promise a wrong exit than a wrong door.
    ///
    /// `Some(BumpKind::Exit { .. })` only from the exit tunnel's **way-out cell**, aimed
    /// **outward** — off the level border, the direction the tunnel points. Everywhere
    /// else, and in every other direction, a step off the grid is the free mis-input it
    /// has always been (§4.3): the border is wall, and there is nothing beyond it.
    ///
    /// **Phased, it offers nothing** (§8.3): while Dephase is up there is no bump at
    /// all — no door opens, no intel is taken, the exit does not win — and walking out
    /// through your own tunnel's wall is not an exception to that.
    pub(super) fn way_out_kind(&self, dir: Direction) -> Option<BumpKind> {
        if self.abilities.effect_active(Effect::Phase) {
            return None;
        }
        let duct = self.occupied_duct()?;
        if duct.way_out() != Some(self.player) {
            return None;
        }
        // Outward is *away from the tunnel*: the way-out cell's one neighbour on the
        // path is the cell behind it, so the step that leaves the board is the opposite.
        let inward = Direction::between(self.player, duct.cells()[duct.cells().len() - 2])?;
        (dir == inward.opposite()).then_some(BumpKind::Exit {
            ready: self.exit_ready(),
        })
    }

    /// **What a bump on a crate holding `id` would do** (§8.3/#209/#266) — the three
    /// answers a live cache has, decided in this order:
    ///
    /// - **Already carried** ([`SalvageRefused`](BumpKind::SalvageRefused), or
    ///   [`SalvageRecharge`](BumpKind::SalvageRecharge)). The crate holds tech the run
    ///   has. A facility is stocked from its own seed and knows nothing of who is coming
    ///   (#209), so this is luck rather than design. Asked **first**, because it is the
    ///   more specific answer and because it is the one case where full hands are beside
    ///   the point: there is nothing here worth trading *for*, so offering the exchange
    ///   would be offering a decision whose every branch is a loss.
    ///
    ///   It pays out in exactly one case (§8.2/#302/#266): the tech has a **per-level use
    ///   budget** and the run has spent some of it, in which case the second copy refills
    ///   it. Otherwise a duplicate restores nothing and the bump is free.
    /// - **No room** ([`SalvageSwap`](BumpKind::SalvageSwap)). The run already carries
    ///   [`AbilityId::MAX_TECH_HELD`] pieces of tech, which §8.3 settles as the most a
    ///   run holds at once — it is what keeps the held set small enough for the ability
    ///   bar to name every entry on one row (§11.4), and what a passive pays with. The
    ///   cap is kept **here, at the crate**, because this is the one moment the player
    ///   can be told; what it now costs is a choice rather than the find.
    /// - **Room for it** ([`Salvage`](BumpKind::Salvage)) — the plain pickup.
    pub(super) fn salvage_kind(&self, id: AbilityId) -> BumpKind {
        if self.abilities.loadout().contains(id) {
            // A duplicate pays out exactly once: when the tech it duplicates has a
            // per-level budget with something missing from it (§8.2/#302/#266). With the
            // budget full — or with no budget at all — a second copy restores nothing,
            // and the bump goes back to being the free refusal it has always been.
            if self.abilities.uses_spent(id) {
                BumpKind::SalvageRecharge
            } else {
                BumpKind::SalvageRefused
            }
        } else if self.abilities.loadout().tech_held() >= AbilityId::MAX_TECH_HELD {
            BumpKind::SalvageSwap
        } else {
            BumpKind::Salvage
        }
    }

    /// What the unopened crate at `cell` holds — for the arms that have already been
    /// told by [`bump_kind`](Self::bump_kind) that one is standing there.
    pub(super) fn live_cache_at(&self, cell: Cell) -> AbilityId {
        self.caches
            .iter()
            .find(|c| c.cell == cell && !c.taken)
            .expect("bump_kind classified a live cache here")
            .holds
    }

    /// What bumping the orthogonally adjacent `target` would do (§4.3) — the **single**
    /// interaction ladder, read-only, that both [`resolve_step`](Self::resolve_step)
    /// (which executes) and [`affordances`](Self::affordances) (which labels the §11.4
    /// usable line) consume. Naming the interaction in one place is what keeps the
    /// arrow labels from ever promising what a bump won't deliver.
    ///
    /// The arms below are in priority order — exit → intel → door → hideout → table →
    /// move → bump. A new interaction is added as one [`BumpKind`] variant classified
    /// here; Rust's exhaustive matching then forces *both* consumers — the executor in
    /// `resolve_step` and the label in [`BumpKind::affordance`] — to handle it, so
    /// neither can silently drift (the §7.2 takedown slots in exactly this way). This
    /// classifies only; the mutation lives in `resolve_step`, so it can stay `&self`.
    pub(super) fn bump_kind(&self, target: Cell) -> BumpKind {
        // Dephase (§8.3, [`Effect::Phase`]): while phased there *is* no bump —
        // every in-bounds cell is a plain move, walls, doors, furniture, guards
        // and bodies included (the player's fill is effectively 0, §4.3). This
        // one short-circuit is the whole "cannot bump" rule: no door opens, no
        // intel is taken, the exit does not win, no takedown, no grab, no climb
        // — you pass straight through everything you came for. And because the
        // usable line reads this same ladder, it truthfully offers nothing
        // while phased (§11.4).
        if self.abilities.effect_active(Effect::Phase) && self.layout.facility().in_bounds(target) {
            return BumpKind::Move;
        }
        // Inside a duct the player is confined to the crawlspace (§10.7): a step to
        // the next path cell crawls, a step off an **entry** onto its floor mouth
        // climbs out (a plain Move), and every other step is walled in — a solid
        // bump. This confinement is what keeps an interior duct cell that happens to
        // touch floor from ever being an unintended exit; you leave only at a mouth.
        if let Some(duct) = self.occupied_duct() {
            if duct.is_crawl_step(self.player, target) {
                return BumpKind::DuctCrawl;
            }
            // The only way off the path is stepping from an **entry** onto its floor
            // mouth. An interior cell that happens to overlie floor (§10.7 cross-room
            // routing) is never an exit — only an entry's mouth is, which the recessed
            // geometry keeps as the entry's single non-solid neighbour.
            if duct.is_entry(self.player) && self.layout.facility().can_enter(target, ACTOR_FILL) {
                return BumpKind::Move; // climb out through the mouth
            }
            return BumpKind::Solid;
        }
        if let Some(i) = self.guard_at(target) {
            return BumpKind::Guard {
                aware: self.guard_detects_now(&self.guards[i]),
            };
        }
        // The body **in hand** is the one interaction a body offers: bump it to let
        // go (§8.3). A body is otherwise non-solid (§7.2) — a *loose* body is not
        // caught here; it falls through to a plain move (you walk over it, and grab
        // it by stepping *off* its cell, the [`BumpKind::Move`] arm). Stowing a body
        // into a cupboard is the Hideout arm below, not a bump on the body itself.
        if let Some(held) = self.dragging {
            if self.bodies[held].cell() == target {
                return BumpKind::BodyRelease;
            }
        }
        // The exit `E` from the facility side (§4.5/#466): the inner mouth of the
        // player's own tunnel.
        //
        // **The intel gate is answered here**, at the mouth, not at the far end of the
        // crawl: short of it the bump refuses exactly as bumping the exit always did —
        // free, with the §4.5 message — rather than letting the player crawl four cells
        // to be told no somewhere they can do nothing about it. With the gate met the
        // bump **climbs in**, exactly as a §10.7 shortcut's entry does, and the win is
        // the step off the board at the far end ([`way_out_kind`](Self::way_out_kind)),
        // which is the one the run *starts* inside and so still answers the gate itself.
        //
        // Like any duct entry it refuses a dragging player: a body cannot follow into
        // the walls, so let it go before you leave.
        if target == self.exit {
            return if self.dragging.is_some() {
                BumpKind::Solid
            } else if self.exit_ready() {
                BumpKind::EnterExitDuct
            } else {
                BumpKind::Exit { ready: false }
            };
        }
        if self.objectives.iter().any(|o| o.cell == target && !o.taken) {
            return BumpKind::Intel;
        }
        // An equipment cache (§2.2/§14 v3/#209), while it still holds its tech. An opened
        // crate falls through to the plain solid bump below, on the spent console's own
        // terms. A live one is never silent: taking it, trading for it (#266) and being
        // refused it are three kinds of their own, because the usable line has to say
        // which (§11.4/§2.3).
        if let Some(cache) = self.caches.iter().find(|c| c.cell == target && !c.taken) {
            return self.salvage_kind(cache.holds);
        }
        // The comms console (§7.3/§7.7), while the net is still live. A silenced one
        // falls through to the plain solid bump below — spent scenery, offering
        // nothing, which is precisely how the design wants it to read.
        if self.comms_console == Some(target) && !self.radio_silenced {
            return BumpKind::SilenceRadio;
        }
        if let Some(action) = self.layout.preview_door_bump(target, |c| self.occupied(c)) {
            // The locked prize room (§10.4/#236), ahead of every other door arm: a key
            // gate refuses the handle, so there is no open to withhold, no frame to
            // slide past and no Autodoors step to take. Only the *closed* door is
            // reached here at all — an open panel never previews as a door action, so a
            // keyed door standing open is walked through like any other (the slip-in).
            if self.key_gate_refuses(target) {
                return BumpKind::DoorLocked;
            }
            // The withheld frame (#320): the hinge of the door the player's *previous*
            // action opened does not shut it — for that one action it is a dead bump
            // that the #57 slide can round instead, so a player walking into a doorway
            // slightly off-line gets through rather than undoing their own open. Only
            // the close is withheld; an obstructed one is already a free no-op (doors
            // never crush) and is left exactly as it was.
            if action == DoorAction::Closed && self.frame_bump_withheld(target) {
                return BumpKind::HingeHeld;
            }
            // Autodoors (§8.3/§7.6): a closed door in the path opens and is walked
            // through in one step — but only when the opened cell is a walkable panel.
            // A hinge (#148) also opens the door on a bump, yet the hinge itself stays
            // solid, so there is nothing to step onto: that remains a plain door op.
            if action == DoorAction::Opened
                && self.abilities.effect_active(Effect::AutoDoors)
                && self.door_panel_at(target)
            {
                return BumpKind::AutoDoor;
            }
            return BumpKind::Door { action };
        }
        match self.layout.facility().terrain(target) {
            Some(Terrain::Hideout) => {
                // A cupboard holding a stowed body is **locked** — no longer a
                // hideout (§7.2) — and one holding an actor is occupied: either way
                // it refuses entry. Empty, a dragging player **stows** their body in
                // it (deposit-and-lock); hands free, they climb in to hide (§10.3).
                if self.body_at(target).is_some() || self.occupied(target) {
                    BumpKind::HideoutBlocked
                } else if self.dragging.is_some() {
                    BumpKind::DepositBody
                } else {
                    BumpKind::Hide
                }
            }
            Some(Terrain::DuctEntry) => {
                // Standing at the mouth, bump the entry to climb in (§10.7) — a
                // decision, like the cupboard. A body cannot follow into the walls,
                // so a dragging player is refused (the entry reads solid); let go
                // first. (Reached only when *not* already in a duct — the in-duct
                // confinement above owns crawling and climbing out.)
                if self.dragging.is_some() {
                    BumpKind::Solid
                } else {
                    BumpKind::EnterDuct
                }
            }
            Some(Terrain::PartialCover) => {
                // Any table of the run already crouched behind is the held pose
                // (§10.3 — the bench is one piece of furniture); a different
                // run's table re-anchors the crouch there.
                if self.crouch_covers(target) {
                    BumpKind::CrouchHeld
                } else {
                    BumpKind::Crouch
                }
            }
            _ if self.layout.facility().can_enter(target, ACTOR_FILL) => BumpKind::Move,
            _ => BumpKind::Solid,
        }
    }
}
