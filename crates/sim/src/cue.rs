//! **Ability cues** (§13.2/§8.1): each ability answers, for itself, whether this
//! is a moment it is *for*.
//!
//! The bot used to know exactly two abilities, as hard-coded arms inside its flee
//! and take-cover routines. Every other ability was **dead by omission** — and a
//! false zero in the §13.2 usage histogram is indistinguishable from a dead
//! ability, so the one metric that "would have caught the free neutralise on day
//! one" quietly stopped measuring the game.
//!
//! This module is the seam that fixes that. [`Moment::bid`] matches
//! **exhaustively** on [`AbilityId`], so adding a row to the §8.1 catalog fails to
//! *compile* until somebody says what that ability is for. The compile-time
//! obligation is the whole point: it is the same move §8.1 makes with
//! `Behaviour::Effects` — a small declared vocabulary you cannot silently skip.
//!
//! # The cue takes the bot's **intent**, not just the state
//!
//! Run is right when fleeing and wrong when pursuing. That is a fact about the
//! *plan*, not about the world, and the bot's policy already computes the plan —
//! so it names it ([`Intent`]) and hands it over. Without that, every cue would
//! have to re-derive "am I being hunted?", which is precisely the duplication the
//! seam exists to delete.
//!
//! # A cue returns a **bid**, not a bare number
//!
//! A [`Bid`] carries the concrete [`Input`] to issue (there is no second place that
//! turns an ability into a keypress), a **reason** (§13.3: a flagged signal has to
//! be traceable back to *why*), the turns of follow-through the cue is committing
//! to ([`Bid::then_hold`]), and an [**urge**](Bid::urge) on the anchored scale
//! below.
//!
//! # Legality is core's answer, never re-derived here
//!
//! A cue is handed the ability's live [`AbilityStatus`] — whose state is the
//! *contextual* one (#345): `Unusable` when a held, economically-available ability
//! would be refused for want of a target. Both bid constructors ([`Moment::press`],
//! [`Moment::hold`]) refuse to build a bid the state says would not fire, so **a
//! cue offered for an ability that cannot fire is a bug, not a low bid** — and it
//! is a bug this module makes unrepresentable rather than one a reviewer has to
//! catch.
//!
//! # The known cost of the architecture
//!
//! Once cues exist, a near-zero histogram slot means "weak ability **or** shy cue".
//! That ambiguity is real and worth paying for; the instrument that resolves it is
//! sweeping the per-ability floor ([`Profile::cue_floor`]) and reading the curve —
//! a flat curve exonerates the cue.

use intrusion_core::{
    AbilityId, AbilityState, AbilityStatus, Cell, Direction, GuardState, Input, State, Terrain,
};

use crate::profile::Profile;

/// What the bot is **trying to do** this turn — the plan its policy has already
/// settled on, named so a cue can be asked "is this a moment for you?" against it
/// rather than re-deriving the situation from [`State`].
///
/// The four are the branches of the bot's own loop (§13.2): break contact, duck a
/// closing patrol, push for the objective, sweep for one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Intent {
    /// A guard has the player, or is about to (§7.6): break contact, whatever it
    /// costs.
    Flee,
    /// Not seen yet, but a patrol is closing: get out of its way before it looks
    /// (§7.6/§10.3).
    TakeCover,
    /// Head for the objective — a known console, or the exit once the intel is in
    /// hand (§4.5).
    Pursue,
    /// Nothing known to head for: sweep the facility until the consoles show
    /// themselves (§11.5a — the bot cannot route to intel it has never seen).
    Explore,
}

/// What a crossing must save the router before it is worth **a third of the level's
/// borer** **[START]** — three times the margin a phase asks for
/// ([`crate::bot::CROSSING_MARGIN`]).
///
/// The two abilities answer the same question ("is there a better way through this
/// wall?") out of very different pockets: Dephase spends a cooldown that comes back,
/// Pierce Wall spends one of three permanent uses that does not (§8.2). The scarcer
/// supply asks the bigger price, which is also what keeps the two cues from bidding
/// for the same ordinary shortcut.
pub const BORE_MARGIN: u64 = 3 * crate::bot::CROSSING_MARGIN;

/// **No fit at all**: this is not a moment for the ability. Never pressed,
/// whatever the floor is set to — a zero urge is the same as declining to bid.
pub const URGE_NONE: u8 = 0;

/// How much room a Dephase crossing wants from the nearest perceived guard
/// (**[START] = 3**): one cell for each turn the *crossing* commits the bot — press,
/// in, out — so a guard walking at the §7.1 one cell per turn cannot reach the landing
/// cell before the bot is out of it.
///
/// Counted off the crossing rather than off the window, which is why #449's tune of
/// the window (3 → 4) left this number alone: the extra turn is slack the bot spends
/// standing on floor, not another turn a guard has to close on the landing.
pub const CROSSING_CLEARANCE: u32 = 3;

/// **A faint fit**: it might help. A step is probably worth more, and by default
/// the bot takes the step (the floor sits above this).
pub const URGE_FAINT: u8 = 25;

/// **A plain fit**: it would help, and there is nothing better to hand. The
/// default floor, so a plain fit is the weakest thing that presses a key.
pub const URGE_PLAIN: u8 = 50;

/// **A strong fit**: squarely the situation the ability's §8.3 row describes, and
/// the turn is better spent activating than stepping.
pub const URGE_STRONG: u8 = 75;

/// **The moment the ability exists for**: not pressing it now loses something the
/// run does not get back. Nothing outranks this, so at most one cue should ever
/// claim it for a given moment.
pub const URGE_DECISIVE: u8 = 100;

/// One ability's answer to "is this a moment for you?" — what to press, how badly,
/// and why.
///
/// It is a bid rather than a bare number because the arbitration has to be able to
/// *issue* it (the [`Input`]) and a §13.3 flag has to be traceable to a stated
/// reason. Bids are only ever built through [`Moment::press`] and [`Moment::hold`],
/// which is what keeps an illegal one unrepresentable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bid {
    /// The concrete input to issue if this bid wins — the activation itself, or
    /// the [`Input::Wait`] that is the *second half* of an ability whose value
    /// comes from holding still (§8.3, Camouflage).
    pub input: Input,
    /// How strongly this cue wants the moment, on the anchored scale
    /// ([`URGE_NONE`] … [`URGE_DECISIVE`]). Any value in between is fair; the
    /// anchors say what its neighbourhood *means*, which is what stops the scale
    /// becoming a handful of independently curve-fitted functions.
    pub urge: u8,
    /// Why, in the cue's own words — the string a §13.3 investigation reads back
    /// off a flagged seed. Justified from the ability's §8.3 row, never from what
    /// makes the bot win.
    pub reason: &'static str,
    /// Turns of follow-through the cue is committing to: some abilities are a
    /// *plan*, not a press (§8.3 — Camouflage is only worth the turn if you then
    /// hold still). Zero for a press that stands on its own, like Run.
    pub then_hold: u32,
}

/// The moment a cue is asked about: the world through the player's own channels,
/// plus the plan the bot has already settled on.
///
/// Everything here is a fact the surrounding policy has *already* computed, handed
/// over rather than re-derived — that sharing is the seam's reason to exist.
#[derive(Clone, Copy, Debug)]
pub struct Moment<'a> {
    /// The live game state, read exactly as a player reads it (§11.5a).
    pub state: &'a State,
    /// What the bot is trying to do this turn.
    pub intent: Intent,
    /// The step toward cover the policy has found for this moment, when it has one
    /// — `None` when no cupboard is within reach, and in pursuit, where cover is
    /// not the plan. A cue for an ability that *substitutes* for a hideout (§8.3,
    /// Camouflage — "a hideout you carry") reads this rather than looking for one
    /// itself.
    pub refuge: Option<Direction>,
    /// Manhattan distance to the nearest guard the player can perceive, seen or
    /// sensed (§9.2), or `None` when none is in reach.
    pub nearest_guard: Option<u32>,
    /// The **crossing** the policy has found worth phasing through, when it has one:
    /// the direction and how much of the router's own cost it saves (§8.3, Dephase).
    ///
    /// Computed by the policy, from the same field it routes on, precisely so the cue
    /// and the steps that follow it cannot disagree about what the shortcut is —
    /// pressing an ability for a crossing the policy would then decline to walk is
    /// the shy cue in its most literal form.
    pub crossing: Option<(Direction, u64)>,
    /// The step the plan would take **if no ability won this turn** — `None` when it
    /// would hold still, cornered or waiting a cone out.
    ///
    /// An ability whose effect lands in a *place* has to know where the bot is
    /// going, or it aims into its own route: a decoy dropped in the cell the bot is
    /// about to step into draws the search onto the escape instead of off it
    /// (§8.3). Handed over rather than re-derived — it is a Dijkstra descent the
    /// policy has already run.
    pub route: Option<Direction>,
}

impl Moment<'_> {
    /// The ability press this moment calls for, or `None` to spend the turn some
    /// other way (§4.4 — the bot's alternative is always a step or a wait).
    ///
    /// Deterministic end to end (§12.4), with **no RNG anywhere**: every held
    /// ability is cued in [`AbilityId::ALL`] order, a bid below its profile floor
    /// is dropped, and the highest urge wins with ties going to the earlier slot.
    pub fn best(&self, profile: &Profile) -> Option<Bid> {
        let mut best: Option<Bid> = None;
        // The roster is the run's *held* abilities in `AbilityId::ALL` order, each
        // already carrying its contextual state (#345) — so the sweep is over what
        // can actually be pressed, in a fixed order.
        for status in self.state.ability_statuses() {
            let Some(bid) = self.bid(status) else {
                continue;
            };
            // A no-fit is never a press, however low the floor is turned — the
            // floor picks how *keen* a cue must be, not whether zero counts.
            if bid.urge == URGE_NONE || bid.urge < profile.cue_floor(status.id) {
                continue;
            }
            // Strict `>` keeps the earlier `AbilityId::ALL` slot on a tie.
            if best.is_none_or(|held| bid.urge > held.urge) {
                best = Some(bid);
            }
        }
        best
    }

    /// **The seam**: one ability's cue. Exhaustive over [`AbilityId`], so a new row
    /// in the §8.1 catalog cannot ship without somebody saying what it is for — the
    /// compile error *is* the obligation.
    ///
    /// `status` carries the ability's contextual state (#345), which is where
    /// legality comes from; a cue never re-implements a precondition.
    pub fn bid(&self, status: AbilityStatus) -> Option<Bid> {
        match status.id {
            AbilityId::Run => self.run(status),
            AbilityId::Camouflage => self.camouflage(status),
            AbilityId::Decoy => self.decoy(status),
            AbilityId::Dephase => self.dephase(status),
            AbilityId::Autodoors => self.autodoors(status),
            AbilityId::Confusion => self.confusion(status),
            AbilityId::PierceWall => self.pierce_wall(status),
            AbilityId::Lockdown => self.lockdown(status),
            // **Passive** (§8.2/#264): always on while held, with no activation to
            // cue. Stated here rather than left to the match's silence, so "no cue"
            // reads as a decision and not an omission.
            //
            // The Saver (#243) is the same answer for a slightly different reason, and
            // it is worth naming: it *does* have something to spend, so the temptation
            // is to cue "play riskier while it is unspent". That would be the bot
            // deciding the ability is good and then proving itself right (§13.3) — the
            // measurement wanted here is what the **unchanged** policy's outcomes do
            // when one capture is survivable, and a bot that plays differently for
            // holding it cannot answer that. If the with/without pair shows the
            // temperaments barely moving, a boldness cue is the next experiment, and it
            // belongs to `Profile` where the other temperament knobs live.
            AbilityId::Vision | AbilityId::Saver => None,
        }
    }

    /// Run (§8.3): *"2 cells/turn while active"* — the innate escape, and what it
    /// is **for** is the moment a hunt starts and the gap has to open.
    fn run(&self, status: AbilityStatus) -> Option<Bid> {
        // Wrong when pursuing: the bot is not racing anybody, and the turn spent
        // activating buys nothing a step would not.
        if self.intent != Intent::Flee {
            return None;
        }
        // Activating costs a turn standing still (§4.4), which a guard already on
        // top of you turns into a capture — so only with a cell of room to spend it.
        if self.nearest_guard.is_some_and(|gap| gap <= 1) {
            return None;
        }
        // Decisive: this is the one turn that decides whether the chase is
        // outrunnable at all (§7.6), and it stands alone — no follow-through, the
        // extra cell rides on every step that follows for free.
        self.press(
            status,
            URGE_DECISIVE,
            "hunted with a cell of room to spare — spend the turn opening the gap (§8.3)",
            0,
        )
    }

    /// Camouflage (§8.3): *"undetectable while you don't move"* — a hideout you
    /// carry. What it is **for** is the hunt you cannot reach a cupboard from.
    fn camouflage(&self, status: AbilityStatus) -> Option<Bid> {
        // Only ever a way out of being found: pressing it while pushing for the
        // objective would spend a turn and its whole cooldown on nothing.
        if !matches!(self.intent, Intent::Flee | Intent::TakeCover) {
            return None;
        }
        // A real cupboard beats the carried one — it costs no cooldown and does not
        // pin the bot still — so this cue only speaks when there is none to reach.
        if self.refuge.is_some() {
            return None;
        }
        // Strong, not decisive: it is the fallback for having nowhere to hide, and
        // an escape that actually moves (Run) should outrank it on the same turn.
        // The press is only half of it — a *moving* cloaked player is seen like any
        // other — so the cue commits to holding still for the cloak's whole
        // duration, and keeps saying so for as long as it lasts.
        self.press(
            status,
            URGE_STRONG,
            "hunted with no cupboard to reach — cloak and let it pass over (§8.3)",
            status.id.def().economy().map_or(0, |e| e.duration()),
        )
        .or_else(|| {
            self.hold(
                status,
                URGE_STRONG,
                "cloaked and still — a stationary cloaked player is unseen (§8.3)",
            )
        })
    }

    /// Decoy (§8.3): *"a fake intruder in the cell you face. Draws Investigating,
    /// not Chasing."* What it is **for** is a guard that has **lost** you — a
    /// search needs somewhere to go, and the fake is somewhere to send it.
    fn decoy(&self, status: AbilityStatus) -> Option<Bid> {
        // Only while somebody is looking. Pressing it on the way to a console spends
        // the fake and its whole cooldown on a facility that is not searching for
        // anybody.
        if !matches!(self.intent, Intent::Flee | Intent::TakeCover) {
            return None;
        }
        // **The §8.3 rule, and the one #347 calls a cue bug rather than a tuning
        // question**: a decoy draws Investigating, never Chasing — so it works on a
        // guard that has lost you and does nothing about one that has you. A guard
        // whose cone is live on the player this turn is already coming to the real
        // intruder; the fake beside them competes with the genuine article and loses.
        if self
            .state
            .guards()
            .iter()
            .any(|guard| self.state.guard_detects_now(guard))
        {
            return None;
        }
        // Somebody has to be hunting for the fake to have a job. These are the three
        // states of a guard that is looking for *something it cannot see*: walking a
        // search out (§7.6), heading for a last-known cell, or answering a missed
        // radio ping (§7.3). A Calm patrol is not searching, so there is nothing to
        // redirect.
        let searching = self.state.guards().iter().any(|guard| {
            self.state.perceive_guard(guard).is_some()
                && matches!(
                    guard.state(),
                    GuardState::Alerted | GuardState::Investigating | GuardState::Responding
                )
        });
        if !searching {
            return None;
        }
        // The fake stands in the cell faced (§8.3), and the bot faces the way it last
        // stepped — so pressing while still heading that way plants the decoy on the
        // very route it is about to walk, drawing the search *onto* the escape. Only
        // worth the turn when the fake is somewhere the bot is not going.
        if self.route.is_some_and(|step| step == self.state.facing()) {
            return None;
        }
        // Strong while breaking contact — a search closing on your last-known cell is
        // exactly what the fake is bought for — and a plain fit when a patrol is only
        // closing, where a cupboard is usually the better turn. No follow-through: a
        // decoy is worth pressing precisely so the bot can leave while somebody else
        // walks toward it.
        let (urge, reason) = match self.intent {
            Intent::Flee => (
                URGE_STRONG,
                "a search is closing and nobody has eyes on me — give it somewhere else to look (§8.3)",
            ),
            _ => (
                URGE_PLAIN,
                "a patrol is hunting for something it cannot see — offer it the fake (§8.3)",
            ),
        };
        self.press(status, urge, reason, 0)
    }

    /// Autodoors (§8.3): a door in your path *"opens as you step into it — no bump,
    /// no lost turn — and shuts behind you once you clear it"*. A closed door breaks
    /// line of sight and makes a pursuer reopen it (§10.3/§10.4), which is why the
    /// row calls it *"a §7.6 flight tool, not invincibility"*.
    fn autodoors(&self, status: AbilityStatus) -> Option<Bid> {
        // Flight only. Everything it gives is about what happens *behind* you, so
        // pressing it on the way to a console buys a 40-turn cooldown and a door
        // nobody is chasing you through.
        if self.intent != Intent::Flee {
            return None;
        }
        // The turn it costs is spent standing still (§4.4), which a guard already at
        // arm's length turns into a capture — the same cell of room Run wants.
        if self.nearest_guard.is_some_and(|gap| gap <= 1) {
            return None;
        }
        // **A door has to be on the way out**, or there is nothing to shut and the
        // window burns down on open floor. The route is the step the plan would
        // otherwise take, so the door that matters is the one it leads into.
        let ahead = self.route.and_then(|dir| self.state.player().step(dir))?;
        if !self.door_at(ahead) {
            return None;
        }
        // Strong, not decisive: opening a gap outright (Run) is the better turn when
        // both are to hand, and this is the trick you play once the gap is open. No
        // follow-through — the door only shuts once the bot walks *through* it, so
        // the cue wants the next turns spent moving, not held.
        self.press(
            status,
            URGE_STRONG,
            "breaking contact through a door — shut it behind me and make them reopen it (§8.3)",
            0,
        )
    }

    /// Pierce Wall (§8.3): *"bore straight through your one adjacent wall,
    /// permanently"*, on a per-level budget of three. What it is **for** is a route
    /// the facility does not offer — never the fact that a wall happens to be there.
    ///
    /// #347 names the failure mode precisely: *"a cue that spends the budget on the
    /// first legal wall makes the histogram look healthy while measuring nothing"*.
    /// So the bid is about what boring **saves**, and the target has to be the wall
    /// the route actually wants.
    fn pierce_wall(&self, status: AbilityStatus) -> Option<Bid> {
        // A hole is not a hiding place — it conceals nothing (§8.3, it is not a
        // cupboard) — so it is never an answer to being hunted.
        if matches!(self.intent, Intent::Flee | Intent::TakeCover) {
            return None;
        }
        let (dir, saving) = self.crossing?;
        // **The wall the route wants, not merely a legal one.** The bore target is
        // unique by precondition (§8.4/#303 — exactly one neighbour is a wall, so
        // there is nothing to aim), and core owns it; the cue's job is to check that
        // the unique answer and the crossing the router would use are the same wall.
        if self.state.player().step(dir) != self.state.bore_target().ok() {
            return None;
        }
        // **The budget is the scarce thing** (§8.2: three a level, no cooldown, and
        // the hole is permanent), so the borer asks for a bigger saving than the
        // phase does for the same crossing — Dephase spends a cooldown that comes
        // back, this spends a third of the level's supply that does not.
        if saving < BORE_MARGIN {
            return None;
        }
        self.press(
            status,
            URGE_STRONG,
            "a wall standing between me and where I am going, and a route worth a third of the borer (§8.3)",
            0,
        )
    }

    /// Dephase (§8.3): *"walk through walls, doors, guards"* for four turns (#449) — and
    /// **it does not conceal you**. So what it is *for* is a short crossing you can
    /// see the far side of, never a way out of being seen.
    fn dephase(&self, status: AbilityStatus) -> Option<Bid> {
        // **Never an escape.** A phased player is as visible as any other, so pressing
        // it while hunted spends a turn and changes nothing about being seen — and
        // walking into a wall to hide is how the safety eject is found (§8.3).
        if matches!(self.intent, Intent::Flee | Intent::TakeCover) {
            return None;
        }
        // The crossing is the policy's, worked out on the field it routes on: one cell
        // of solid, a far side the bot has actually seen, and a saving big enough to
        // pay for the three turns the crossing costs. No crossing, no bid — "there is
        // a wall here" is not a reason.
        let (_, saving) = self.crossing?;
        // **Not with company.** The crossing takes three turns and the bot is committed
        // for all of them; a guard near the far side can be standing on the exit cell
        // when the duration runs out, and a phase that expires inside a solid costs the
        // safety eject and a stun (§8.3). The shortcut is never worth that, so the cue
        // only speaks when nobody perceived is close enough to wander into the landing.
        if self
            .nearest_guard
            .is_some_and(|gap| gap <= CROSSING_CLEARANCE)
        {
            return None;
        }
        // Plain by default and strong for a crossing that saves a lot: the ability is
        // a shortcut, and a shortcut is worth exactly what it saves. Nothing here is
        // ever decisive — no crossing is worth losing the run over, which is the
        // difference between a shortcut and an escape.
        let (urge, reason) = if saving >= 2 * crate::bot::CROSSING_MARGIN {
            (
                URGE_STRONG,
                "a wall between me and where I am going, with the far side in view (§8.3)",
            )
        } else {
            (
                URGE_PLAIN,
                "a short crossing that beats walking round it (§8.3)",
            )
        };
        self.press(status, urge, reason, 0)
    }

    /// Confusion (§8.3): *"a costed panic-buy of time, not a kill"* — fired once from
    /// the cell you stand in, freezing every guard the clamped blast catches for six
    /// turns. What it is **for** is the moment the time is worth more than the
    /// 45-turn cooldown.
    fn confusion(&self, status: AbilityStatus) -> Option<Bid> {
        // A panic-buy is bought in a panic. Freezing a patrol you have not been seen
        // by spends the longest cooldown in the catalog on a guard that was going to
        // walk past anyway.
        if self.intent != Intent::Flee {
            return None;
        }
        // How many the blast would catch. The reach is core's, already clamped to the
        // live guard sense (§8.3 [SETTLED]: `min(CONFUSION_RADIUS, sense_range())`),
        // so counting guards inside it stays within what the player was shown —
        // anything it could catch, they were already sensing. Legality is core's too:
        // a firing that catches nobody is `Unusable`, so `press` refuses it and this
        // count is never zero by the time it matters.
        let blast = self.state.confusion_blast();
        let caught = self
            .state
            .guards()
            .iter()
            .filter(|guard| blast.contains(guard.pos()))
            .count();

        // **Cornered is the moment this ability exists for.** A guard at arm's length
        // is a capture next turn (§4.5) and the two escapes both decline here — Run
        // and Autodoors need a cell of room to spend the activation turn in. A dazed
        // adjacent guard cannot step into you (§8.3), so this is the one turn that
        // buys the run back, and it does not get another.
        if self.nearest_guard.is_some_and(|gap| gap <= 1) {
            return self.press(
                status,
                URGE_DECISIVE,
                "a guard at arm's length and nowhere to spend a turn — freeze it or be taken (§8.3)",
                0,
            );
        }
        // Otherwise the daze is worth its cooldown when it buys time against more than
        // one hunter at once; against a single chaser with room to run, outrunning it
        // is the cheaper turn and Run will say so.
        let (urge, reason) = if caught > 1 {
            (
                URGE_STRONG,
                "more than one hunter inside the blast — buy six turns from all of them at once (§8.3)",
            )
        } else {
            (
                URGE_PLAIN,
                "hunted, with the blast on a guard — buy the six turns (§8.3)",
            )
        };
        self.press(status, urge, reason, 0)
    }

    /// Lockdown (§8.3/#242): every door within `LOCKDOWN_RADIUS` of **where you fired
    /// it** is shut and sealed, so *"a guard cannot work the handle and its route goes
    /// the long way round"*. What it is **for** is sending a pursuit round the houses
    /// while you take the short way.
    fn lockdown(&self, status: AbilityStatus) -> Option<Bid> {
        // Flight. Route denial only means something against somebody following a
        // route to you — a sealed door costs a patrol nothing it was not already
        // walking past.
        if self.intent != Intent::Flee {
            return None;
        }
        // The turn is spent standing still (§4.4), so the same cell of room Run and
        // Autodoors insist on.
        if self.nearest_guard.is_some_and(|gap| gap <= 1) {
            return None;
        }
        // **Never across a route you still have to travel.** §8.3 names this as the
        // real mistake: your own lock is never refused, but bumping it open costs the
        // turn and leaves the door standing open — "unmaking it is paid in the very
        // turns the ability was bought to save". The step the plan would take is the
        // route, so a door in it means the seal would land on the bot's own way out.
        // The sharpest form of it first: **standing in the doorway itself**. Sealing
        // from inside a door shuts the one cell the bot is halfway through, which is
        // a route it has to travel by definition.
        if self.door_at(self.state.player()) {
            return None;
        }
        let ahead = self.route.and_then(|dir| self.state.player().step(dir));
        if ahead.is_some_and(|cell| self.door_at(cell)) {
            return None;
        }
        // How much is actually being denied. Legality is core's — a firing with no
        // door in reach is `Unusable`, refused for free (§4.4) — so this count is
        // never zero here; what it grades is whether the box is worth a 40-turn
        // cooldown, and one door is a detour where three is a wall.
        let doors = self.state.lockdown_doors().len();
        let (urge, reason) = if doors > 1 {
            (
                URGE_STRONG,
                "breaking contact with a knot of doors around me — seal them and send the hunt round (§8.3)",
            )
        } else {
            (
                URGE_PLAIN,
                "one door in reach while hunted — a sealed handle is a detour they have to walk (§8.3)",
            )
        };
        self.press(status, urge, reason, 0)
    }

    /// Whether `cell` holds a door panel, open or closed, **as the player knows it**
    /// (§11.5a): a door the bot has never seen is not a fact it may plan around.
    fn door_at(&self, cell: Cell) -> bool {
        self.state.memory().contains(cell)
            && matches!(
                self.state.layout().facility().terrain(cell),
                Some(Terrain::DoorPanelClosed | Terrain::DoorPanelOpen)
            )
    }

    /// A bid to **activate** `status`'s ability, or `None` when its live state says
    /// the press would not fire.
    ///
    /// This is where "legality is never re-implemented in a cue" is enforced rather
    /// than asked for: `Ready` and `Limited` are the two states that promise the
    /// press does something (§8.2/#302), and every other state — cooling,
    /// exhausted, already active, passive, or contextually `Unusable` (#345) — has
    /// no activation to offer.
    fn press(
        &self,
        status: AbilityStatus,
        urge: u8,
        reason: &'static str,
        then_hold: u32,
    ) -> Option<Bid> {
        matches!(
            status.state,
            AbilityState::Ready | AbilityState::Limited { .. }
        )
        .then_some(Bid {
            input: Input::Activate(status.id),
            urge,
            reason,
            then_hold,
        })
    }

    /// A bid to **hold still** for an ability that is already running and only pays
    /// out while you do not move (§8.3). `None` unless it is genuinely active, so
    /// this can never become a way to wait for nothing.
    fn hold(&self, status: AbilityStatus, urge: u8, reason: &'static str) -> Option<Bid> {
        let AbilityState::Active { remaining } = status.state else {
            return None;
        };
        Some(Bid {
            input: Input::Wait,
            urge,
            reason,
            // Exactly what is left of it: the commitment shortens as the ability does.
            then_hold: remaining,
        })
    }
}

/// An ability's **cue slot**: its position in [`AbilityId::ALL`], which is the index
/// its per-ability floor sits at in [`Profile::cue_floors`].
///
/// Derived from the catalog order rather than written out, so a new ability lands
/// in a slot without a second list to keep in step — and, per the never-renumber
/// rule, the positions are permanent.
pub fn slot(id: AbilityId) -> usize {
    AbilityId::ALL
        .iter()
        .position(|&a| a == id)
        .expect("every ability is in AbilityId::ALL")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The urge anchors are **ordered and distinct**, or the scale they document
    /// would not be a scale: a cue author picking "strong" over "plain" has to get
    /// a keener bid for it, and #349's sweep has to have somewhere to move.
    #[test]
    fn the_urge_anchors_climb() {
        let anchors = [
            URGE_NONE,
            URGE_FAINT,
            URGE_PLAIN,
            URGE_STRONG,
            URGE_DECISIVE,
        ];
        for pair in anchors.windows(2) {
            assert!(pair[0] < pair[1], "the anchors must climb: {pair:?}");
        }
        assert_eq!(URGE_DECISIVE, 100, "the scale runs 0..=100");
    }

    /// Every ability has a distinct cue slot, and the slots cover the catalog —
    /// the floors array is indexed by this, so a collision would silently give two
    /// abilities one threshold.
    #[test]
    fn every_ability_has_its_own_cue_slot() {
        let slots: Vec<usize> = AbilityId::ALL.iter().map(|&id| slot(id)).collect();
        let mut sorted = slots.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            AbilityId::ALL.len(),
            "slots collide: {slots:?}"
        );
        assert_eq!(sorted, (0..AbilityId::ALL.len()).collect::<Vec<_>>());
    }

    /// **No cue may offer a press that would not fire** (#345): swept over every
    /// ability × every state, an [`Input::Activate`] bid may only come out of the
    /// two states that promise the press does something. This is the acceptance
    /// criterion "legality is never re-implemented in a cue", asserted at the seam
    /// rather than trusted per cue — and it holds for cues not yet written, so
    /// #347 lands under it.
    #[test]
    fn a_cue_never_bids_an_activation_that_could_not_fire() {
        let (state, _) = crate::test_support::boot(0);
        let states = [
            AbilityState::Ready,
            AbilityState::Limited { uses: 2 },
            AbilityState::Active { remaining: 3 },
            AbilityState::Cooling { remaining: 4 },
            AbilityState::Exhausted,
            AbilityState::Passive,
            AbilityState::Unusable,
        ];
        for intent in [
            Intent::Flee,
            Intent::TakeCover,
            Intent::Pursue,
            Intent::Explore,
        ] {
            for refuge in [None, Some(Direction::North)] {
                for nearest_guard in [None, Some(0), Some(1), Some(5)] {
                    for route in [None, Some(Direction::North), Some(Direction::South)] {
                        for crossing in [
                            None,
                            Some((Direction::East, 4)),
                            Some((Direction::East, 40)),
                        ] {
                            let moment = Moment {
                                state: &state,
                                intent,
                                refuge,
                                nearest_guard,
                                route,
                                crossing,
                            };
                            for id in AbilityId::ALL {
                                for &ability_state in &states {
                                    let status = AbilityStatus {
                                        id,
                                        state: ability_state,
                                    };
                                    let Some(bid) = moment.bid(status) else {
                                        continue;
                                    };
                                    assert!(
                                        !bid.reason.is_empty(),
                                        "{}: a bid must say why (§13.3)",
                                        id.name(),
                                    );
                                    if bid.input == Input::Activate(id) {
                                        assert!(
                                            matches!(
                                                ability_state,
                                                AbilityState::Ready | AbilityState::Limited { .. }
                                            ),
                                            "{} bid an activation while {ability_state:?}",
                                            id.name(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// The two cues that exist today say what their §8.3 rows say, at the seam:
    /// Run is for **fleeing with room to spend the turn**, Camouflage for a hunt
    /// with **no cupboard to reach** — and neither speaks while pursuing.
    #[test]
    fn the_two_written_cues_answer_their_own_rows() {
        let (state, _) = crate::test_support::boot(0);
        let moment = |intent, refuge, nearest_guard| Moment {
            state: &state,
            intent,
            refuge,
            nearest_guard,
            route: None,
            crossing: None,
        };
        let ready = |id| AbilityStatus {
            id,
            state: AbilityState::Ready,
        };
        let run = ready(AbilityId::Run);
        let camo = ready(AbilityId::Camouflage);

        // Run: fleeing with a cell of room.
        assert!(moment(Intent::Flee, None, Some(2)).bid(run).is_some());
        assert!(moment(Intent::Flee, None, None).bid(run).is_some());
        // …but not with a guard already on top of you — that turn is a capture.
        assert!(moment(Intent::Flee, None, Some(1)).bid(run).is_none());
        // …and never while pushing for the objective.
        for intent in [Intent::TakeCover, Intent::Pursue, Intent::Explore] {
            assert!(moment(intent, None, Some(5)).bid(run).is_none());
        }

        // Camouflage: only when there is no cupboard to reach.
        for intent in [Intent::Flee, Intent::TakeCover] {
            assert!(moment(intent, None, Some(3)).bid(camo).is_some());
            assert!(
                moment(intent, Some(Direction::North), Some(3))
                    .bid(camo)
                    .is_none(),
                "a real cupboard is within reach — the carried one is not needed",
            );
        }
        for intent in [Intent::Pursue, Intent::Explore] {
            assert!(moment(intent, None, Some(3)).bid(camo).is_none());
        }

        // The cloak's second half: while it is running, the cue holds still, and
        // says how long it is committing for.
        let cloaked = AbilityStatus {
            id: AbilityId::Camouflage,
            state: AbilityState::Active { remaining: 7 },
        };
        let bid = moment(Intent::Flee, None, Some(3))
            .bid(cloaked)
            .expect("an active cloak holds");
        assert_eq!(bid.input, Input::Wait);
        assert_eq!(bid.then_hold, 7, "the commitment is what is left of it");

        // The press commits to the cloak's whole duration up front.
        let bid = moment(Intent::Flee, None, Some(3))
            .bid(camo)
            .expect("a ready cloak presses");
        assert_eq!(bid.input, Input::Activate(AbilityId::Camouflage));
        assert_eq!(
            bid.then_hold,
            AbilityId::Camouflage
                .def()
                .economy()
                .expect("Camouflage is activated")
                .duration(),
        );
    }

    /// Arbitration is deterministic and floor-gated (§12.4): the keener bid wins,
    /// a bid below its ability's floor is dropped, and a floor above every anchor
    /// silences a cue entirely — which is the handle #349 turns to produce its
    /// curve.
    #[test]
    fn the_floor_decides_which_cues_can_speak() {
        let (state, _) = crate::test_support::boot(0);
        let state =
            state.with_loadout(intrusion_core::Loadout::innate().with(AbilityId::Camouflage));
        // Fleeing, no cupboard to reach, room to spare: both cues speak, and Run's
        // decisive urge outranks the cloak's strong one.
        let moment = Moment {
            state: &state,
            intent: Intent::Flee,
            refuge: None,
            nearest_guard: Some(4),
            route: None,
            crossing: None,
        };
        let balanced = Profile::BALANCED;
        assert_eq!(
            moment.best(&balanced).map(|b| b.input),
            Some(Input::Activate(AbilityId::Run)),
        );

        // Silence Run alone and the cloak takes the moment — per-ability floors,
        // not one shared threshold.
        let quiet_run = balanced.with_cue_floor(AbilityId::Run, URGE_DECISIVE + 1);
        assert_eq!(
            moment.best(&quiet_run).map(|b| b.input),
            Some(Input::Activate(AbilityId::Camouflage)),
        );

        // Silence both and the bot spends its turn some other way.
        let quiet = quiet_run.with_cue_floor(AbilityId::Camouflage, URGE_DECISIVE + 1);
        assert_eq!(moment.best(&quiet), None);

        // An ability the run does not hold is never bid, whatever its floor: the
        // roster the arbitration sweeps is the *held* one.
        let bare = crate::test_support::boot(0).0;
        let moment = Moment {
            state: &bare,
            intent: Intent::Flee,
            refuge: None,
            nearest_guard: Some(4),
            route: None,
            crossing: None,
        };
        let keen = balanced.with_cue_floor(AbilityId::Camouflage, URGE_NONE);
        assert_eq!(
            moment.best(&keen).map(|b| b.input),
            Some(Input::Activate(AbilityId::Run)),
            "the bare loadout holds no cloak to press",
        );
    }
}
