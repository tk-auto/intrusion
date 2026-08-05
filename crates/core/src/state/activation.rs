//! The activation precondition ladder (§8.4/§11.4, #345): **one** answer to *would
//! pressing this key actually do anything right now?*
//!
//! Some abilities need more than the §8.2 economy before they can fire — a decoy
//! needs somewhere to stand, Pierce Wall needs exactly one adjacent wall, Lockdown
//! needs a door in reach, Confusion needs a guard in the blast. Each of those rules
//! already lived in its own pure derived function ([`decoy_spawn_cell`], [`bore_target`],
//! [`lockdown_doors`], [`confusion_blast`]); what did not exist was a single place
//! that *asked all of them*. So the turn loop asked them one by one inside
//! [`Input::Activate`], and the ability bar — which has no business re-deriving a
//! rule — asked none of them and drew `Bore(3)` in the middle of a room where the
//! press was a guaranteed free no-op.
//!
//! [`State::aim`] is that place. The turn loop calls it to decide whether the press
//! does anything (and gets the resolved target back, so nothing is measured twice),
//! and [`State::ability_state`](crate::State::ability_state) calls it to decide
//! whether the bar's entry is `Unusable` (§11.4). A future precondition is added by
//! extending this ladder, and both surfaces pick it up — which is the same
//! one-ladder discipline [`bump_kind`](State::bump_kind) already gives the usable
//! line.
//!
//! # This answers *can I*, never *should I*
//!
//! Whether an ability is **worth** pressing is bot policy and lives in `crates/sim`
//! (§13.2). Keeping the two apart is what stops the game crate growing a bot — and
//! it is why this ladder only ever asks questions the rules already ask.
//!
//! # What it costs to be asked every frame
//!
//! The bar is a pure derived function recomputed every frame (§11.4), so this must
//! stay cheap. The heaviest arm is Confusion's, which walks the guards once; the
//! rest are O(4 neighbours) or a pass over the door list. A future precondition that
//! is genuinely expensive should be memoised **here**, at the ladder, rather than at
//! any one call site.
//!
//! [`decoy_spawn_cell`]: State::decoy_spawn_cell
//! [`bore_target`]: State::bore_target
//! [`lockdown_doors`]: State::lockdown_doors
//! [`confusion_blast`]: State::confusion_blast

use super::*;

/// What a press has **already resolved** by the time the deck commits (§8.4:
/// targeting is settled up front, not discovered on refusal).
///
/// The turn loop hands this straight to the effect that consumes it, so the cell,
/// the door set and the blast the world change uses are the very ones the
/// precondition approved — a footprint can never disagree with what was measured.
///
/// At most **one** aimed effect per ability, which
/// [`no_ability_needs_two_targets`](super::tests::abilities::no_ability_needs_two_targets)
/// pins: an ability that needed two would need this to be a set, not a choice.
pub(super) enum Aimed {
    /// Nothing outside the §8.2 economy stands between the press and the effect —
    /// Run, Camouflage, Dephase, Autodoors, and every ability yet to grow a rule.
    Nothing,
    /// Where the decoy would stand (§8.3): the faced cell.
    Decoy(Cell),
    /// The wall Pierce Wall would open (§8.3/#303).
    Bore(Cell),
    /// The doors a Lockdown would seal (§8.3/#242).
    Seal(Vec<DoorId>),
    /// The blast Confusion would fire (§8.3/#325) — already known to catch at least
    /// one guard.
    Blast(EffectArea),
    /// The reach a False Call would broadcast over (§7.7/§8.3/#504) — already known to
    /// have a live net to travel down and at least one guard inside it.
    Call(EffectArea),
    /// The cell a control-transfer ability would launch its remote from (§8.1/#273):
    /// the player's own, because you let it go from your hands.
    ///
    /// It carries a cell it could have re-read from the state, for the reason every
    /// other arm does: what the effect acts on is settled by the precondition and handed
    /// to the world change, so a launch can never happen somewhere the ladder did not
    /// approve.
    Launch(Cell),
}

/// Why a press would not fire — the [`Err`] half of [`State::aim`], and the reason
/// the bar greys the entry (§11.4).
///
/// Every case is a **free** no-op (§4.4): a refused activation costs neither the
/// turn nor a use, and this type only decides what the player is *told*, never what
/// the press costs.
pub(super) enum Refused {
    /// The faced cell could not hold an intruder (§8.3) — a wall, or somebody
    /// already standing there.
    NoDecoyCell,
    /// Pierce Wall's geometry or supply says no (§8.3/#303), in its own words.
    Bore(BoreRefusal),
    /// No door within the lockdown box (§8.3/#242) — a window bought to seal
    /// nothing.
    NoDoorsInReach,
    /// The blast would catch no guard (§8.3/#325).
    BlastCatchesNobody,
    /// The radio net is dead (§7.3/§7.7/#504), so there is nothing to forge a call on.
    NetIsDead,
    /// There is no room to launch a remote, or to take one's controls, from where the
    /// player is standing (§10.7/#273) — a crawlspace.
    NoRoomToPilot,
    /// The player's hands are on a remote's controls (§8.1/#273), so this is not an
    /// ability they can press right now.
    HandsOnTheControls,
}

impl Refused {
    /// What the near line says about this refusal (§11.7), or `None` where the rule
    /// has nothing to teach.
    ///
    /// Silence is the decoy's alone, and it is the pre-existing behaviour rather
    /// than a judgement made here: the faced cell is drawn on the board, so *why*
    /// the fake has nowhere to stand is already on screen. The other three speak,
    /// because the rule they enforce is invisible — a supply, a box reaching through
    /// walls, a guard just outside it — and a press that changed nothing has to say
    /// why (§11.7).
    pub(super) fn event(self) -> Option<Event> {
        match self {
            Refused::NoDecoyCell => None,
            // Silent for the decoy's reason exactly: the rule is already on the board.
            // The remote is drawn under its own mark and every other bar entry is
            // greyed (§11.4), so a player who presses one has been shown why — and a
            // line per stray keypress would spend the one row the near line has on the
            // fact it is currently spending it on (§11.7).
            Refused::HandsOnTheControls => None,
            Refused::NoRoomToPilot => Some(Event::LaunchRefused),
            Refused::Bore(reason) => Some(Event::BoreRefused { reason }),
            Refused::NoDoorsInReach => Some(Event::LockdownRefused),
            Refused::BlastCatchesNobody => Some(Event::ConfusionMissed),
            Refused::NetIsDead => Some(Event::FalseCallDead),
        }
    }
}

impl State {
    /// Resolve everything a press of `id` needs beyond the §8.2 economy — the one
    /// precondition ladder (#345).
    ///
    /// `Ok` carries the resolved target for the turn loop to spend; `Err` is the
    /// free no-op (§4.4) and the bar's contextual `Unusable` (§11.4).
    ///
    /// **The economy is not asked here**, with one deliberate exception: Pierce
    /// Wall's supply, which [`bore_target`](Self::bore_target) has always folded
    /// into its own verdict so the near line can say *"the borer is spent"* rather
    /// than refusing in silence. That exception is invisible to the bar, which only
    /// ever consults this ladder once the economy has already said the ability is
    /// pressable.
    pub(super) fn aim(&self, id: AbilityId) -> Result<Aimed, Refused> {
        // **Flying comes first, and refuses everything else** (§8.1/#273). While the
        // player holds a remote's controls their hands are not free for any other tool,
        // so every ability but the one they are flying is refused here — which is what
        // greys the whole bar rather than leaving it advertising presses the loop will
        // swallow (§11.4). The remote's own ability is exempt because its key is the way
        // *out* of the mode, and it resolves to the toggle-off, not to this.
        if self.piloting() && !self.flying(id) {
            return Err(Refused::HandsOnTheControls);
        }
        // A control-transfer ability (§8.1/#273) launches from the player's own cell,
        // and asks only that the player is on their feet to do it — the precondition
        // that keeps the ability's cost real (§2.3), since a body in a crawlspace is a
        // body nothing can reach.
        if transfers_control(id) {
            return if self.can_take_controls() {
                Ok(Aimed::Launch(self.player))
            } else {
                Err(Refused::NoRoomToPilot)
            };
        }
        if declares(id, Effect::SpawnDecoy) {
            return self
                .decoy_spawn_cell()
                .map(Aimed::Decoy)
                .ok_or(Refused::NoDecoyCell);
        }
        // Pierce Wall's target is unique by precondition rather than aimed (§8.4/#303),
        // so the geometry *is* the ability; `bore_target` owns it, refusal reasons and
        // all.
        if id == AbilityId::PierceWall {
            return self.bore_target().map(Aimed::Bore).map_err(Refused::Bore);
        }
        if declares(id, Effect::SealDoors) {
            let doors = self.lockdown_doors();
            return if doors.is_empty() {
                Err(Refused::NoDoorsInReach)
            } else {
                Ok(Aimed::Seal(doors))
            };
        }
        // The forged call asks the economy for **one** thing only (§7.7/§8.3/#504): a
        // net to transmit on. A silenced facility has nothing listening, so the spoofer
        // dies with the radio it spoofs — the console's trade, stated where the press is
        // refused rather than discovered as a turn that bought nothing. That is a fact
        // about a switch the *player* threw, so saying it gives nothing away.
        //
        // **It deliberately does not ask whether anybody is in reach**, which is where
        // it parts company with Confusion's ladder above. Confusion may refuse an empty
        // blast because its reach is clamped inside the guard sense: everything it could
        // have caught was already drawn for the player, so the refusal tells them
        // nothing they did not know. This reach is **not** clamped — it covers guards
        // behind the §5 cone and, in a duct, well past the §9 sense — so a refusal here
        // would answer *"is anyone within ten cells of me?"*, and a greyed bar entry
        // would answer it every frame, for free, without spending the turn. That is a
        // detector, and this ability is a transmitter (§9 stays [SETTLED]: the player's
        // picture is sight and the sense, and nothing else). So the press always fires,
        // always costs its turn and its lockout, and what it found is the player's to
        // infer from what walks out of the dark.
        if declares(id, Effect::FakeCall) {
            return if self.radio_silenced() {
                Err(Refused::NetIsDead)
            } else {
                Ok(Aimed::Call(self.false_call_area()))
            };
        }
        if declares(id, Effect::Confuse) {
            let blast = self.confusion_blast();
            return if self.guards.iter().any(|g| blast.contains(g.pos())) {
                Ok(Aimed::Blast(blast))
            } else {
                Err(Refused::BlastCatchesNobody)
            };
        }
        Ok(Aimed::Nothing)
    }

    /// Whether pressing `id` right now would fire — [`aim`](Self::aim) with the
    /// resolved target thrown away. The bar's half of the ladder (§11.4).
    pub(super) fn would_fire(&self, id: AbilityId) -> bool {
        self.aim(id).is_ok()
    }
}
