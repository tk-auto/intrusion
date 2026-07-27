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
}

impl Profile {
    /// **Today's bot, unchanged.** The numbers the policy carried as constants
    /// before they were data, so every metric captured under it stays comparable
    /// with the batches that came before the profile seam existed.
    pub const BASELINE: Profile = Profile {
        name: "baseline",
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
        ..Profile::BASELINE
    };

    /// **Pushes toward the objective, tolerates a cone to save turns, hides late
    /// and briefly.** A tight halo it will skim rather than round, cover taken
    /// only when a patrol is nearly on top of it, and — the temperament's
    /// signature — `hold_watched: false` while pursuing, so it walks a watched
    /// cell instead of waiting the sweep out. It is not a better player, just an
    /// impatient one (§13.4): it should be *detected more*, which is the point.
    pub const AGGRESSIVE: Profile = Profile {
        name: "aggressive",
        // A quarter of the baseline weight over a radius of 3: a patrol still
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
        ..Profile::BASELINE
    };

    /// Every profile that ships, in a fixed order — the `--profile` vocabulary,
    /// and the order a multi-profile report walks.
    pub const ALL: [Profile; 3] = [Profile::BASELINE, Profile::CAUTIOUS, Profile::AGGRESSIVE];

    /// The profile called `name`, or `None` when nothing is. Lookup is exact:
    /// a near-miss is an error the caller reports with the vocabulary, never a
    /// silent fall back to the baseline (which would make a batch's rows lie
    /// about what produced them).
    pub fn by_name(name: &str) -> Option<Profile> {
        Profile::ALL.into_iter().find(|p| p.name == name)
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
        Profile::BASELINE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `--profile` vocabulary round-trips, and an unknown name is an error
    /// rather than a quiet baseline — a row that claims a profile it did not run
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
        assert_eq!(Profile::by_name("Baseline"), None, "lookup is exact");
        assert_eq!(Profile::by_name("reckless"), None);
        assert_eq!(Profile::names(), "baseline, cautious, aggressive");
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
