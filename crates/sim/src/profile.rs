//! Playstyle profiles (§13.2): the baseline bot's temperament, as one row of
//! numbers.
//!
//! The bot's behaviour was governed by a handful of hard-coded thresholds — how
//! wide a berth it gives a patrol, how early it ducks into cover, how long it
//! waits there. Those numbers *are* its temperament, so lifting them into a
//! [`Profile`] the bot reads turns one fixed player into a small set of named
//! ones, all running the **same policy**.
//!
//! # Why more than one temperament
//!
//! §13.2 names strategy diversity *"the most important and the least obvious"*
//! metric: **win rate tells you if the game is hard, strategy diversity tells you
//! if it is interesting.** A single fixed bot can never surface it — it always
//! plays the one way, so every seed it solves is solved once. Running `cautious`
//! and `aggressive` over the *same* facility is how the sim says a seed is
//! solvable two ways (healthy) or that both temperaments collapse onto the same
//! line (a puzzle with one answer). Where the two **disagree** — one wins by
//! waiting, the other is caught pushing — is exactly the §13.3 flag worth playing.
//!
//! # A profile is a temperament, not a solver (§13.4)
//!
//! Every profile stays the greedy, legible baseline: explore → take the intel →
//! leave, ducking into cover when a patrol closes. `aggressive` is **not** a
//! min-maxer looking for the optimal line — that would make its metrics measure
//! the bot rather than the game, which is the one thing the sim exists to avoid.
//! Only the numbers differ. If a temperament ever wants a genuinely different
//! *decision* rather than a different number, that is a signal to stop and
//! reconsider, not to fork [`decide`](crate::StealthBot).
//!
//! Every value here is **[START]** (§13.4): free to move, pinned only by the
//! shape assertions in `bot.rs`, never by a leaderboard.

use intrusion_core::AbilityId;

use crate::cue::{self, URGE_PLAIN};

/// How the bot weighs its options when stepping down the routing field — the
/// difference between picking a careful route to an objective and bolting for
/// cover. Two flags per descent, and each profile sets both for both descents,
/// so "walks into cones" is a temperament like any other number.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Descent {
    /// Add a keep-away cost near every perceived guard, so a route gives patrols
    /// a wide berth rather than skimming them.
    pub keep_clear: bool,
    /// Refuse a step into a cone from a currently-safe cell (hold instead) — the
    /// patience to let a sweep pass, at the price of the turns it costs.
    pub hold_watched: bool,
}

/// A playstyle profile: the whole of the bot's temperament, as data.
///
/// Adding a profile is one more row of numbers, never a second policy — see the
/// module docs for why that constraint is the point rather than a convenience.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Profile {
    /// The profile's stable name: the `--profile` argument, and the string every
    /// emitted row carries so a batch's output is attributable (§13.2).
    pub name: &'static str,
    /// The cost penalty added to a step that enters a cell a visible guard is
    /// watching (§11.5). Far larger than any real path distance on the v1
    /// footprint (a 40×40 facility bounds any route well under this), so a
    /// watched step is always dearer than any unwatched one, while still being
    /// *comparable* — when every option is watched, the least-watched, shortest
    /// route still wins.
    pub watched_penalty: u64,
    /// How near a perceived guard a cell must be to draw a keep-away penalty
    /// (Manhattan). Inside this radius the bot steers wide; outside it, a guard
    /// is far enough to ignore while routing. Also the range within which a held
    /// bot sidesteps rather than standing.
    ///
    /// The same radius covers seen and sensed guards. A *wider* berth for
    /// sensed-only guards (whose facing is unknown, §9.2) was tried under #196
    /// and rejected: the §13.2 sim showed it did not lower detections and cost
    /// the bot wins/timeouts — steering omnidirectionally away from an exact cell
    /// is as apt to walk into the unseen cone as away from it, and the bigger
    /// halo just lengthens exposed routes.
    pub proximity_radius: u32,
    /// The keep-away weight per unit of closeness-squared. Sized to dominate raw
    /// path distance on the v1 footprint — so the bot will take a long way round
    /// to keep its distance — while staying well under [`Profile::watched_penalty`],
    /// which no amount of proximity may ever outweigh.
    pub proximity_unit: u64,
    /// How near a perceived guard must be (Manhattan) for the bot to break off
    /// and take cover before it is seen (§7.6). Wide enough to react while the
    /// guard is still out of striking range, narrow enough not to hide from a
    /// patrol two rooms away.
    pub threat_radius: u32,
    /// The furthest a hideout may be (Manhattan) and still be worth diverting to
    /// for cover. A bolthole further than this is not shelter — reaching it means
    /// marching across the very patrol being dodged — so the bot routes carefully
    /// on instead.
    pub cover_reach: u32,
    /// Once hidden, the bot stays put until the nearest perceived guard is beyond
    /// this (Manhattan) — wider than [`Profile::threat_radius`] so a guard
    /// loitering just outside does not make it pop in and out, glimpsed on every
    /// step.
    pub clear_radius: u32,
    /// The most turns the bot will wait out a patrol from one cover stint. A
    /// guard whose beat keeps it parked nearby would otherwise pin the bot in its
    /// cupboard forever; past this it gives up hiding and pushes on, trading a
    /// certain timeout for a chance.
    pub max_hide: u32,
    /// Turns of committed pursuit after leaving cover before the bot may hide
    /// again — the anti-oscillation guard that keeps a patrol looping past a
    /// cupboard from trapping it in an endless in-and-out.
    pub cover_cooldown: u32,
    /// How the bot routes toward an objective.
    pub pursue: Descent,
    /// How the bot routes when bolting for a refuge.
    pub flee: Descent,
    /// How far out of its way this temperament will go to spring a takedown from a
    /// guard's **rear blind spot** (§7.2/§155), counted in steps of route that cross
    /// no cone.
    ///
    /// **Zero declines the verb outright** — not "never gets the chance", but "does
    /// not want it": an unaware guard is left blocked, so the router waits the patrol
    /// out exactly as it did before this knob existed. That is what keeps the
    /// avoidance-first temperaments byte-identical across this seam (#316), and it is
    /// the honest reading of a bot that steers wide of guards rather than hunting
    /// them (§13.3) — a cautious profile reporting `takedowns: 0` is **correct
    /// behaviour**, not a defect.
    ///
    /// Larger is keener rather than better (§13.4). The budget is the *whole* point:
    /// a takedown taken because the route offered one cheaply measures the game; a
    /// bot that crosses the facility to hunt measures the bot.
    pub takedown_reach: u32,
    /// How far this temperament will haul a body to stow it in a cupboard
    /// (§8.3/§10.3), counted the same way — the tidy-up half of §7.2's cost.
    ///
    /// **Zero leaves every body where it fell**, which is not laziness but the other
    /// half of the measurement: a stowed body is *gone* (no cone ever finds it), so a
    /// bot that always tidies up drives `bodies_found` to zero and §7.3's radio clock
    /// goes back to being untested. Splitting the two across temperaments is how one
    /// batch covers the drag/stow chain and another covers body discovery (#316).
    pub body_stow_reach: u32,
    /// Whether this temperament will **duck behind a bench** at all (§10.3/#379).
    ///
    /// A flag rather than a reach, and the shape is the finding. The obvious sibling
    /// to [`takedown_reach`](Profile::takedown_reach) — *how far will it walk to one* —
    /// was built and measured out again: a bench you walk to goes **stale**, because
    /// the spot is chosen for where a guard stands now and the concealing side of the
    /// furniture has flipped by the time you arrive. Over 100 seeds a reach of 2 or
    /// more did not add crouches, it **replaced** them, from ~51 down to ~1, as the bot
    /// spent its cover turns walking to benches it never ducked behind. So the crouch
    /// is only ever taken from where the bot already stands, and the one thing left to
    /// say about it is yes or no.
    ///
    /// **True for every temperament that spends turns on cover at all**, because from
    /// your own cell it is a reflex rather than an appetite: ducking behind the table
    /// at your elbow when a patrol walks in is what anybody does, careful or impatient.
    /// The profiles that do it still crouch at very different rates, on the numbers
    /// they already carry — how near a patrol must be before cover is worth a turn
    /// ([`threat_radius`](Profile::threat_radius)), and how far a *cupboard* is worth
    /// walking to instead ([`cover_reach`](Profile::cover_reach)).
    ///
    /// **False is what [`CARELESS`](Profile::CARELESS) needs**, and it is the same
    /// decision as its `cover_reach: 0`: a temperament that spends no turn on
    /// concealment must decline *all* of it, or its §7.2 row stops meaning what it is
    /// there to mean.
    pub crouches: bool,
    /// The **urge floor** each ability's cue must clear before the bot will press
    /// it (§13.2/#346), indexed by [`cue::slot`] — the ability's permanent position
    /// in [`AbilityId::ALL`].
    ///
    /// One floor **per ability**, deliberately: a single shared threshold would
    /// make every cue's keenness the same dial, and there would be nothing to turn
    /// for one verb without turning it for all of them. Sweeping one ability's
    /// floor from 0 to past [`URGE_DECISIVE`](crate::cue::URGE_DECISIVE) and reading
    /// the curve is what separates "weak ability" from "shy cue" — the ambiguity
    /// the cue seam introduces and the only thing that resolves it. Build a swept
    /// profile with [`with_cue_floor`](Profile::with_cue_floor).
    ///
    /// [START] like everything else here: the default is
    /// [`URGE_PLAIN`](crate::cue::URGE_PLAIN), so a *plain fit* is the weakest thing
    /// that presses a key.
    pub cue_floors: [u8; AbilityId::ALL.len()],
}

impl Profile {
    /// **The middle temperament: steers wide of a patrol, takes cover when one
    /// closes, and waits it out for a while — but pushes on rather than sit in a
    /// cupboard all run.** It sits between [`CAUTIOUS`](Profile::CAUTIOUS) and
    /// [`AGGRESSIVE`](Profile::AGGRESSIVE) on every number they disagree about,
    /// which is what makes it the default and the one a single-profile batch runs.
    ///
    /// It is also, historically, the numbers the policy carried as constants before
    /// they were data, so every metric captured under it stays comparable with the
    /// batches that came before the profile seam existed.
    pub const BALANCED: Profile = Profile {
        name: "balanced",
        watched_penalty: 1_000_000,
        proximity_radius: 5,
        proximity_unit: 1_000,
        threat_radius: 6,
        cover_reach: 8,
        clear_radius: 8,
        max_hide: 12,
        cover_cooldown: 8,
        pursue: Descent {
            keep_clear: true,
            hold_watched: true,
        },
        flee: Descent {
            keep_clear: false,
            hold_watched: false,
        },
        // Avoidance-first: it does not want the verb (§13.3), so it never strikes and
        // never has a body to tidy. Both zero is what makes "today's bot, unchanged"
        // still true after #316 added the play.
        takedown_reach: 0,
        body_stow_reach: 0,
        // It *does* duck behind a table when a patrol walks in on it and there is no
        // cupboard to reach — survival, not a plan, and never a detour (#379). Unlike
        // the strike, this moves this profile's numbers rather than leaving them
        // byte-identical, because the crouch is an innate reflex rather than a
        // temperament's appetite: declining it outright would have been a claim about
        // *this bot* rather than about avoidance-first play.
        crouches: true,
        // Every cue starts at the same plain-fit floor. The floors are a per-ability
        // dial precisely so a *sweep* can move one of them; a profile that shipped
        // with them already scattered would make its own metrics harder to read.
        cue_floors: [URGE_PLAIN; AbilityId::ALL.len()],
    };

    /// **Gives patrols a wide berth, ducks into cover early and waits long.**
    /// Trades speed for never being seen: a halo half again as wide, a threat
    /// radius that reacts a room sooner, boltholes worth a longer detour, and the
    /// patience to sit one out for twice as many turns. Its flight keeps clear
    /// too — even bolting, it would rather round a patrol than brush past it.
    pub const CAUTIOUS: Profile = Profile {
        name: "cautious",
        proximity_radius: 8,
        threat_radius: 9,
        cover_reach: 12,
        clear_radius: 10,
        max_hide: 24,
        // Short, because the point of this temperament is to hide *often*: the
        // cooldown exists only to stop an in-and-out shuffle, not to force a
        // stretch of exposure.
        cover_cooldown: 4,
        flee: Descent {
            keep_clear: true,
            hold_watched: false,
        },
        ..Profile::BALANCED
    };

    /// **Pushes toward the objective, tolerates a cone to save turns, hides late
    /// and briefly — and clears a patrol out of its way when the route offers the
    /// angle.** A tight halo it will skim rather than round, cover taken only when a
    /// patrol is nearly on top of it, and — the temperament's signature —
    /// `hold_watched: false` while pursuing, so it walks a watched cell instead of
    /// waiting the sweep out. It is not a better player, just an impatient one
    /// (§13.4): it should be *detected more*, which is the point.
    ///
    /// It also **tidies up after itself**: a short strike detour and a stow reach
    /// wide enough to reach the cupboard the guard was patrolling past, so its rows
    /// carry the whole §7.2 chain — takedown, drag, stow — rather than only its first
    /// link. What it does *not* cover is body discovery, because a stowed body can
    /// never be found; that is [`CARELESS`](Profile::CARELESS)'s job.
    ///
    /// It **crouches** too (§10.3/#379), and needs no number of its own to say how
    /// much: the bench is the cover it settles for because a cupboard is only worth
    /// five cells to it. What its tight `threat_radius` costs is the *pose* rather than
    /// the duck — it ducks with a guard already on top of it, so the patrol is round
    /// the furniture within a turn and the crouch buys about one turn of cover, where
    /// `cautious` ducks early and holds for a dozen.
    pub const AGGRESSIVE: Profile = Profile {
        name: "aggressive",
        // A quarter of the balanced weight over a radius of 3: a patrol still
        // bends the route, but no longer sends it the long way round.
        proximity_radius: 3,
        proximity_unit: 250,
        threat_radius: 3,
        cover_reach: 5,
        clear_radius: 5,
        max_hide: 6,
        // Long, so a hide is a rare interruption to the push rather than the plan.
        cover_cooldown: 16,
        pursue: Descent {
            keep_clear: true,
            hold_watched: false,
        },
        // Short: a guard already beside the route, not one across the map. Four steps
        // is about a room's width on the v1 footprint, so the detour stays a
        // *diversion* from the push rather than becoming the push.
        takedown_reach: 4,
        // Wider than the strike detour, because the haul is the expensive half: at
        // half speed (§8.3) a six-step carry is a dozen turns, which is exactly the
        // price §7.2 means the body to be.
        body_stow_reach: 6,
        ..Profile::BALANCED
    };

    /// **Strikes readily, never hides, never tidies up.**
    /// [`AGGRESSIVE`](Profile::AGGRESSIVE)'s impatience carried one step further:
    /// twice the detour for a rear blind spot, no cupboard ever worth diverting to,
    /// and a stow reach of zero, so every body it leaves stays on the floor where a
    /// patrol will eventually walk its cone over it.
    ///
    /// Declining the crouch is part of the same bargain: it is the profile whose §7.2
    /// row is **only** §155, so a bench would blur the very split it is here to keep.
    ///
    /// It exists because a *tidy* bot cannot measure body discovery — stowing puts a
    /// body beyond every cone, so the tidier the temperament, the flatter §7.3's
    /// radio clock and the `bodies_found` row read (§13.2). This is the profile those
    /// two rows have a live source from, and #198 anticipated it by name: *"a
    /// takedown-happy one … cheap follow-ups once the `Profile` seam exists"*.
    ///
    /// **Keener does not mean more**, and the measured numbers say so plainly: over
    /// 100 seeds this lands *fewer* takedowns than `aggressive` (11 against 18),
    /// because refusing every kind of concealment — cupboards and benches alike — costs
    /// it every concealed strike (§7.2) and leaves it only the rear blind spot (§155)
    /// to work with. That is the split doing its job rather than a mis-tune — one
    /// temperament per legal angle — and it is worth remembering before anyone reads
    /// the bigger reach as the bigger number.
    ///
    /// Not a *better* player for striking at all (§13.4) — bodies on the floor are how
    /// a run gets loud, and its detections should say so.
    pub const CARELESS: Profile = Profile {
        name: "careless",
        takedown_reach: 8,
        // **It does not use cupboards at all.** No bolthole is ever worth diverting
        // to, so it never ducks in, never waits a patrol out, and never stows. That
        // needs no new mechanism — a reach of zero is the existing knob turned all the
        // way down — and it sharpens what the profile measures: with concealment off
        // the table, its takedowns can only be **rear blind spot** strikes (§155),
        // while `aggressive` still gets the cupboard-mouth concealment ones (§7.2). One
        // temperament per legal angle, rather than both crowding onto the easy one.
        cover_reach: 0,
        // **And it does not crouch** (#379), which is the same decision as the line
        // above rather than a second one: a temperament that spends no turn on a
        // cupboard spends none on worse cover either. It is what keeps this profile's
        // takedowns readable — with concealment off the table entirely, every strike it
        // lands is a **rear blind spot** one (§155), while `aggressive` gets the
        // concealed ones (§7.2). One temperament per legal angle, which is the split
        // #316 built these two around and the reason the row is worth reading at all.
        crouches: false,
        // The signature: it drops what it is carrying rather than spend a dozen turns
        // hauling. The grab itself is not optional — stepping off a body's cell takes
        // hold automatically (§8.3/#187) — so "never stows" is a decision the bot has
        // to *act* on, by letting go, not one it can make by standing still.
        body_stow_reach: 0,
        ..Profile::AGGRESSIVE
    };

    /// Every profile that ships, in a fixed order — the `--profile` vocabulary,
    /// and the order a multi-profile report walks.
    pub const ALL: [Profile; 4] = [
        Profile::BALANCED,
        Profile::CAUTIOUS,
        Profile::AGGRESSIVE,
        Profile::CARELESS,
    ];

    /// The profile called `name`, or `None` when nothing is. Lookup is exact:
    /// a near-miss is an error the caller reports with the vocabulary, never a
    /// silent fall back to the default (which would make a batch's rows lie
    /// about what produced them).
    pub fn by_name(name: &str) -> Option<Profile> {
        Profile::ALL.into_iter().find(|p| p.name == name)
    }

    /// The urge an `id` cue must reach before this temperament will press it
    /// (§13.2/#346) — the per-ability half of [`cue_floors`](Profile::cue_floors).
    pub fn cue_floor(&self, id: AbilityId) -> u8 {
        self.cue_floors[cue::slot(id)]
    }

    /// This profile with **one** ability's cue floor moved — the handle a threshold
    /// sweep turns, one verb at a time, to read the curve that says whether a
    /// near-zero histogram slot is a weak ability or a shy cue.
    ///
    /// It keeps the profile's name, because it is still that temperament: what
    /// moved is how keen one cue has to be, not how the bot routes or hides. A
    /// sweep that wants its rows attributable records the floor alongside the name.
    pub fn with_cue_floor(mut self, id: AbilityId, floor: u8) -> Self {
        self.cue_floors[cue::slot(id)] = floor;
        self
    }

    /// The shipped profile names, comma-separated — the one place an error
    /// message or a usage string gets the vocabulary, so it cannot drift from
    /// [`Profile::ALL`].
    pub fn names() -> String {
        Profile::ALL
            .iter()
            .map(|p| p.name)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl Default for Profile {
    fn default() -> Self {
        Profile::BALANCED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `--profile` vocabulary round-trips, and an unknown name is an error
    /// rather than a quiet default — a row that claims a profile it did not run
    /// under is worse than a refused batch (§13.2: output must be attributable).
    #[test]
    fn every_profile_is_reachable_by_its_own_name() {
        for profile in Profile::ALL {
            assert_eq!(
                Profile::by_name(profile.name),
                Some(profile),
                "{} must resolve to itself",
                profile.name,
            );
        }
        assert_eq!(Profile::by_name("Balanced"), None, "lookup is exact");
        assert_eq!(Profile::by_name("reckless"), None);
        assert_eq!(Profile::names(), "balanced, cautious, aggressive, careless");
    }

    /// Names are unique, or `--profile` would be ambiguous and a row's
    /// attribution meaningless.
    #[test]
    fn the_profile_names_are_distinct() {
        for (i, a) in Profile::ALL.iter().enumerate() {
            for b in &Profile::ALL[i + 1..] {
                assert_ne!(a.name, b.name, "two profiles share a name");
            }
        }
    }

    /// The structural invariant every profile must keep, whatever its
    /// temperament: a cone must outweigh the *whole* keep-away halo, or a bot
    /// would take a watched step to dodge a guard it can already see — the
    /// ordering [`Profile::watched_penalty`] exists to guarantee.
    #[test]
    fn no_profiles_halo_can_outweigh_a_cone() {
        for p in Profile::ALL {
            let steepest = u64::from(p.proximity_radius + 1);
            let worst = steepest * steepest * p.proximity_unit;
            assert!(
                worst < p.watched_penalty,
                "{}: a halo of {worst} rivals the {} cone penalty",
                p.name,
                p.watched_penalty,
            );
        }
    }

    /// The takedown axis is a **temperament**, not a capability the seam hands to
    /// everybody (#316). Two claims worth pinning, because both are load-bearing:
    ///
    /// - the avoidance-first profiles decline the verb outright, which is what makes
    ///   "the avoidance-first temperaments are unchanged" true by construction rather
    ///   than by
    ///   measurement — a reach of zero never reaches a line of the strike code;
    /// - a profile that never strikes has no body to deal with, so a stow reach on one
    ///   would be a number that could never do anything.
    #[test]
    fn only_the_striking_temperaments_carry_a_takedown_reach() {
        // Read back through the name lookup rather than off the constants, so these
        // are assertions about the shipped `--profile` vocabulary and not arithmetic
        // the compiler can fold away.
        let by = |name: &str| Profile::by_name(name).expect("a shipped profile");
        let (declines, strikes): (Vec<Profile>, Vec<Profile>) = Profile::ALL
            .into_iter()
            .partition(|p| p.takedown_reach == 0);

        assert_eq!(
            declines.iter().map(|p| p.name).collect::<Vec<_>>(),
            ["balanced", "cautious"],
            "exactly the avoidance-first temperaments decline the verb",
        );
        for p in &declines {
            assert_eq!(
                p.body_stow_reach, 0,
                "{}: never strikes, so it can never have a body to stow",
                p.name,
            );
        }
        assert_eq!(
            strikes.iter().map(|p| p.name).collect::<Vec<_>>(),
            ["aggressive", "careless"],
        );

        // The two that strike differ in *both* directions, or they would be one
        // temperament measured twice: careless wants it more and tidies less.
        assert!(
            by("careless").takedown_reach > by("aggressive").takedown_reach,
            "careless must be the keener striker",
        );
        assert!(
            by("aggressive").body_stow_reach > 0,
            "aggressive tidies up — that is what covers the drag/stow chain",
        );
        assert_eq!(
            by("careless").body_stow_reach,
            0,
            "careless never does — that is what gives `bodies_found` a source",
        );
    }

    /// **Concealment is one decision, not two** (#379). `careless` refuses the
    /// cupboard *and* the bench, and the pairing is what its §7.2 row rests on: with
    /// no concealment of any kind available to it, every takedown it lands is a rear
    /// blind spot one (§155), while `aggressive` covers the concealed angle. Split the
    /// pair — let it duck behind furniture while refusing cupboards — and the two
    /// profiles both cover both angles, which is one temperament measured twice.
    ///
    /// The converse is asserted too, because it is the easier thing to get wrong: a
    /// profile that *does* spend turns on cover must also crouch. From your own cell
    /// the pose is a reflex rather than an appetite, so declining it there would be a
    /// claim about the bot rather than about the temperament.
    #[test]
    fn a_profile_that_refuses_cupboards_refuses_benches_too() {
        for p in Profile::ALL {
            assert_eq!(
                p.crouches,
                p.cover_reach > 0,
                "{}: concealment must be one decision — cover_reach {} against \
                 crouches {}",
                p.name,
                p.cover_reach,
                p.crouches,
            );
        }
        // Named rather than only derived, so the shipped vocabulary is the assertion.
        let by = |name: &str| Profile::by_name(name).expect("a shipped profile");
        assert!(!by("careless").crouches, "careless declines the pose");
        for name in ["balanced", "cautious", "aggressive"] {
            assert!(by(name).crouches, "{name} takes cover, so it crouches");
        }
    }

    /// Hysteresis, per profile: the bot must come out of cover on a *wider*
    /// radius than the one that sent it in, or it pops out onto a guard still on
    /// its doorstep and ducks straight back, glimpsed each time.
    #[test]
    fn every_profile_leaves_cover_later_than_it_takes_it() {
        for p in Profile::ALL {
            assert!(
                p.clear_radius > p.threat_radius,
                "{}: clear {} must exceed threat {}",
                p.name,
                p.clear_radius,
                p.threat_radius,
            );
        }
    }
}
