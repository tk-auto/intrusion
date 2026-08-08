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
//! **exhaustively** on [`AbilityId`], so adding a row to the §8.1 catalogue fails to
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
//! be traceable back to *why*), and an [**urge**](Bid::urge) on the anchored scale
//! below. An ability that is a *plan* rather than a press (Camouflage's hold-still,
//! §8.3) is followed through by **re-bidding each turn** while it runs — there is no
//! stored commitment for the policy to honour or forget.
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

use crate::profile::{Profile, BORE_MARGIN, CROSSING_MARGIN};

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

/// How much room a False Call wants from **every guard it would call** (**[START] = 8**):
/// the turn the press costs, plus the turns it takes to be somewhere else once the
/// responders start walking.
///
/// Measured against the *called* set rather than against
/// [`Moment::nearest_guard`](Moment::nearest_guard), and the difference is load-bearing
/// twice over. The nearest guard in view may not be one the call reaches at all; and the
/// call may reach one the bot has no picture of, which a check written against what the
/// bot perceives would miss entirely.
///
/// Much larger than the crossing's clearance ([`CROSSING_CLEARANCE`]), because the two
/// buy opposite things. A phase asks only that nobody wanders onto the landing cell in
/// the three turns it takes; this **summons** guards, deliberately more than one, from
/// every side of a box — so what it needs is not a cell of elbow room but a head start
/// long enough to be out of the neighbourhood before the first of them arrives. At 4 the
/// §13.2 batch took the balanced win rate from 0.35 to 0.05: the bot called searches onto
/// its own feet, which is exactly the failure §8.3's row warns a *player* about.
pub const FALSE_CALL_CLEARANCE: u32 = 8;

/// How far down the route a False Call looks for ground worth emptying (**[START]**).
///
/// Not one cell, and the reason is structural rather than a tuning taste: the router
/// already prices watched cells out ([`Profile::watched_penalty`]), so the step it hands
/// over is almost never itself watched, and a cue keyed on that single cell is shy by
/// construction. What the ability is for is the ground the bot is *heading for*.
pub const FALSE_CALL_SCOUT: usize = 6;

/// How close a hunter has to be for a Repel to read as **pressing** rather than merely
/// available (**[START] = 6**): near enough that the detour the field buys is one the
/// guard was actually about to not take.
///
/// It sits above [`REPEL_RADIUS`](intrusion_core::REPEL_RADIUS) with daylight to spare, and
/// the gap between the two is the whole cue: inside the radius the ability is *void* (the
/// guard would be walled in with you), and just outside it the ability is at its best. Six
/// is roughly a guard's certain zone — the range at which something is genuinely bearing
/// down — and it is a keenness knob, not a rule: getting it wrong makes the cue shy or
/// eager, never wrong.
const REPEL_PRESSING: u32 = 6;

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
    /// Whether this temperament **wants the takedown verb at all** — the
    /// [`takedown_reach`](Profile::takedown_reach) knob as the yes/no the cues need
    /// (§7.2/#316).
    ///
    /// It is here rather than read off the profile inside a cue because it is a fact
    /// about the *plan*, exactly like [`intent`](Moment::intent): `balanced` and
    /// `cautious` ship a reach of zero, which the knob's own doc defines as **"not
    /// 'never gets the chance', but 'does not want it'"** — an avoidance-first bot
    /// leaves an unaware guard blocked and waits the patrol out.
    ///
    /// **A cue for a takedown must honour it, whatever the range** (§13.3). The Dart
    /// (§8.3/#239) is §7.2's verb with reach, so handing it to a profile that declines
    /// the verb would not measure the ability — it would change the temperament and then
    /// report the change as the ability's doing. The tell is unmissable once you look
    /// for it: a profile with `takedowns: 0` in its control column suddenly leaving
    /// bodies, which is a bot that has stopped being the bot whose baseline it is being
    /// read against.
    pub strikes: bool,
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
    /// in the §8.1 catalogue cannot ship without somebody saying what it is for — the
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
            AbilityId::FalseCall => self.false_call(status),
            AbilityId::Dart => self.dart(status),
            AbilityId::Repel => self.repel(status),
            // **Cover** (§8.3/§10.3/#562) is a fourth kind of "no cue", and it is not that
            // the ability is weak: **this policy cannot aim it.**
            //
            // The ability is aimed by facing (§8.4) and the bot faces the way it last
            // stepped — there is no turn-in-place (§5). Its router prices watched cells out
            // and holds rather than stepping into a cone, so by the time a patrol is close
            // enough to take cover from, every recent step has been *away* from that cone
            // and the faced cell is on the wrong side of the bot. A cue was written to the
            // §8.3 row and gated on core's own geometry — *would ducking behind the piece
            // this press puts down hide me from every guard I perceive?*
            // ([`State::crouch_would_conceal`]) — and it fired **zero** times over 120 seeds
            // on each of the three temperaments. Ungated it fired a dozen times in forty and
            // never once ducked behind what it had built, which is a press bought with a
            // turn and a 35-turn lockout for nothing (§13.3, and `Verb::Cover`'s own doc
            // names that exact tell).
            //
            // The other half of the ability is worse served still. What it *sells* is a
            // crossing walked behind the piece, one push a turn — and the bot's route is a
            // Dijkstra over cells the player can walk **through**, which a table is not, so
            // the router plans around the bot's own cover rather than through it. Making it
            // plan through would mean teaching the field that a pushable solid is passable
            // when and only when the cell beyond it is free, and then following that route
            // as a mode: a second policy, exactly as the Drone's flight plan is, and its own
            // ticket for the same reason.
            //
            // So the honest report is a zero in the histogram (`Verb::Cover`) with this
            // comment and `docs/stats/abilities/cover.md` saying which kind of zero it is
            // (§13.3). What the bot *does* still do is plan a duck against the right
            // geometry: on the run's own cover a bump is a shove, and `Bot::crouch` asks
            // [`State::cover_push`] rather than assuming the furniture stays put — so a
            // scripted run, or a policy that grows the mode later, meets a bot that is not
            // blind to the mechanic.
            AbilityId::Cover => None,
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
            // The **Drone** (§8.1/#273) is a third kind of "no cue", and the reason is
            // not that it is weak: piloting is a **control mode**, and this bot does not
            // have one. Pressing it would transfer the keys to a machine while the
            // policy went on issuing steps for a body that is no longer listening — the
            // bot would fly its drone into a wall for thirty turns and call the result a
            // measurement.
            //
            // What it would take is a real second policy: a flight plan over the fog
            // (§11.5a — where has nobody looked?), a judgement about how long the body
            // can be left standing where it stands, and a reason to come back. That is
            // its own ticket, and until it exists the honest report is a zero in the
            // usage histogram (`Verb::Drone`) with this comment and
            // `docs/stats/abilities/drone.md` saying which kind of zero it is (§13.3).
            AbilityId::Drone => None,
            // The **Guide** (§8.3/#505) is the Vision answer again, and for exactly the
            // Vision reason: a passive has no activation to cue. It is worth naming the
            // temptation it does raise, since unlike Vision it hands over something the
            // policy could act on — a bearing. Acting on it would be the bot routing to
            // an objective it has never *seen*, which is the one thing §11.5a forbids the
            // policy to do (`known_intel` is the no-cheat gate), so a cue here could not
            // be written without punching a hole in that gate. The honest measurement is
            // the with/without pair on `docs/stats/abilities/guide.md`, and what it
            // watches is whether the *human* holding it stops exploring.
            AbilityId::Vision | AbilityId::Saver | AbilityId::Guide => None,
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
        // other — so the follow-through is the `hold` arm below, re-bid on every
        // turn the cloak still runs: the plan is restated while it holds, never
        // stored as a commitment nothing would read.
        self.press(
            status,
            URGE_STRONG,
            "hunted with no cupboard to reach — cloak and let it pass over (§8.3)",
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
        self.press(status, urge, reason)
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
        let (urge, reason) = if saving >= 2 * CROSSING_MARGIN {
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
        self.press(status, urge, reason)
    }

    /// Confusion (§8.3): *"a costed panic-buy of time, not a kill"* — fired once from
    /// the cell you stand in, freezing every guard the clamped blast catches for six
    /// turns. What it is **for** is the moment the time is worth more than the
    /// 45-turn cooldown.
    fn confusion(&self, status: AbilityStatus) -> Option<Bid> {
        // A panic-buy is bought in a panic. Freezing a patrol you have not been seen
        // by spends the longest cooldown in the catalogue on a guard that was going to
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
        self.press(status, urge, reason)
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
        self.press(status, urge, reason)
    }

    /// False Call (§7.7/§8.3/#504): a forged control message naming the cell you fired
    /// from, which every guard in reach walks to and searches. What it is **for** is
    /// *"a vacuum, not a trap"* — the ground you empty, not the guards you gather.
    ///
    /// So the cue is written as that sentence and nothing wider. It fires only where the
    /// bot is **going somewhere** and the call would pull the guards **behind** it, which
    /// is the one arrangement in which the ability does what its §8.3 row says rather
    /// than what its most obvious misreading says. That narrowness is deliberate: the
    /// ability's whole value is in the turns *after* the press, and a cue that pressed it
    /// without checking where the bot was about to walk would be measuring the bot
    /// getting itself caught (§13.3).
    fn false_call(&self, status: AbilityStatus) -> Option<Bid> {
        // Never in flight. Calling the net to your own cell while something is hunting
        // you is the suicide press the §8.3 row warns about — the escapes are Run,
        // Autodoors and Lockdown, and every one of them is about the guards having
        // *less* reason to be where you are.
        if !matches!(self.intent, Intent::Pursue | Intent::Explore) {
            return None;
        }
        // A vacuum needs somewhere to go. With no route the bot is standing still, and a
        // call fired from a cell it is not leaving is a search fired at itself.
        let step = self.route?;
        let ahead = self.state.player().step(step)?;
        // Who it would actually pull. Read off the reach rather than off what the bot
        // perceives, because the two are **not the same set** here: the broadcast is a
        // radio and is not clamped to the guard sense (§8.3/#504), so it can summon
        // guards the bot has no picture of — and those are exactly the ones a check
        // written against `nearest_guard` would miss. Legality is core's: a call
        // reaching nobody, or one fired into a dead net, is `Unusable`, so `press`
        // refuses it and this is never empty.
        let reach = self.state.false_call_area();
        let called: Vec<Cell> = self
            .state
            .guards()
            .iter()
            .map(|guard| guard.pos())
            .filter(|&at| reach.contains(at))
            .collect();
        let player = self.state.player();
        // **Room to be gone before anybody arrives**, measured against the guards being
        // *called* and not merely the nearest one in view. The turn is spent standing
        // still (§4.4) and the responders start walking on it, so a guard summoned from
        // close by arrives while the bot is still in the neighbourhood — which is the
        // press paying for its own capture.
        if called
            .iter()
            .any(|&at| player.manhattan_distance(at) < FALSE_CALL_CLEARANCE)
        {
            return None;
        }
        // **Rule one: only call them to a cell you are already walking away from.** If
        // the next step of the route does not open the gap on every guard the call would
        // pull, the responders are converging *across* the bot's own way out — which is
        // the trap the ability is not, spelled as the one predicate that separates the
        // two. Note the direction this takes the guards: toward the cell being left, and
        // therefore away from the route.
        if !called
            .iter()
            .all(|&at| ahead.manhattan_distance(at) > player.manhattan_distance(at))
        {
            return None;
        }
        // **Rule two: there has to be ground worth emptying.** A call that pulls a
        // patrol which was not looking at anything the bot wanted buys nothing at all —
        // it just puts guards where the bot is. So the step the plan would take must be
        // **watched** (§11.5's detection set, read exactly as the router prices it), and
        // what the call then does is take those cones off it while the bot walks
        // through.
        //
        // The two rules together are the ability's §8.3 sentence as a predicate: the
        // guards watch the ground *ahead* but do not stand on the way to it, so
        // answering a call behind them walks them off the route rather than down it.
        //
        // It looks a **little** way down the route rather than only at the very next
        // cell, and the reason is a Catch-22 the first version walked straight into: the
        // router already prices watched cells out ([`Profile::watched_penalty`]), so the
        // step it hands over is almost never watched, and a cue keyed on that one cell
        // fired about once in a hundred runs. The ground the bot is *heading for* is the
        // honest question; the ground it has already decided to stand on next turn is
        // not.
        let scout: Vec<Cell> = std::iter::successors(Some(player), |cell| cell.step(step))
            .take(FALSE_CALL_SCOUT + 1)
            .skip(1)
            .collect();
        if !self
            .state
            .visible_cone_cells()
            .any(|cell| scout.contains(&cell))
        {
            return None;
        }
        // Never decisive: nothing about a shortcut through a patrol is worth the run, and
        // emptying ground is worth exactly what it empties — one guard pulled off a
        // corridor is a convenience, three is a wing.
        let (urge, reason) = if called.len() > 1 {
            (
                URGE_STRONG,
                "walking away from a knot of guards — forge a call and empty the ground behind me (§8.3)",
            )
        } else {
            (
                URGE_PLAIN,
                "one guard behind me and a route ahead — call it to the cell I am leaving (§8.3)",
            )
        };
        self.press(status, urge, reason)
    }

    /// Dart (§7.2/§8.3/#239): *"the first guard on the line goes down if it has not seen
    /// you"* — one shot a facility, fired along the way you are already facing. What it is
    /// **for** is the watcher standing between the bot and where it is going that it cannot
    /// walk up to.
    ///
    /// #347's failure mode is sharper here than anywhere else, because the target is free:
    /// a cue that fired at the first legal guard on the line would spend the level's only
    /// dart on the first patrol that happened to be in front of the bot, and the histogram
    /// would read *used* while measuring nothing about whether a ranged takedown is worth
    /// having. So the bid asks the one question that separates this ability from the §7.2
    /// verb it copies: **is this a guard I could not have reached on foot?**
    fn dart(&self, status: AbilityStatus) -> Option<Bid> {
        // **A temperament that declines the takedown declines the dart** (§7.2/§13.3/#316).
        // This is the first gate rather than a detail, because getting it wrong does not
        // make the cue shy or keen — it makes the measurement meaningless. `balanced` and
        // `cautious` carry `takedown_reach: 0`, which the knob's own doc defines as *"does
        // not want it"*, and both report `takedowns: 0` in their control columns. A dart is
        // §7.2's verb with the range changed, so a cue that fired it for them would hand an
        // avoidance-first bot a kill it would never walk up and take, and every number in
        // the with/without pair would then be measuring a **different bot** against the
        // baseline of the old one (§13.3's whole failure mode).
        //
        // Read off the plan rather than out of `Profile` here for [`Intent`]'s reason: this
        // is a fact the policy already holds, and a cue that reached for the profile itself
        // would be the second place a temperament is interpreted.
        if !self.strikes {
            return None;
        }
        // **Never in flight.** A guard that is hunting the bot has detected it, so it is not
        // a legal target at all (§7.2) — the press would fire, miss, and spend the level's
        // dart on the turn the bot could least afford it. This is the one cue where declining
        // while fleeing is a *legality* statement rather than a judgement about value.
        if matches!(self.intent, Intent::Flee | Intent::TakeCover) {
            return None;
        }
        // **The cue owns the aim check, and that is not a re-derived precondition.** Every
        // other cue leans on `status.state` for legality (#345), because every other ability
        // is `Unusable` when its target is missing. A dart is deliberately never refused —
        // that would make the bar a detector (§8.4/#239) — so `Ready`/`Limited` says nothing
        // about whether there is anything on the line, and the shot has to be asked for
        // directly. Core still owns the answer: this reads `dart_shot`, the same function
        // the firing uses, and re-implements none of it.
        let shot = self.state.dart_shot();
        let target = shot.hit()?;
        // **A guard the bot could simply walk into is not what this is for.** The adjacent
        // takedown costs the same turn and no use at all (§7.2), so spending the facility's
        // one dart on a guard already at arm's length is strictly worse than stepping.
        if self.state.player().manhattan_distance(target) <= 1 {
            return None;
        }
        // **It has to be in the way.** The shot is worth the level's supply when the target
        // is *watching the ground the bot is heading for* — that is the guard a route cannot
        // simply be planned around, and taking it off the board is the thing a dart does
        // that nothing else in the kit does at range. The scouted line is False Call's, for
        // False Call's reason: the router already prices watched cells out
        // ([`Profile::watched_penalty`]), so keying on the single next step would make this
        // shy by construction, and the honest question is about where the bot is *going*.
        //
        // Read off the guard's own cone rather than off `visible_cone_cells`, because the
        // question is about **this** guard and not about whether any cone covers the route:
        // darting a guard whose cone is elsewhere would leave the watcher standing.
        let step = self.route?;
        let player = self.state.player();
        let scout: Vec<Cell> = std::iter::successors(Some(player), |cell| cell.step(step))
            .take(FALSE_CALL_SCOUT + 1)
            .skip(1)
            .collect();
        let watcher = self
            .state
            .guards()
            .iter()
            .find(|guard| guard.pos() == target)?;
        if !scout.iter().any(|&cell| watcher.fov().contains(cell)) {
            return None;
        }
        // **Strong, never decisive.** Nothing about clearing a route is worth the run, and
        // the run's one dart should lose to any escape that wants the same turn — Run and
        // Confusion both claim `URGE_DECISIVE` in their own moments, and both of those
        // moments are ones this cue has already declined (it never speaks in flight). No
        // follow-through: the dart resolves on the turn it is pressed and the next turns are
        // for walking down the line it cleared.
        self.press(
            status,
            URGE_STRONG,
            "a guard watching the ground ahead, on my line and unaware — spend the dart (§8.3)",
        )
    }

    /// Repel (§7.6/§8.3/#554): a disc stamped where you fire it that no guard will walk
    /// into for its window. What it is **for** is the chase across ground a lockdown cannot
    /// touch — open floor, a hub room, a corridor with nothing to shut — where the only way
    /// to buy a detour is to put the wall down yourself.
    ///
    /// It is written as [`lockdown`](Self::lockdown)'s sibling and reads almost the same,
    /// which is deliberate: the two abilities buy the same thing, so a cue that asked
    /// something different of each would make the histogram's comparison of them
    /// meaningless (§13.3).
    ///
    /// **It used to carry a third gate and no longer does**, which is worth recording
    /// rather than quietly deleting. While a guard inside the disc was unconstrained,
    /// pressing with a hunter at arm's length built a wall with the hunter on the inside —
    /// the worst press in the ability — and the cue declined it. Guards now walk out
    /// (§8.3/#554), so that press is no longer a mistake and the gate would make the cue
    /// shy in exactly the moment the ability is best. A cue that checks for a rule the
    /// game has stopped having measures the cue's memory, not the verb.
    fn repel(&self, status: AbilityStatus) -> Option<Bid> {
        // Flight, for Lockdown's reason exactly: a wall only means something to somebody
        // following a route to you, and ground a patrol was walking past anyway costs it
        // nothing to walk round.
        if self.intent != Intent::Flee {
            return None;
        }
        // Somebody has to be coming. With nothing perceived at all the bot is fleeing a
        // lead rather than a hunter, and a wall between it and nobody is a turn and a
        // lockout spent on the geometry of an empty room.
        let gap = self.nearest_guard?;
        // **And there has to be somewhere to be.** A wall buys turns, and turns are only
        // worth something to a bot that is going to spend them walking: pressed while
        // cornered with no step to take, the field holds the guards off for eight turns and
        // then hands the bot back the same cell with a ring around it (§8.3's own warning
        // about what it costs). `route` is `None` exactly when the plan would hold still.
        self.route?;
        // Strong when the hunt is close enough that the detour is a real one, plain when it
        // is far enough that walking on is probably the better turn anyway. Never decisive:
        // an escape that actually *moves* — Run — should win the turn against a wall
        // whenever both speak, and Run claims `URGE_DECISIVE` in its own moment.
        let (urge, reason) = if gap <= REPEL_PRESSING {
            (
                URGE_STRONG,
                "hunted across open ground with the field still clear — put the wall down (§8.3)",
            )
        } else {
            (
                URGE_PLAIN,
                "breaking contact with room to work — stamp the ground they have to go round (§8.3)",
            )
        };
        self.press(status, urge, reason)
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
    fn press(&self, status: AbilityStatus, urge: u8, reason: &'static str) -> Option<Bid> {
        matches!(
            status.state,
            AbilityState::Ready | AbilityState::Limited { .. }
        )
        .then_some(Bid {
            input: Input::Activate(status.id),
            urge,
            reason,
        })
    }

    /// A bid to **hold still** for an ability that is already running and only pays
    /// out while you do not move (§8.3). `None` unless it is genuinely active, so
    /// this can never become a way to wait for nothing.
    fn hold(&self, status: AbilityStatus, urge: u8, reason: &'static str) -> Option<Bid> {
        matches!(status.state, AbilityState::Active { .. }).then_some(Bid {
            input: Input::Wait,
            urge,
            reason,
        })
    }
}

/// An ability's **cue slot**: its position in [`AbilityId::ALL`], which is the index
/// its per-ability floor sits at in [`Profile::cue_floors`].
///
/// Derived from the catalogue order rather than written out, so a new ability lands
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

    /// Every ability has a distinct cue slot, and the slots cover the catalogue —
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
                            // Both takedown appetites (#316): the striking half of the
                            // roster reaches cue arms the avoidance-first half never does.
                            for strikes in [false, true] {
                                let moment = Moment {
                                    state: &state,
                                    intent,
                                    refuge,
                                    nearest_guard,
                                    route,
                                    crossing,
                                    strikes,
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
                                                    AbilityState::Ready
                                                        | AbilityState::Limited { .. }
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
            // Neither cue under test is on the takedown axis; the permissive value keeps
            // this measuring what it always measured.
            strikes: true,
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

        // The press stands on its own; the follow-through is the hold arm re-bid
        // each turn the cloak runs, asserted above.
        let bid = moment(Intent::Flee, None, Some(3))
            .bid(camo)
            .expect("a ready cloak presses");
        assert_eq!(bid.input, Input::Activate(AbilityId::Camouflage));
    }

    /// The **Dart**'s cue answers its own row and nothing wider (§8.3/#239). It is the one
    /// cue that must check the aim itself — the ability is never `Unusable`, deliberately
    /// (§8.4: a greyed entry would be a detector), so `Ready` says nothing about whether
    /// there is anything on the line.
    ///
    /// The two refusals that matter are pinned from both sides: it never speaks in flight
    /// (a hunter has seen you, so it is not a legal target at all), and it never speaks
    /// with no shot resolved — which is what stops the histogram filling up with darts
    /// fired at empty corridors (#347's failure mode, at its sharpest here because the
    /// press is never refused).
    /// **A temperament that declines the takedown declines the dart, at every range**
    /// (§7.2/§13.3/#316) — swept over the shipped roster rather than asserted about one
    /// profile, so it is a claim about the `--profile` vocabulary and not arithmetic.
    ///
    /// This is the invariant, not a detail. `balanced` and `cautious` carry
    /// `takedown_reach: 0`, which that knob's doc defines as *"not 'never gets the chance',
    /// but 'does not want it'"*, and both report `takedowns: 0` in their control columns. A
    /// dart is §7.2's verb with the range changed, so a cue that fired it for them would
    /// hand an avoidance-first bot a kill it would never walk up and take — and the whole
    /// with/without pair would then compare a **different bot** against the old one's
    /// baseline, which is §13.3's failure mode rather than a tuning question. The first
    /// version of this cue did exactly that, and the tell was `balanced` leaving 11 bodies
    /// in a column whose control leaves none.
    #[test]
    fn a_temperament_that_declines_the_takedown_declines_the_dart() {
        let (state, _) = crate::test_support::boot(0);
        let dart = AbilityStatus {
            id: AbilityId::Dart,
            state: AbilityState::Ready,
        };
        for profile in Profile::ALL {
            if profile.takedown_reach > 0 {
                continue; // the striking half is what the cue is *for*
            }
            for intent in [
                Intent::Flee,
                Intent::TakeCover,
                Intent::Pursue,
                Intent::Explore,
            ] {
                for route in [None, Some(Direction::North), Some(Direction::East)] {
                    for nearest_guard in [None, Some(1), Some(6)] {
                        let moment = Moment {
                            state: &state,
                            intent,
                            refuge: None,
                            nearest_guard,
                            route,
                            crossing: None,
                            strikes: profile.takedown_reach > 0,
                        };
                        assert!(
                            moment.bid(dart).is_none(),
                            "{}: declines the takedown verb, so it must decline the dart",
                            profile.name,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_dart_cue_needs_a_real_shot_and_never_fires_in_flight() {
        let (state, _) = crate::test_support::boot(0);
        let dart = AbilityStatus {
            id: AbilityId::Dart,
            state: AbilityState::Ready,
        };
        let moment = |intent, route| Moment {
            state: &state,
            intent,
            refuge: None,
            nearest_guard: Some(6),
            route,
            crossing: None,
            strikes: true,
        };

        // Never in flight — a guard hunting you has detected you (§7.2).
        for intent in [Intent::Flee, Intent::TakeCover] {
            assert!(
                moment(intent, Some(Direction::North)).bid(dart).is_none(),
                "{intent:?}: an aware guard is no target",
            );
        }
        // And never without a resolved shot. The boot state has nobody lined up on the
        // player's facing, so `dart_shot` finds no legal target and the cue declines —
        // whatever the intent, and whatever the route.
        assert!(state.dart_shot().hit().is_none(), "fixture precondition");
        for intent in [Intent::Pursue, Intent::Explore] {
            for route in [None, Some(Direction::North), Some(Direction::East)] {
                assert!(
                    moment(intent, route).bid(dart).is_none(),
                    "{intent:?}/{route:?}: no shot, no bid",
                );
            }
        }
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
            strikes: true,
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
            strikes: true,
        };
        let keen = balanced.with_cue_floor(AbilityId::Camouflage, URGE_NONE);
        assert_eq!(
            moment.best(&keen).map(|b| b.input),
            Some(Input::Activate(AbilityId::Run)),
            "the bare loadout holds no cloak to press",
        );
    }
}
