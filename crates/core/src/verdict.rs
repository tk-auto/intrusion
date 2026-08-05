//! How a run ended, what it cost, and what the player may do next (§14 v2/§2.2, #138).
//!
//! The turn loop already knows *that* a run is over ([`Outcome`](crate::Outcome), §4.5).
//! What it did not keep is the one thing the end screen exists to show: **why**. §2.2's
//! promise is that "every capture must be traceable to a decision the player made", and
//! a screen that can only say *you lost* keeps none of it — the tracing needs the guard,
//! the mood it was in, and the cell contact happened on.
//!
//! So the loop **latches the terminal event** into an [`Ending`] the moment it fires
//! ([`State::ending`](crate::State::ending)), and the renderer presents that. It never
//! reconstructs the cause from the finished board: by the time the screen draws, the
//! guard that caught you has been standing on your cell for as long as the screen has
//! been up, and its mood at the moment of contact is gone. One reading, at the one
//! instant it is true.
//!
//! # The exits belong to the mode, not to the screen
//!
//! The same screen ends a quick-play run and, one day, a campaign run — and they may
//! not offer the same way on. Quick play is **training**: a level is a thing you retry
//! until you have read the building. The campaign *is* the run (§2.2), and a run you
//! can restart is not a permadeath run at all. So the exit set is a property of
//! [`RunMode`] ([`RunMode::exits`]), and a campaign cannot inherit a retry button by
//! reusing this screen. §14 v3 does not exist yet; the gate does, in shape, which is
//! the whole point of writing it down now (see design appendix 31).

use crate::ability::AbilityId;
use crate::cell::Cell;
use crate::difficulty::Difficulty;
use crate::guard::GuardState;
use crate::level_seed::LevelSeed;

/// **Why** a run ended (§4.5) — the terminal event, latched at the instant it fired.
///
/// One variant per way a run can stop, each carrying exactly what the end screen has
/// to say and nothing more. The two losses are genuinely different facts and read
/// differently: a guard reached you, or the wall did (§8.3's phase safety, #329).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ending {
    /// A guard walked into the player (§4.5) — the game's one ordinary loss.
    ///
    /// `guard` indexes [`State::guards`](crate::State::guards), `state` is the mood it
    /// held **at the moment of contact** (§7.4), and `at` is the cell it happened on.
    /// The mood is the load-bearing half: a red Chasing capture is the end of a hunt
    /// you knew you were in, and a yellow Calm one is a patrol that walked into a
    /// cell you thought nobody was coming to — two different mistakes, and the screen
    /// must not blur them into "caught".
    Captured {
        guard: usize,
        state: GuardState,
        at: Cell,
    },
    /// The player was left inside a solid with nowhere to be thrown clear to
    /// (§8.3/#329) — the degenerate phase loss, kept truthful rather than hidden.
    Entombed { at: Cell },
    /// The intel gate was satisfied and the exit reached (§4.5): the run is won.
    Escaped,
}

impl Ending {
    /// Whether this ending is a **win**. The screens differ in more than a heading —
    /// what is worth saying about a won run is not what is worth saying about a lost
    /// one — so the two are told apart here rather than by matching variants at every
    /// draw site.
    pub fn won(self) -> bool {
        matches!(self, Ending::Escaped)
    }
}

/// What a run amounted to, in the five numbers worth reading afterwards (§14 v2).
///
/// Every one of them is a fact the [`State`](crate::State) already holds or counts;
/// this type is the *reading*, taken once when the screen draws. Kept plain and
/// `Copy` so the renderer can be handed the numbers and nothing else.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RunStats {
    /// Spent turns (§4.4) — free actions excluded, exactly as the sim counts them
    /// ([`State::turn`](crate::State::turn)).
    pub turns: u32,
    /// Consoles taken, and how many the level held: the run's actual haul (§4.5).
    pub intel: usize,
    /// How many objectives the level had in total.
    pub intel_total: usize,
    /// Takedowns landed (§7.2) — the permanent cost the run chose to pay.
    pub takedowns: u32,
    /// How often stealth **broke**: fresh detections (§7.6), counted on the
    /// transition, so a chase that holds you in sight for ten turns is one.
    pub detections: u32,
    /// The facility alert's peak rung (§7.3). The ladder never decays, so the rung
    /// standing at the end *is* the peak.
    pub alert_peak: u32,
    /// The **tech salvaged** from this facility's equipment cache (§2.2/§8.3/§14
    /// v3/#209), or `None` — no cache, or one left unopened, which is a legal and
    /// sometimes correct choice.
    ///
    /// **This is the seam the run's power curve travels along.** A raid's haul crosses
    /// into the campaign through the verdict and nothing else (`Campaign::complete`), so
    /// the ability found in a crate rides beside the intel taken rather than through a
    /// second channel of its own — one place where "what a facility was worth" is said,
    /// and the layer above folds it into the loadout every later facility boots with.
    ///
    /// `Option` and not a list because a facility hides at most one crate
    /// ([`LevelConfig::caches`](crate::LevelConfig)); it also keeps [`RunStats`] `Copy`,
    /// which the end screen relies on.
    pub salvaged: Option<AbilityId>,
}

/// A finished run, as the end screen reads it: why it ended, and what it cost.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Verdict {
    /// Why the run ended — the latched terminal event.
    pub ending: Ending,
    /// What the run amounted to.
    pub stats: RunStats,
}

/// **How a run is being played** — which decides what the end screen may offer
/// (§2.2/§14, appendix 31).
///
/// Not a difficulty and not a level modifier (§12.6): those bend the facility, and
/// this bends nothing inside it. It says what the run *is for* — practice, or the
/// run itself — and the only thing that reads it is [`exits`](Self::exits).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RunMode {
    /// Quick play (§10.2/§14 v1): one tuned facility, played as **training**. The
    /// level is a building to learn, so the end screen hands back the same level and
    /// a fresh one. The [`Default`], because it is the only mode v1 ships.
    #[default]
    QuickPlay,
    /// The campaign (§14 v3): 2–3 hours, and **the run is the game** (§2.2). Its end
    /// screen offers no way to play the run again, because permadeath means there is
    /// no again. Nothing constructs this yet — it exists so the shape of the gate is
    /// in the code before the mode that needs it is.
    Campaign,
}

impl RunMode {
    /// The exits this mode's end screen offers, in the order they are drawn.
    ///
    /// Quick play offers all three; the campaign offers only the way out of the run
    /// it just ended. This is the whole gate, and it is one function so that adding
    /// the campaign cannot quietly grant it a retry: a new mode has to answer here.
    pub fn exits(self) -> &'static [EndExit] {
        match self {
            RunMode::QuickPlay => &[EndExit::Retry, EndExit::NewRun, EndExit::Menu],
            RunMode::Campaign => &[EndExit::Menu],
        }
    }
}

/// The run's framing, as the shell holds it: the mode that decides the exits, and the
/// options a fresh run inherits.
///
/// It is not part of [`LevelSeed`] and must not become part of it: a token names a
/// **level** (§13.1/#245), and neither of these is one. The difficulty in particular
/// is already resolved into a level's modifiers by the time it boots (§12.6/#298) —
/// what is kept here is the *setting the player chose*, which is what "a new run with
/// the same options" means.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RunOptions {
    /// Whether this is a training run or the campaign — the exit gate.
    pub mode: RunMode,
    /// The difficulty the run was started at (§12.6/#298), so *new run* rolls the
    /// same setting rather than dropping silently back to Standard.
    pub difficulty: Difficulty,
}

/// One way on from the end screen (§14 v2/#138) — the three actions a finished run
/// offers, gated by [`RunMode::exits`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EndExit {
    /// Play **this level** again: the same seed, the same modifiers, the same loadout
    /// — the run exactly as it was, from turn one (§12.4). The [`Default`], and the
    /// first row: after a capture the thing a player most often wants is the building
    /// they have just started to learn.
    #[default]
    Retry,
    /// Roll a **fresh** level at the same options — quick play without the trip
    /// through the menu.
    NewRun,
    /// Back to the title screen.
    Menu,
}

impl EndExit {
    /// The row's label, in §11.8's meta vocabulary: these name the *game around the
    /// run*, so they read as plainly as the menu's own entries do.
    pub fn label(self) -> &'static str {
        match self {
            EndExit::Retry => "Retry this level",
            EndExit::NewRun => "New run",
            EndExit::Menu => "Back to menu",
        }
    }

    /// The level this exit boots, or `None` for an exit that starts no run.
    ///
    /// **The rule lives here, in the core, so both halves are pinned by a test** —
    /// what *retry* means (§12.4: the identical [`LevelSeed`], so the identical run)
    /// and what *new run* means (the same options, a different facility). The shell
    /// contributes only `fresh_seed`, its one impurity: a reading of the clock,
    /// narrowed to the token's seed width ([`LevelSeed::narrow_seed`]) so every run
    /// the game creates is still sayable.
    pub fn level(
        self,
        played: &LevelSeed,
        options: RunOptions,
        fresh_seed: u64,
    ) -> Option<LevelSeed> {
        match self {
            // The same config, verbatim — not "quick play at this seed again", which
            // would re-resolve the preset and hand back a different run the day the
            // preset moves (the #333 lesson, one surface over).
            EndExit::Retry => Some(*played),
            EndExit::NewRun => Some(LevelSeed::quick_play_at(fresh_seed, options.difficulty)),
            EndExit::Menu => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level_seed::start_level;
    use crate::render::render;

    /// The gate the campaign will inherit (§2.2/appendix 31): **the exits come from
    /// the mode**. Quick play is training and may be replayed; a campaign run is the
    /// run, so its only way on is out.
    #[test]
    fn only_quick_play_offers_a_way_to_play_again() {
        assert_eq!(
            RunMode::QuickPlay.exits(),
            &[EndExit::Retry, EndExit::NewRun, EndExit::Menu],
        );
        assert_eq!(RunMode::Campaign.exits(), &[EndExit::Menu]);
        for exit in [EndExit::Retry, EndExit::NewRun] {
            assert!(
                !RunMode::Campaign.exits().contains(&exit),
                "{exit:?} is a training affordance, not a campaign one",
            );
        }
    }

    /// **Retry reproduces the level exactly** (§12.4): the same seed, the same
    /// modifiers, the same loadout — asserted by booting the level the exit hands
    /// back and comparing the first frame glyph for glyph, not by eye.
    #[test]
    fn retry_boots_the_very_same_run() {
        let played = LevelSeed::quick_play_at(8371, Difficulty::Harder);
        let options = RunOptions {
            mode: RunMode::QuickPlay,
            difficulty: Difficulty::Harder,
        };
        let again = EndExit::Retry
            .level(&played, options, 99)
            .expect("retry starts a run");
        assert_eq!(again, played, "retry is the run as it was, config and all");

        let first = start_level(&played).expect("the v1 footprint carves");
        let second = start_level(&again).expect("the v1 footprint carves");
        assert_eq!(
            render(&first).to_text(),
            render(&second).to_text(),
            "the retried level is the level, cell for cell",
        );
    }

    /// **New run keeps the options and changes the seed.** The difficulty the player
    /// chose rides along — the run is quick play without the trip through the menu,
    /// not a silent drop back to Standard — and the facility is a fresh one.
    #[test]
    fn a_new_run_rerolls_the_facility_at_the_same_options() {
        let played = LevelSeed::quick_play_at(8371, Difficulty::MuchHarder);
        let options = RunOptions {
            mode: RunMode::QuickPlay,
            difficulty: Difficulty::MuchHarder,
        };
        let next = EndExit::NewRun
            .level(&played, options, 4242)
            .expect("a new run starts a run");
        assert_eq!(next.seed, 4242, "a fresh seed");
        assert_ne!(next.seed, played.seed, "and not the one just played");
        assert_eq!(
            next,
            LevelSeed::quick_play_at(4242, Difficulty::MuchHarder),
            "the options the player chose, applied to the new seed",
        );

        // The same seed at a different setting is a different run, which is what
        // makes carrying the difficulty worth doing at all.
        assert_ne!(next, LevelSeed::quick_play_at(4242, Difficulty::Standard));
    }

    /// Back to menu starts nothing — it is the one exit with no run behind it.
    #[test]
    fn the_menu_exit_boots_no_level() {
        let played = LevelSeed::quick_play(1);
        assert_eq!(
            EndExit::Menu.level(&played, RunOptions::default(), 7),
            None,
            "the title screen rolls its own level when the player chooses one",
        );
    }
}
