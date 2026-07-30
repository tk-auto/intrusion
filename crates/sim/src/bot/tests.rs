use super::*;
use crate::test_support::boot;
use crate::{run_batch, run_one, RunOutcome, UsageHistogram, Verb, DEFAULT_INPUT_CAP};
use intrusion_core::{Event, Loadout, Outcome};

/// #276: the bot routes by **the core's rule**, never a table of its own.
///
/// It used to hold a private `matches!` allow-list — which meant a new
/// [`Terrain`] compiled silently as unroutable, and it had already fallen a
/// variant behind (§10.7 duct entries). The bot plans on the player's own
/// channels so its metrics describe *this* game (§13.2/§13.4); a second terrain
/// table is exactly how that quietly stops being true.
///
/// Swept over a whole generated facility, so it runs against the §10.3 table as
/// generation actually stamps it. Reintroducing a local allow-list here would
/// have to match the core's answer on every cell of a real level.
#[test]
fn the_bot_routes_by_the_cores_rule_not_its_own() {
    let (state, _) = boot(4242);
    let f = state.layout().facility();
    let mut seen: Vec<Terrain> = Vec::new();
    for y in 0..f.height() {
        for x in 0..f.width() {
            let cell = Cell::new(x, y);
            let t = f.terrain(cell).expect("every in-bounds cell has terrain");
            if !seen.contains(&t) {
                seen.push(t);
            }
            assert_eq!(
                routable(f, cell),
                t.routes_player(),
                "{t:?} at {cell:?}: the bot's routing must be the core's",
            );
        }
    }
    // The sweep only means something if it met the interesting kinds — a level of
    // nothing but floor and wall would pass vacuously.
    for t in [
        Terrain::Floor,
        Terrain::Wall,
        Terrain::DoorHinge,
        Terrain::DoorPanelClosed,
        Terrain::Hideout,
        Terrain::PartialCover,
        Terrain::Console,
        // The comms console (§7.7) is a *distinct* kind from the intel console, so
        // the bot's objective scan can never mistake one for the other — and its
        // routing must agree with the core's on it like any other solid usable.
        Terrain::CommsConsole,
        Terrain::Exit,
    ] {
        assert!(seen.contains(&t), "seed 4242 stamps no {t:?} to check");
    }
    // The wrapper's own contribution: off-grid is not routable, whatever the
    // terrain table says.
    assert!(
        !routable(f, Cell::new(9_999, 9_999)),
        "a cell outside the facility routes nowhere",
    );
}

/// §10.7, stated deliberately rather than left to silence: the bot **cannot**
/// route through a duct entry, even though the player can enter one.
///
/// Climbing in is a mode change into the crawlspace — movement confined to the
/// duct's recorded path, perception degraded — not a step a plain floor route can
/// take, and the bot has no crawl policy at all. Teaching it to use ducts is its
/// own piece of work; this test is what makes the current answer a decision
/// rather than the old allow-list's silence.
#[test]
fn the_bot_does_not_route_through_a_duct_entry() {
    // Sweep seeds until one generates a duct — not every level carries one.
    let entry = (0..40).find_map(|seed| {
        let (state, _) = boot(seed);
        let entry = state.layout().ducts().first()?.entries()[0];
        Some((state, entry))
    });
    let Some((state, entry)) = entry else {
        panic!("no seed in 0..40 generated a duct to check");
    };
    let f = state.layout().facility();
    assert_eq!(
        f.terrain(entry),
        Some(Terrain::DuctEntry),
        "an entry cell is stamped as one",
    );
    assert!(
        !routable(f, entry),
        "a duct is not a through-route for the bot (§10.7)",
    );
    assert!(
        !Terrain::DuctEntry.routes_player(),
        "and the core says the same, so the two cannot drift apart",
    );
}

/// §12.4: the same `(seed, profile)` under the bot produces byte-identical
/// rows, twice. The bot carries its own state (taken consoles, cover timers),
/// so this pins that none of it leaks non-determinism into the run — and it
/// sweeps **every** shipped profile (#198), not just the default one, since a
/// temperament is only a regression instrument if it reproduces.
#[test]
fn the_bot_is_deterministic_per_seed_and_profile() {
    for profile in Profile::ALL {
        for seed in [0, 7, 200] {
            let play =
                || run_one(seed, &mut StealthBot::with_profile(profile), 300).expect("generates");
            let (a, b) = (play(), play());
            assert_eq!(a, b, "{} seed {seed}: a bot run reproduces", profile.name);
            assert_eq!(
                a.to_json_line(),
                b.to_json_line(),
                "{} seed {seed}: identical bytes",
                profile.name,
            );
            assert_eq!(
                a.profile,
                Some(profile.name),
                "seed {seed}: the row names the temperament that played it",
            );
        }
    }
}

/// #198's behaviour-preservation clause: the [`Profile::BALANCED`] row of
/// numbers **is** the constants the bot carried before the seam existed, so
/// every metric captured under it stays comparable with the batches that came
/// before. Asserted as byte-identical rows between the default bot and one
/// explicitly given the balanced profile, over a spread of seeds — the numbers
/// are pinned by the profile literal, and this pins that the policy actually
/// reads them rather than a stray leftover constant.
#[test]
fn the_balanced_profile_is_the_default_bot() {
    assert_eq!(StealthBot::new().profile(), Profile::BALANCED);
    for seed in 30..40 {
        let default = run_one(seed, &mut StealthBot::new(), 300).expect("generates");
        let explicit = run_one(seed, &mut StealthBot::with_profile(Profile::BALANCED), 300)
            .expect("generates");
        assert_eq!(
            default.to_json_line(),
            explicit.to_json_line(),
            "seed {seed}: the balanced profile must reproduce today's bot",
        );
    }
}

/// **#346's behaviour-preservation clause, pinned rather than eyeballed.**
///
/// Lifting Run and Camouflage out of hard-coded `match` arms and into cues
/// ([`crate::cue`]) is worth nothing if it moved the bot: the whole reason the
/// seam lands behaviour-preserving is so that the *interesting* diffs — the
/// verbs the bot has never pressed (#347) — arrive one at a time with their own
/// measurable delta. A seam that quietly retuned the two cues it replaced would
/// make every one of those deltas unattributable.
///
/// So this pins the runs themselves, per profile: the ending, its turn count,
/// and the **exact sequence of ability activations** the bot issued, spelled in
/// the replay script's letters (§12.4). Nothing else in the suite covers this —
/// the batch under the sim's bare loadout never presses Camouflage at all (it is
/// not held), so the cloak cue's rewrite would have gone unmeasured. The loadout
/// here grants it, and grants **Decoy** alongside it: an ability with no cue yet
/// must be inert, not merely unused, and holding one is how that is asserted.
///
/// The numbers are `[START]` in the sense that any deliberate change to the bot
/// moves them — that is what makes them useful. Update them *with* the change
/// and say what moved, never to make a red test green.
///
/// **#442 moved 8 of the 48 rows, 5 changing outcome — every one of them a loss
/// becoming a win, and the pin's wins go 20 → 25.** Adopting the flank rule
/// (§6.1/§6.2/§7.2) makes a *calm* guard blind at its sides, so runs that used to end
/// against a patrol the bot walked past now survive it: `cautious 7` and `cautious 10`
/// convert long losses into longer wins, `aggressive 2`/`8` and `careless 2` likewise.
/// A one-directional move of this size is what adoption is *supposed* to look like —
/// the rule only ever removes detections, and only from patrols — and it is the twelve
/// seeds agreeing with appendix 28's 150-seed conditional table rather than a new
/// finding. Read the deltas against the refreshed baseline in the same PR, not here:
/// twelve seeds are a pin, not a balance signal (§13.4).
///
/// **#430 moved 12 of the 48 rows, 3 changing outcome.** A guard's first alert no
/// longer moves it (§4.2): it finishes the turn it had planned and turns for the
/// player from the next decision, so every run in which a Calm guard freshly
/// spotted the bot diverges from that turn on. The mix drifts up one win
/// (19 → 20), with the movement in both directions: `cautious 1` converts a loss
/// into a win, `careless 10` — the pin's only run at the input cap — resolves
/// into a 346-turn win, and `cautious 7` loses late a run it used to win late,
/// because the deferred first step re-times every patrol the bot meets
/// afterwards. One beat of tempo cuts both ways. Twelve seeds are a pin, not a
/// balance signal (§13.4); the 100-seed baseline comparison recorded in the PR is
/// what judges the rule against §8.3's "being seen must stay expensive".
///
/// **A change to *generation* moves them too**, and #361 did: a cupboard now
/// needs solid back diagonals, so these twelve seeds build different facilities
/// and the bot's identical policy meets different levels in them. Rows were
/// regenerated there — the cue seam itself is untouched, which is why the
/// *shape* of the batch (endings mixed, the cloak pressed) is what carries the
/// assertion when the levels underneath it move.
///
/// **#383 opened the run with the wait's look, and moved 17 of the 48 rows, 5 of
/// them changing outcome.** The first frame is now computed as if the previous turn
/// had been a Wait (§5/§8.3/§9.1) — 360° sight and the widened guard sense — so the
/// bot's very first decision is taken against a different picture of the entry room
/// and every run diverges from there. It is not a policy change: the same cues read
/// a perception the game hands them one turn earlier. The **outcome mix barely
/// moves** (19 wins before and after; one loss more), and `balanced 2`'s stall
/// (`playing 1000`) resolves into a 47-turn loss, leaving `careless 10` as the pin's
/// only run at the cap. Twelve seeds are a pin, not a balance signal (§13.4); the
/// 100-seed baseline refreshed in the same PR is what judges the change.
///
/// **#347 moved every profile**, and that is the ticket landing rather than a
/// regression: the batch grants Decoy, so writing its cue is *supposed* to show
/// up here as `d` presses and the runs they change. Read the diff as the cue's
/// first evidence — `balanced 3` was the batch's lone stall (`playing 1000`) and
/// now finishes, while several runs that won now lose. Neither is a verdict:
/// twelve seeds are a pin, not a balance signal (§13.4), and the measurement that
/// carries the ticket is the 100-seed with/without batch recorded in
/// `docs/stats/abilities/decoy.md`. Its rows were regenerated *on top of* #379's
/// crouch below, so the batch carries both changes at once.
///
/// **#379 moved exactly one row of the forty-eight** — `cautious 5`, from
/// `won 217 cr` to `won 343 crr` — and that narrowness is the finding rather than
/// luck. The crouch (§10.3) is the one behaviour here that *every* profile carries,
/// so unlike #316 there is no reach of zero making the avoidance-first blocks
/// unchanged by construction; they are unchanged because the pose is rare and
/// cheap. On seed 5 the cautious bot ducks behind a bench instead of pressing on,
/// the patrol passes differently, and it wins later by a longer route. Note the
/// crouch spells no letter either: ducking is a bump (§4.3), like the takedown and
/// the grab, so it shows in the turn counts and not in the script.
///
/// **#311 moved seven rows of the forty-eight**, and every one of them is the
/// facility alert ladder (§7.3) doing what it was built to do: from rung 1 a Calm
/// guard's patrol dwell drops from 3–7 turns to 1–3, so on any seed where the bot
/// was seen or left a post silent, the patrols it walks past are on a different
/// rhythm from that turn on. The bot's policy is untouched — it does not read the
/// rung at all (#198) — so these are the same decisions meeting a facility that
/// reacts. Net one win fewer (28 → 27), with the movement in both directions:
/// `balanced 11` turns a 648-turn loss into a 253-turn win, while `cautious 11`
/// and `aggressive 9` lose runs they used to win. Twelve seeds are a pin, not a
/// balance signal (§13.4); the 100-seed batch is what judges the ladder.
///
/// **#316 moved the striking half and left the rest alone**, which is the whole
/// point of putting the takedown behind [`Profile::takedown_reach`]. The
/// `balanced` and `cautious` blocks below are **byte-for-byte what they were
/// before that ticket** — the strongest form of its "the avoidance-first
/// temperaments are unchanged" criterion, since a profile with a reach of zero declines the verb
/// and never reaches a line of the new code. `aggressive` moved because it now
/// takes the strikes it walks past, and `careless` is new. Note the script letters
/// spell *activations* only: a takedown, a grab and a stow are steps (§7.2/§8.3),
/// so they leave no letter here and are asserted in
/// [`the_striking_profiles_work_the_body_chain`] instead.
///
/// The pin has moved three times since, and for reasons worth naming. First, rungs
/// 2 and 3 began **walking guards into the facility** (§7.3/#374). Then #398 took
/// the spawn-cell anchor off patrol territory, moving two rows.
///
/// **#387** moved **all 48 rows**, which is the expected shape rather than a signal:
/// the throat rule changes where tables and repair pillars may be stamped, so every
/// seed's *geometry* is different and the bot is simply playing 48 different levels.
/// Nothing here is comparable row to row.
///
/// The aggregate did move — **25 wins to 20**, with the first `playing` row since
/// #401 — and that is precisely the kind of number twelve seeds per temperament
/// cannot settle. Read it against the committed baseline in the same PR, which runs
/// 100 seeds per profile and moved far less; the doc note on #401 above is the
/// standing warning that these two can disagree even in *sign*. A 48-run pin is a
/// change-detector, not a balance signal (§13.4).
///
/// Then #401 clipped the §7.6 post-search watch to each guard's own territory
/// instead of replacing it, and **12 of 48 rows moved, 5 changing outcome** — a net
/// three wins lost *here*. Read that against the committed baseline rather than on
/// its own: over 100 seeds the same change moves win rate the other way (balanced
/// 0.34 → 0.38, careless 0.30 → 0.35), because two responders splitting one area
/// into halves watch it *less* densely than two pacing all of it. Twelve seeds are
/// a pin, not a balance signal (§13.4), and this is the sharpest illustration of
/// that in the suite: the two disagree on the sign. `careless 10` joins
/// `balanced 2` in reaching the input cap still playing.
///
/// Before that, #399 moved nearly all of them, and that is the point rather than a
/// problem:
/// the guards stopped covering part of the level and started **partitioning all of
/// it** (§7.5). There is no longer a wing with nobody on it, so a route the bot used
/// to take unseen now meets a patrol, and 12 wins become losses while 5 losses
/// become wins. One run (`balanced 2`) now reaches the input cap still playing —
/// the first `playing` row this pin has ever carried, and worth watching rather
/// than waving through: the 100-seed batch puts timeouts at 4 in 100 against 3
/// before, so it is a tail, not a trend.
///
/// This list is the bot's play against a fixed game, so a change to the *game*
/// moves it exactly as a change to the cue seam would — which is why the refresh
/// belongs in the PR that changed the game, with the deltas read rather than waved
/// through. What it must never do is move on a refactor that was supposed to change
/// nothing.
#[test]
fn the_cue_seam_reproduces_the_hardcoded_bots_runs() {
    const PINNED: [&str; 48] = [
        "balanced 0 playing 1000 rdrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr",
        "balanced 1 lost 49 rrc",
        "balanced 2 won 160 r",
        "balanced 3 won 201 cdrrrd",
        "balanced 4 won 67 ",
        "balanced 5 lost 118 rcrcrdr",
        "balanced 6 lost 147 rrr",
        "balanced 7 lost 11 rc",
        "balanced 8 lost 113 r",
        "balanced 9 won 349 c",
        "balanced 10 won 249 ",
        "balanced 11 lost 199 crrr",
        "cautious 0 won 150 dr",
        "cautious 1 lost 112 rr",
        "cautious 2 lost 313 rcdrdr",
        "cautious 3 won 376 rdr",
        "cautious 4 won 73 c",
        "cautious 5 lost 73 crcrcr",
        "cautious 6 lost 140 rrr",
        "cautious 7 lost 11 rc",
        "cautious 8 won 296 rd",
        "cautious 9 lost 285 rdrdr",
        "cautious 10 lost 795 dcrrrdrdrrdrdrr",
        "cautious 11 lost 224 rrrdrc",
        "aggressive 0 won 124 rdr",
        "aggressive 1 lost 40 r",
        "aggressive 2 lost 110 r",
        "aggressive 3 lost 62 rdcr",
        "aggressive 4 lost 172 rrr",
        "aggressive 5 won 74 rcrc",
        "aggressive 6 won 75 c",
        "aggressive 7 lost 11 rc",
        "aggressive 8 lost 142 rr",
        "aggressive 9 won 355 crd",
        "aggressive 10 won 247 ",
        "aggressive 11 won 166 r",
        "careless 0 lost 130 rdrr",
        "careless 1 lost 40 r",
        "careless 2 lost 110 r",
        "careless 3 lost 62 rdcr",
        "careless 4 lost 162 rdcrcr",
        "careless 5 won 70 rcr",
        "careless 6 won 75 c",
        "careless 7 lost 11 rc",
        "careless 8 won 200 rdc",
        "careless 9 lost 263 cr",
        "careless 10 won 247 ",
        "careless 11 won 138 rc",
    ];

    let mut played = Vec::new();
    // The activation letters alone, kept apart from the formatted rows so the
    // cloak check below counts presses and not the letters of a profile's name.
    let mut activations: Vec<String> = Vec::new();
    for profile in Profile::ALL {
        for seed in 0..12 {
            let (state, _) = boot(seed);
            let mut state = state.with_loadout(
                intrusion_core::Loadout::innate()
                    .with(AbilityId::Camouflage)
                    .with(AbilityId::Decoy),
            );
            let mut bot = StealthBot::with_profile(profile);
            let mut pressed = String::new();
            for _ in 0..DEFAULT_INPUT_CAP {
                if state.outcome() != Outcome::Playing {
                    break;
                }
                let input = bot.decide(&state);
                if let Input::Activate(id) = input {
                    pressed.push(id.script_letter());
                }
                state.step(input);
            }
            let ending = match state.outcome() {
                Outcome::Playing => "playing",
                Outcome::Won => "won",
                Outcome::Lost => "lost",
            };
            played.push(format!(
                "{} {seed} {ending} {} {pressed}",
                profile.name,
                state.turn(),
            ));
            activations.push(pressed);
        }
    }
    assert_eq!(
        played, PINNED,
        "the cue seam changed how the bot plays — see this test's doc comment",
    );

    // The pin only means something if the cloak cue is actually exercised by it:
    // a batch that never presses Camouflage would pin the Run cue alone and call
    // the rewrite proven.
    let cloak = AbilityId::Camouflage.script_letter();
    let cloaked = activations
        .iter()
        .filter(|pressed| pressed.contains(cloak))
        .count();
    assert!(
        cloaked >= 3,
        "only {cloaked} pinned runs press the cloak — this batch would not \
             catch a change to its cue",
    );

    // Same demand of the decoy (#347): the loadout grants it, so a pin where
    // nobody presses it would be pinning the fake's *absence* and calling the cue
    // covered.
    let fake = AbilityId::Decoy.script_letter();
    let decoyed = activations
        .iter()
        .filter(|pressed| pressed.contains(fake))
        .count();
    assert!(
        decoyed >= 3,
        "only {decoyed} pinned runs press the decoy — this batch would not \
             catch a change to its cue",
    );
}

/// **Every decoy the bot presses is pressed at a guard that has lost it** — the
/// §8.3 rule *"draws Investigating, not Chasing"*, which #347 names as a cue bug
/// rather than a tuning question, checked over real play instead of a fixture.
///
/// Two halves, because the rule has two: nobody's cone may be live on the player
/// at the moment of the press (a guard that has you is coming to the real
/// intruder, and the fake beside you competes with the genuine article), and
/// somebody must actually be searching, or the fake is bought for a facility that
/// is not looking for anybody.
#[test]
fn every_decoy_the_bot_drops_is_dropped_at_a_search() {
    let mut dropped = 0;
    for seed in 0..40 {
        let (state, _) = boot(seed);
        let mut state = state.with_loadout(Loadout::innate().with(AbilityId::Decoy));
        let mut bot = StealthBot::with_profile(Profile::BALANCED);
        for _ in 0..DEFAULT_INPUT_CAP {
            if state.outcome() != Outcome::Playing {
                break;
            }
            let input = bot.decide(&state);
            if input == Input::Activate(AbilityId::Decoy) {
                assert!(
                    !state.guards().iter().any(|g| state.guard_detects_now(g)),
                    "seed {seed}: dropped a decoy while a guard had the player — \
                         a decoy draws Investigating, never Chasing (§8.3)",
                );
                assert!(
                    state
                        .guards()
                        .iter()
                        .any(|g| state.perceive_guard(g).is_some()
                            && matches!(
                                g.state(),
                                GuardState::Alerted
                                    | GuardState::Investigating
                                    | GuardState::Responding
                            )),
                    "seed {seed}: dropped a decoy with nobody searching — there \
                         was no hunt to redirect (§8.3)",
                );
                dropped += 1;
            }
            state.step(input);
        }
    }
    assert!(
        dropped > 0,
        "no decoy in 40 seeds — this test would prove nothing",
    );
}

/// **Every autodoors press is a press with a door on the way out** — §8.3's *"a
/// door in your path… shuts behind you"*, which is the whole flight tool (§7.6).
/// A press on open floor would spend the turn and a 40-turn cooldown on a window
/// that closes nothing, so the cue's job is exactly this precondition.
#[test]
fn every_autodoors_press_has_a_door_on_the_route() {
    let mut pressed = 0;
    for seed in 0..40 {
        let (state, _) = boot(seed);
        let mut state = state.with_loadout(Loadout::innate().with(AbilityId::Autodoors));
        let mut bot = StealthBot::with_profile(Profile::BALANCED);
        for _ in 0..DEFAULT_INPUT_CAP {
            if state.outcome() != Outcome::Playing {
                break;
            }
            let input = bot.decide(&state);
            if input == Input::Activate(AbilityId::Autodoors) {
                // The cue bids off the step the *plan* would take, which cannot be
                // read back off the state — so assert the fact that makes the
                // press worth its turn: a door is adjacent to be walked through.
                let doors = Direction::ALL
                    .iter()
                    .filter_map(|&dir| state.player().step(dir))
                    .filter(|&cell| {
                        matches!(
                            state.layout().facility().terrain(cell),
                            Some(Terrain::DoorPanelClosed | Terrain::DoorPanelOpen)
                        )
                    })
                    .count();
                assert!(
                    doors > 0,
                    "seed {seed}: opened the autodoors with no door to walk \
                         through — the window would shut nothing (§8.3)",
                );
                pressed += 1;
            }
            state.step(input);
        }
    }
    assert!(
        pressed > 0,
        "no autodoors in 40 seeds — this test would prove nothing",
    );
}

/// **Every confusion is fired in a panic, at somebody it actually catches** —
/// §8.3's *"a costed panic-buy of time, not a kill"*. Two facts per press: the bot
/// was being hunted, and at least one guard stood inside the clamped blast.
///
/// The second is core's own precondition (a firing that catches nobody is
/// `Unusable`, §4.4's free no-op), asserted here anyway — it is the difference
/// between a cue that reads the blast and one that presses hopefully and lets the
/// refusal absorb it.
#[test]
fn every_confusion_is_fired_at_a_guard_it_catches() {
    let mut fired = 0;
    for seed in 0..40 {
        let (state, _) = boot(seed);
        let mut state = state.with_loadout(Loadout::innate().with(AbilityId::Confusion));
        let mut bot = StealthBot::with_profile(Profile::BALANCED);
        for _ in 0..DEFAULT_INPUT_CAP {
            if state.outcome() != Outcome::Playing {
                break;
            }
            let danger = danger_cells(&state);
            let hunted = being_hunted(&state, &danger);
            let input = bot.decide(&state);
            if input == Input::Activate(AbilityId::Confusion) {
                assert!(
                    hunted,
                    "seed {seed}: fired confusion without being hunted — the \
                         longest cooldown in the catalog is a panic-buy (§8.3)",
                );
                let blast = state.confusion_blast();
                assert!(
                    state.guards().iter().any(|g| blast.contains(g.pos())),
                    "seed {seed}: fired confusion at nobody — the blast catches \
                         no guard and the press is a free no-op (§8.3/§4.4)",
                );
                fired += 1;
            }
            state.step(input);
        }
    }
    assert!(
        fired > 0,
        "no confusion in 40 seeds — this test would prove nothing",
    );
}

/// **The bot never gets caught in a wall it phased into** — the risk #347 names
/// as Dephase's own: a duration that expires inside a solid costs a safety eject
/// plus a stun as long as the throw was deep (§8.3), and a bot that phases
/// casually will find it.
///
/// The eject is the *only* thing in the game that stuns the player
/// (`phase_eject_stun`, core's `state/abilities.rs`), which makes `stunned() == 0`
/// over a batch an exact statement that it never fired — a much sharper assertion
/// than counting crossings. Two policies hold it up together: the cue only wants a
/// **one-cell** crossing (in at turn 1, out at 2, of a 3-turn duration), and
/// leaving the wall outranks every other plan while the bot is in one.
#[test]
fn the_bot_is_never_ejected_from_a_wall_it_phased_into() {
    let mut crossings = 0;
    for seed in 0..40 {
        for profile in Profile::ALL {
            let (state, _) = boot(seed);
            let mut state = state.with_loadout(Loadout::innate().with(AbilityId::Dephase));
            let mut bot = StealthBot::with_profile(profile);
            for _ in 0..DEFAULT_INPUT_CAP {
                if state.outcome() != Outcome::Playing {
                    break;
                }
                let input = bot.decide(&state);
                if input == Input::Activate(AbilityId::Dephase) {
                    crossings += 1;
                }
                state.step(input);
                assert_eq!(
                    state.stunned(),
                    0,
                    "seed {seed} ({}): stunned, so a phase expired inside a solid \
                         — the safety eject is the one thing Dephase's cue must never \
                         walk into (§8.3)",
                    profile.name,
                );
            }
        }
    }
    assert!(
        crossings > 0,
        "no phase in 40 seeds × 4 profiles — this test would prove nothing",
    );
}

/// **Every bore opens a route, never a pocket, and never as an escape** — the
/// discipline #347 asks of Pierce Wall's cue, since a budget of three spent on
/// the first legal wall *"makes the histogram look healthy while measuring
/// nothing"*.
///
/// Two checks per press, both re-derived from the state rather than taken from
/// the cue's word for it: the bot was not being hunted (a hole conceals nothing,
/// §8.3), and the cell beyond the bored wall is floor the bot has already seen —
/// which is what makes it a way *through* rather than the one-cell pocket a
/// two-thick wall would open.
#[test]
fn every_bore_opens_a_route_the_bot_has_seen() {
    let mut bored = 0;
    for seed in 0..40 {
        for profile in Profile::ALL {
            let (state, _) = boot(seed);
            let mut state = state.with_loadout(Loadout::innate().with(AbilityId::PierceWall));
            let mut bot = StealthBot::with_profile(profile);
            for _ in 0..DEFAULT_INPUT_CAP {
                if state.outcome() != Outcome::Playing {
                    break;
                }
                let danger = danger_cells(&state);
                let hunted = being_hunted(&state, &danger);
                let input = bot.decide(&state);
                if input == Input::Activate(AbilityId::PierceWall) {
                    assert!(
                        !hunted,
                        "seed {seed} ({}): bored a wall while hunted — a hole is \
                             not a cupboard and conceals nothing (§8.3)",
                        profile.name,
                    );
                    let target = state.bore_target().expect("the press must be legal");
                    let dir = Direction::ALL
                        .iter()
                        .find(|&&d| state.player().step(d) == Some(target))
                        .expect("the bore target is one of the four neighbours");
                    let beyond = target.step(*dir).expect("a bore is never the shell");
                    assert!(
                        state.memory().contains(beyond)
                            && state
                                .layout()
                                .facility()
                                .terrain(beyond)
                                .is_some_and(|t| !t.blocks_movement()),
                        "seed {seed} ({}): bored into {beyond:?}, which is not \
                             seen floor — that is a pocket, not a route (§8.3)",
                        profile.name,
                    );
                    bored += 1;
                }
                state.step(input);
            }
        }
    }
    assert!(
        bored > 0,
        "no bore in 40 seeds × 4 profiles — this test would prove nothing",
    );
}

/// **Every lockdown seals something, and never the door the bot is about to walk
/// through** — §8.3's own warning that *"a lockdown fired across a route you still
/// have to travel is a real mistake"*, since your own lock is never refused but
/// bumping it open costs the turn and leaves the door standing open.
#[test]
fn every_lockdown_seals_doors_that_are_not_on_the_way_out() {
    let mut sealed = 0;
    for seed in 0..40 {
        let (state, _) = boot(seed);
        let mut state = state.with_loadout(Loadout::innate().with(AbilityId::Lockdown));
        let mut bot = StealthBot::with_profile(Profile::BALANCED);
        for _ in 0..DEFAULT_INPUT_CAP {
            if state.outcome() != Outcome::Playing {
                break;
            }
            let input = bot.decide(&state);
            if input == Input::Activate(AbilityId::Lockdown) {
                assert!(
                    !state.lockdown_doors().is_empty(),
                    "seed {seed}: fired a lockdown with no door in reach — a \
                         window bought to seal nothing (§8.3)",
                );
                // The cue bids off the step the *plan* would take, which cannot be
                // read back off the state. What can: the bot is not standing in a
                // doorway it is mid-way through, which is the same mistake in its
                // sharpest form.
                assert!(
                    !matches!(
                        state.layout().facility().terrain(state.player()),
                        Some(Terrain::DoorPanelOpen)
                    ),
                    "seed {seed}: sealed the doors while standing in a doorway — \
                         that is a route the bot still has to travel (§8.3)",
                );
                sealed += 1;
            }
            state.step(input);
        }
    }
    assert!(
        sealed > 0,
        "no lockdown in 40 seeds — this test would prove nothing",
    );
}

/// The profiles are **distinguishable** over a batch — a shape assertion, never
/// a leaderboard (§13.4). Three directions the temperaments are built to differ
/// in, checked over the same seeds so the facility is held fixed:
///
/// - **`cautious` is seen less often, per turn it plays.** Rate, not raw count:
///   waiting a sweep out costs turns, so the careful temperament racks up a
///   longer run and would lose a raw-total comparison it is actually winning.
///   `aggressive` routes with `hold_watched: false` — it walks a watched cell
///   rather than waiting — and gives a third of the berth, so being seen more
///   often is the *cost* of its temperament, not a verdict on it.
/// - **`cautious` spends a bigger share of its turns waiting.** The direct
///   signature of "ducks into cover early and waits long" against "hides late
///   and briefly".
/// - **The two play differently at all** — their usage histograms are not the
///   same numbers. Identical histograms would mean the profile seam changed
///   nothing, and the batch would be measuring one bot twice.
///
/// Deliberately loose and direction-only, in the spirit of the mixed-outcome
/// test: never "cautious wins more" (§13.4 — a profile is a temperament, not a
/// better player, and on this batch the win rates are close enough that either
/// could lead).
#[test]
fn the_profiles_play_the_same_seeds_differently() {
    let seeds = 30..70;
    let batch = |profile: Profile| {
        crate::Summary::of(
            &run_batch(seeds.clone(), DEFAULT_INPUT_CAP, move |_| {
                StealthBot::with_profile(profile)
            })
            .expect("generates"),
        )
    };
    let cautious = batch(Profile::CAUTIOUS);
    let aggressive = batch(Profile::AGGRESSIVE);

    let seen_per_turn = |s: &crate::Summary| s.detections as f64 / s.total_turns as f64;
    assert!(
        seen_per_turn(&cautious) <= seen_per_turn(&aggressive),
        "the cautious profile was seen more often per turn than the aggressive \
             one ({:.4} vs {:.4}) — the temperaments are not what they claim",
        seen_per_turn(&cautious),
        seen_per_turn(&aggressive),
    );

    let waiting = |s: &crate::Summary| f64::from(s.usage.count(Verb::Wait)) / s.total_turns as f64;
    assert!(
        waiting(&cautious) > waiting(&aggressive),
        "the cautious profile did not wait more than the aggressive one \
             ({:.4} vs {:.4}) — cover patience is its whole signature",
        waiting(&cautious),
        waiting(&aggressive),
    );

    assert_ne!(
        cautious.usage, aggressive.usage,
        "the two temperaments spent their turns identically — the profile \
             seam is not reaching the policy",
    );
}

/// **#316: the §13.2 takedown and bodies rows have a live source.**
///
/// Both rows read a flat zero on every batch ever captured, so nothing in the
/// harness exercised §7.2 takedowns, §8.3 dragging, §10.3 stowing or §7.3's radio
/// clock — the most-churned code in the repo, and a regression anywhere in it
/// would have moved no metric at all. This is the test that stops that being true
/// again, and it asserts the split the temperaments were built around:
///
/// - the **declining** profiles land exactly zero, which is `takedown_reach: 0`
///   working rather than an opportunity that never came (§13.3 — a cautious bot
///   reporting no takedowns is correct behaviour, not a defect);
/// - the **striking** ones land some, from the core's own gate and no other —
///   the bot never re-implements a precondition, it bumps and the game rules;
/// - `aggressive` **grabs** bodies, so the drag half of the chain runs, and
///   **stows** some of them, so §10.3's deposit-and-lock runs too;
/// - `careless` gets bodies **found**, which is the first exercise §7.3's clock
///   has ever had. That is why it exists: a stowed body is beyond every cone, so
///   the tidier the temperament the flatter this row reads.
///
/// Since **#381** the stow has its own §13.2 slot, so every one of these is read
/// straight off the histogram — the hand-rolled `BodyStored` counter this test used
/// to carry was the tell that the metric was missing.
///
/// Loose and direction-only (§13.4), like every other shape assertion here: the
/// counts are free to move, the zero-versus-nonzero split is not.
#[test]
fn the_striking_profiles_work_the_body_chain() {
    let batch = |profile: Profile| {
        let records = run_batch(0..60, DEFAULT_INPUT_CAP, move |_| {
            StealthBot::with_profile(profile)
        })
        .expect("generates");
        let usage = records
            .iter()
            .fold(UsageHistogram::new(), |acc, r| acc.merged(&r.usage));
        let takedowns: u32 = records.iter().map(|r| r.takedowns).sum();
        let found: u32 = records.iter().map(|r| r.bodies_found).sum();
        (takedowns, found, usage)
    };

    for profile in [Profile::BALANCED, Profile::CAUTIOUS] {
        let (takedowns, found, usage) = batch(profile);
        assert_eq!(
            (
                takedowns,
                found,
                usage.count(Verb::Takedown),
                usage.count(Verb::Drag),
                usage.count(Verb::Stow)
            ),
            (0, 0, 0, 0, 0),
            "{}: a profile that declines the verb must land none of it",
            profile.name,
        );
    }

    let (strikes, _, aggressive) = batch(Profile::AGGRESSIVE);
    assert!(
        strikes > 0,
        "aggressive landed no takedown over 60 seeds — the §13.2 row is dead again",
    );
    assert_eq!(
        aggressive.count(Verb::Takedown),
        strikes,
        "every takedown must reach the histogram as well as the metric",
    );
    assert!(
        aggressive.count(Verb::Drag) > 0,
        "aggressive never took hold of a body — §8.3's drag is still unexercised",
    );

    let (strikes, found, careless) = batch(Profile::CARELESS);
    assert!(strikes > 0, "careless landed no takedown over 60 seeds");
    assert!(
        found > 0,
        "no body careless left on the floor was ever found — §7.3's clock still \
             has nothing to react to",
    );

    // The temperaments' actual split is **stowing**, not grabbing. Taking hold is
    // not a decision — stepping off a body's cell grabs it whether you meant to or
    // not (§8.3/#187) — so `careless` racks up grabs it immediately undoes, and a
    // `Drag` count says nothing about temperament on its own. Putting a body
    // *away* is the decision, and since #381 it has its own §13.2 slot, so the
    // split is **read off the histogram** rather than replayed and counted by hand.
    assert!(
        aggressive.count(Verb::Stow) > 0,
        "aggressive never stowed a body — §10.3's deposit-and-lock is unexercised",
    );
    assert_eq!(
        careless.count(Verb::Stow),
        0,
        "careless stowed a body — a reach of zero must mean it never tidies up, \
             or `bodies_found` loses the source this profile exists to be",
    );
}

/// **#379: §10.3's partial cover has a live source, on every temperament.**
///
/// Before this, `crouched_behind` was `None` for every turn of every run under
/// every profile — so §10.1a's benches and the exact run geometry `cover.rs`
/// computes for them were **inert in the harness**: a regression in
/// core's `cover::run_conceals` or `cover::run_hugs` would have moved no metric at
/// all. This is the test that stops that being true again, and it asserts three
/// things a false zero could not fake:
///
/// - every profile that **takes cover at all** ducks, because from your own cell
///   the crouch is a reflex rather than an appetite (see [`StealthBot::crouch`]) —
///   so a zero on one of those is a broken policy, not a temperament;
/// - and `careless`, which declines concealment outright
///   ([`Profile::crouches`]), lands exactly none — the decline working rather than
///   an opportunity that never came, exactly as its `takedowns: 0` siblings do;
/// - the **histogram** sees each duck, so the §13.2 row and the events agree;
/// - the pose is entered from where the bot stands and **not left immediately**:
///   crouched turns outnumber the ducks that started them, which is the "spent a
///   turn for nothing" failure the ticket names.
///
/// Direction-only and loose (§13.4) — the counts are free to move, the
/// zero-versus-nonzero is not.
#[test]
fn every_profile_ducks_behind_a_bench() {
    let mut histogram = 0u32;
    for profile in Profile::ALL {
        // **The sweep searches and stops; it does not pin a window (#442/#387).**
        // Adopting the flank rule gave the bot a better answer than ducking when a
        // *calm* guard is close: walk to its blind side and take it. `aggressive`,
        // which keeps moving and strikes what it walks past, reaches for the crouch
        // vanishingly rarely — a handful of ducks in hundreds of seeds, against far
        // more for `balanced` and `cautious`. That is a temperament finding a
        // different tool, not §10.3 going inert.
        //
        // It used to be spelled as a hand-picked window (`100..170`, chosen because a
        // duck happened to land in it). That made a *generation* change — which moves
        // every seed's geometry — look like a §10.3 regression, which is exactly what
        // #387 hit: the window emptied while the behaviour was intact. So the range is
        // generous and the loop **stops at the first seed that satisfies the
        // assertions**: fast in the ordinary case, and it still fails honestly if the
        // crouch really does go inert.
        //
        // A temperament that declines the pose proves the opposite — that it *never*
        // crouches — and a search cannot exit early on a negative, so it keeps a short
        // fixed sweep rather than paying for the long one.
        let seeds = if profile.crouches { 0..400 } else { 0..60 };
        let (mut ducks, mut crouched_turns) = (0u32, 0u32);
        for seed in seeds {
            // Everything this profile has to prove is proved — stop paying for more.
            if ducks > 0 && crouched_turns > ducks {
                break;
            }
            let (mut state, _) = boot(seed);
            let mut bot = StealthBot::with_profile(profile);
            for _ in 0..DEFAULT_INPUT_CAP {
                if state.outcome() != Outcome::Playing {
                    break;
                }
                let before = state.turn();
                ducks += state
                    .step(bot.decide(&state))
                    .iter()
                    .filter(|e| matches!(e, intrusion_core::Event::Crouched { .. }))
                    .count() as u32;
                if state.turn() > before && state.crouched() {
                    crouched_turns += 1;
                }
            }
        }
        if !profile.crouches {
            assert_eq!(
                (ducks, crouched_turns),
                (0, 0),
                "{}: a temperament that declines concealment must never crouch",
                profile.name,
            );
            continue;
        }
        assert!(
            ducks > 0,
            "{}: never ducked behind a bench over its seed sweep — §10.3 is inert again",
            profile.name,
        );
        assert!(
            crouched_turns > ducks,
            "{}: {crouched_turns} crouched turns from {ducks} ducks — the pose is \
                 being dropped the turn after it is taken, so the turn bought nothing",
            profile.name,
        );
        // The same policy through the harness, so the §13.2 row cannot drift from
        // the events the policy actually produced. Summed across the crouching
        // temperaments rather than asserted per profile: `aggressive` only ducks
        // with a patrol already on top of it (`threat_radius` 3), which over the
        // sim preset is about **two crouches in 200 seeds** — a 60-seed batch
        // measures that rarity rather than the seam, and #383's opening look was
        // enough to take its count here from one to none. What the row must never
        // be is structurally empty.
        let records = run_batch(0..60, DEFAULT_INPUT_CAP, move |_| {
            StealthBot::with_profile(profile)
        })
        .expect("generates");
        histogram += records
            .iter()
            .fold(UsageHistogram::new(), |acc, r| acc.merged(&r.usage))
            .count(Verb::Crouch);
    }
    assert!(
        histogram > 0,
        "no crouch reached the usage histogram under any temperament — the §13.2 \
             row has no live source",
    );
}

/// The bench geometry is **genuinely entered**, not merely brushed past (#379).
///
/// The acceptance criterion in its own words: a test that fails if
/// `cover::run_conceals` regressed. It asserts the payoff rather than the call —
/// over a batch there are crouched turns where the player stands **inside a
/// guard's live cone** and is concealed from it anyway, which is exactly what the
/// run geometry is for and what nothing in the harness could previously produce.
/// A `run_conceals` that returned `false` would leave the bot standing up (and the
/// count at zero); one that returned `true` everywhere would break the core's own
/// directional tests next door.
///
/// Read off `cautious`, which crouches most: it holds out for a cupboard, so the
/// bench is what it settles for when a patrol arrives and none is near.
#[test]
fn the_bots_crouch_beats_a_live_cone() {
    let mut beat_a_cone = 0;
    for seed in 0..60 {
        let (mut state, _) = boot(seed);
        let mut bot = StealthBot::with_profile(Profile::CAUTIOUS);
        for _ in 0..DEFAULT_INPUT_CAP {
            if state.outcome() != Outcome::Playing {
                break;
            }
            state.step(bot.decide(&state));
            if state.crouched() {
                beat_a_cone += state
                    .guards()
                    .iter()
                    .filter(|g| g.fov().contains(state.player()) && state.concealed_from(g.pos()))
                    .count()
                    .min(1);
            }
        }
    }
    assert!(
        beat_a_cone > 0,
        "no crouched turn ever had a cone on the player it was concealed from — \
             the bench's run geometry is not being exercised",
    );
}

/// The **crouch-walk** (§10.3/#379): the bot shuffles along the bench rather than
/// standing up when a patrol comes round to its side of the furniture, and the
/// pose survives the step.
///
/// This is the half of `cover.rs` the duck alone never reaches — `run_hugs`, the
/// rule that a plain step landing still against the run keeps the pose, corners
/// included. Counted as "the anchor was there before the step and is still there
/// after, and the turn produced a `Moved`", which is precisely the turn loop's own
/// `crouch_walked` predicate observed from outside.
#[test]
fn the_bot_crouch_walks_along_the_bench() {
    let mut walks = 0;
    for profile in [Profile::CAUTIOUS, Profile::BALANCED] {
        for seed in 0..60 {
            let (mut state, _) = boot(seed);
            let mut bot = StealthBot::with_profile(profile);
            for _ in 0..DEFAULT_INPUT_CAP {
                if state.outcome() != Outcome::Playing {
                    break;
                }
                let anchor = state.crouched_behind();
                let events = state.step(bot.decide(&state));
                let moved = events
                    .iter()
                    .any(|e| matches!(e, intrusion_core::Event::Moved { .. }));
                if anchor.is_some() && moved && state.crouched_behind() == anchor {
                    walks += 1;
                }
            }
        }
    }
    assert!(
        walks > 0,
        "the bot never crouch-walked — §10.3's `run_hugs` is unexercised, so the \
             bot only ever ducks and waits",
    );
}

/// The strike is **legitimate**, seed by seed, not merely counted (#316/#183).
///
/// A takedown is legal from a guard's rear blind spot or under concealment and
/// nowhere else (§7.2/§155), and the front strike the old bot could sneak through
/// must not come back. Rather than trust the bot's own gate, this replays every
/// strike a batch lands and checks the *core's* answer at the moment of the bump:
/// the guard must not have detected the player, which is exactly the predicate
/// [`State::guard_detects_now`] settles and the one §7.2 refuses a bump against.
///
/// **The concealed strikes it lands are cupboard ones, never bench ones** (#379),
/// and that is geometry rather than a shy policy: concealment across a bench needs
/// the viewer on the far side of the furniture, which puts it at least two cells
/// away, while a takedown needs it orthogonally adjacent. The two conditions
/// cannot both hold — proven exhaustively in core's
/// `cover::an_adjacent_viewer_is_never_concealed_by_a_bench` — so there is no
/// crouch strike here to assert legal, and a batch reporting zero of them is the
/// right answer rather than a gap to close.
#[test]
fn every_takedown_the_bot_lands_is_a_legal_one() {
    let mut struck = 0;
    for seed in 0..40 {
        let (state, _) = boot(seed);
        let mut state = state;
        let mut bot = StealthBot::with_profile(Profile::CARELESS);
        for _ in 0..DEFAULT_INPUT_CAP {
            if state.outcome() != Outcome::Playing {
                break;
            }
            let input = bot.decide(&state);
            // A step into a guard is the takedown attempt (§7.2) — check the gate
            // the core will read, before it reads it.
            if let Input::Step(dir) = input {
                if let Some(target) = state.player().step(dir) {
                    for guard in state.guards() {
                        if guard.pos() == target {
                            assert!(
                                !state.guard_detects_now(guard),
                                "seed {seed}: struck a guard that had the player — \
                                     a front strike is refused by §7.2 and must never \
                                     be attempted",
                            );
                            struck += 1;
                        }
                    }
                }
            }
            state.step(input);
        }
    }
    assert!(
        struck > 0,
        "no strike happened in 40 seeds — this test would prove nothing",
    );
}

/// Every shipped profile still **plays the game** (§13.4), not just the
/// default one: over a batch each one reaches real endings rather than stalling
/// out en masse. A temperament whose numbers livelock the bot would quietly
/// turn its rows into a measurement of the bot instead of the game (§13.3),
/// which is exactly what this catches. Loose, like the balanced profile's own
/// mixed-outcome test: the exact counts are free to move.
#[test]
fn every_profile_finishes_its_runs() {
    let runs = 40;
    for profile in Profile::ALL {
        let records = run_batch(30..30 + runs, DEFAULT_INPUT_CAP, move |_| {
            StealthBot::with_profile(profile)
        })
        .expect("generates");
        let count = |o: RunOutcome| records.iter().filter(|r| r.outcome == o).count();
        let timeouts = count(RunOutcome::Timeout);
        assert!(
            timeouts <= runs as usize / 5,
            "{}: too many timeouts ({timeouts}/{runs}) — this temperament \
                 stalls rather than plays",
            profile.name,
        );
        assert!(
            count(RunOutcome::Win) >= 1 && count(RunOutcome::Capture) >= 1,
            "{}: a degenerate outcome profile ({} wins, {} captures)",
            profile.name,
            count(RunOutcome::Win),
            count(RunOutcome::Capture),
        );
    }
}

/// Regression (#171): the endless stalls #165 tipped the bot into now *finish*.
/// The close-behind/automatic doors (§10.4) reshaped guard coverage enough to
/// surface two self-inflicted stalls, both of which spent the whole input budget
/// without the run ending:
///
/// - **Marching onto its own exit.** Hunted with no reachable hideout, the flee
///   routine used to fall back on the exit cell; with objectives still out, a step
///   onto the exit is a refused, *free* bump (§4.5), so the turn never advanced and
///   the hunt never cooled (seeds 30, 43). It now cloaks or retreats instead.
/// - **Sealing itself into a cupboard.** Waiting out a guard parked on a hideout's
///   only mouth, the bot would eventually push on, take the guard down, and drop
///   the body across that mouth — the §7.2/#170 soft-lock (seeds 33, 34, 44, 58,
///   64, 65). It now leaves such a guard be and waits for the patrol to step off.
///
/// The second stall could no longer happen either way: #187 made a loose body
/// non-solid, so a body across a mouth stops nobody (#316). The seeds are kept
/// under **`balanced`**, which still declines the strike, so this stays a
/// regression test for the stalls rather than becoming a test of the new play.
///
/// Each seed must reach a real end (win or capture), never the input cap.
#[test]
fn the_close_behind_door_stalls_now_finish() {
    for seed in [30, 43, 33, 34, 44, 58, 64, 65] {
        let record = run_one(seed, &mut StealthBot::new(), DEFAULT_INPUT_CAP).expect("generates");
        assert_ne!(
            record.outcome,
            RunOutcome::Timeout,
            "seed {seed}: the bot should play the run to an end, not stall out",
        );
    }
}

/// The **no-cheat** guarantee (§11.5a, the ticket's asserted case): the bot cannot
/// route to intel it has never seen. At level start the player sees only their own
/// room, so a console in another room is fogged — outside `memory` — and must not
/// be a goal. The exit, by contrast, is the player's own tunnel and is known from
/// the off.
#[test]
fn the_bot_cannot_route_to_unseen_intel() {
    let (state, placement) = boot(0);
    let bot = StealthBot::new();

    // The exit is always known — the way the player came in.
    assert_eq!(
        exit_cell(&state),
        Some(placement.exit()),
        "the exit is known from the start"
    );

    // Every console the bot would head for is one it has actually seen.
    let known = bot.known_intel(&state);
    for &console in &known {
        assert!(
            state.memory().contains(console),
            "known intel {console:?} must have been seen"
        );
    }

    // There is at least one placed console the player has not seen yet, and the
    // bot does not treat it as a goal — it cannot route to what it has never seen.
    let unseen: Vec<Cell> = placement
        .intel()
        .iter()
        .copied()
        .filter(|&c| !state.memory().contains(c))
        .collect();
    assert!(
        !unseen.is_empty(),
        "the start room should not reveal every console at turn zero"
    );
    for console in unseen {
        assert!(
            !known.contains(&console),
            "unseen intel {console:?} must not be a goal"
        );
    }
}

/// The ticket's batch smoke test (§13.2–§13.4): over a batch of generated seeds
/// the bot finishes runs with a **mixed** outcome profile — some wins, some
/// captures, few timeouts — and actually uses its innate escape (Run to flee), so
/// the ability histogram has something real to measure. These are shape
/// assertions, deliberately loose: they check the bot *plays*, not that it plays
/// well (§13.4 — a smoke detector, not a judge), and the exact numbers are free to
/// move as the game is tuned.
///
/// The sim baseline holds the **innate-only** loadout (§8.3) — it plays *bare*, no
/// salvaged tech — so only Run is asserted here. Camouflage and the other tech are
/// not in the loadout to fire (a level must be winnable with no tech is the
/// baseline this measures); a run that wants to weigh a specific tech grants it
/// back and asserts on that.
///
/// The **takedown** is deliberately not required either, and under this profile
/// it must read exactly zero. It lands only from a guard's rear blind spot or
/// under concealment (§7.2/§155, gated live since #183), and `balanced` is an
/// avoidance-first temperament that declines the verb outright
/// ([`Profile::takedown_reach`] of zero, #316) — mandating it here would measure a
/// contrived hunt rather than the game (§13.3). Deliberate rear-takedown play now
/// exists; it lives in the striking profiles, and
/// [`the_striking_profiles_work_the_body_chain`] is where it is asserted.
#[test]
fn over_a_batch_the_outcome_profile_is_mixed() {
    let runs = 40;
    let records =
        run_batch(30..30 + runs, DEFAULT_INPUT_CAP, |_| StealthBot::new()).expect("generates");
    let count = |o: RunOutcome| records.iter().filter(|r| r.outcome == o).count();
    let wins = count(RunOutcome::Win);
    let captures = count(RunOutcome::Capture);
    let timeouts = count(RunOutcome::Timeout);

    assert!(wins >= 1, "expected some wins, got {wins}");
    assert!(captures >= 1, "expected some captures, got {captures}");
    // "Few" timeouts: the bot should almost always *finish* a run one way or the
    // other, never stall out en masse (the whole point over a hand-player).
    assert!(
        timeouts <= runs as usize / 5,
        "too many timeouts: {timeouts}/{runs} — the bot is stalling, not playing"
    );

    // The innate escape fires, so the §13.2 histogram is not measuring a bot that
    // never acts: Run (fleeing) shows. Tech is out of the bare loadout, so it is
    // not asserted — nor the takedown (see the doc comment above).
    let usage = records
        .iter()
        .fold(UsageHistogram::new(), |acc, r| acc.merged(&r.usage));
    assert!(
        usage.count(Verb::Run) > 0,
        "the bot never used its one innate escape — the histogram is measuring \
             a bot that does not play",
    );
}

/// **The bot still plays the game while holding Pierce Wall** (§13.2/#303).
///
/// The ability is unusable from most cells by design — its precondition is
/// *exactly one adjacent wall* — which is precisely the shape that could make a
/// naive policy hammer a key that never fires and stall the run out to the input
/// cap. This grants it and plays a batch through the ordinary loop: the outcome
/// profile stays mixed, so nothing livelocks.
///
/// Holding it is also very nearly **free**: on almost every seed the armed run is
/// input-for-input the run the bare bot played, because the cue declines from
/// almost every cell. What used to be true was *stronger* — that it declined from
/// **every** cell, so the key was never pressed at all — and it stopped being true
/// when rungs 2 and 3 started walking guards in (§7.3/#374). Under that much more
/// pressure the bot does occasionally find the one shape Pierce Wall is for and
/// takes it.
///
/// So the assertion is the one that still means something: holding an ability may
/// only change a run **by being used**. A diverging run that never pressed the key
/// would be the real defect — a loadout perturbing the policy by its mere presence,
/// which would make every with/without comparison (§4a) measure the perturbation
/// rather than the ability.
#[test]
fn the_bot_plays_identically_while_holding_pierce_wall() {
    /// Play `state` to a decision and report the inputs issued and how it ended.
    fn play(mut state: State) -> (Vec<Input>, Outcome, u32) {
        let mut bot = StealthBot::new();
        let mut issued = Vec::new();
        for _ in 0..DEFAULT_INPUT_CAP {
            if state.outcome() != Outcome::Playing {
                break;
            }
            let input = bot.decide(&state);
            issued.push(input);
            state.step(input);
        }
        (issued, state.outcome(), state.turn())
    }

    let (mut decided, mut diverged) = (0, 0);
    for seed in 30..50 {
        let (bare, _) = boot(seed);
        let (armed, _) = boot(seed);
        let armed =
            armed.with_loadout(intrusion_core::Loadout::innate().with(AbilityId::PierceWall));
        // Held, read off the bar's own roster rather than off the ability's
        // state: since #345 that state is **contextual**, so a fresh run standing
        // anywhere but square against one wall reads `Unusable` — which is the
        // ability working as designed, not a loadout that failed to take.
        assert!(
            armed
                .ability_statuses()
                .iter()
                .any(|s| s.id == AbilityId::PierceWall),
            "seed {seed}: the run holds the ability",
        );

        let bare = play(bare);
        let armed = play(armed);
        if bare != armed {
            assert!(
                armed.0.contains(&Input::Activate(AbilityId::PierceWall)),
                "seed {seed}: holding the ability changed the run without using it",
            );
            diverged += 1;
        }
        decided += u32::from(armed.1 != Outcome::Playing);
    }
    assert!(
        diverged <= 4,
        "{diverged}/20 runs diverged — holding one situational ability should be \
             nearly free, so this many says the policy is being perturbed rather than \
             finding the shape the ability is for",
    );
    assert!(
        decided >= 15,
        "only {decided}/20 runs reached a decision — the baseline is stalling, \
             so this test would prove nothing",
    );
}

/// **#405: §7.7's comms console has a live source at last.**
///
/// Until this verb existed `Terrain::CommsConsole` was scenery in the metrics: the
/// bot never routed to one and never bumped one, so `bodies_found` and every alert
/// row was measured in a world where the radio counterplay was never taken.
///
/// The split is the one the profiles were built around, and it is the same shape the
/// `takedown_reach` assertions make:
///
/// - the **opting-in** temperaments land a non-zero count over a sweep — a zero here
///   would be a broken policy rather than a temperament (#379's rule);
/// - the **declining** ones land exactly zero, which is `comms_reach: 0` working.
///
/// **The opting-in half searches rather than fixing a range, and that is a finding
/// rather than a convenience.** An opportunistic bump only happens on a seed whose
/// layout walks the bot past the console, and at §7.7's current 16-cell placement
/// distance **[START]** that is roughly one seed in fifty. A fixed 60-seed window
/// contained exactly *one* hit, which is a test one generation change away from a
/// false red — so this walks seeds until it finds a hit and stops, which is fast in
/// the ordinary case and still fails honestly if the policy ever breaks. If this
/// starts needing most of its budget, the encounter rate has dropped and §7.7's
/// placement knob is what to look at.
///
/// Loose and direction-only (§13.4): the counts are free to move, the split is not.
#[test]
fn the_careful_temperaments_silence_the_comms_console() {
    // Generous, because it bounds a search rather than describing the sweep: the
    // hit normally lands far inside it.
    const HUNT: u64 = 400;
    const ZERO_SWEEP: std::ops::Range<u64> = 0..60;

    let silences = |profile: Profile, seed: u64| {
        let mut bot = StealthBot::with_profile(profile);
        run_one(seed, &mut bot, DEFAULT_INPUT_CAP)
            .expect("generates")
            .usage
            .count(Verb::SilenceRadio)
    };

    for profile in [Profile::BALANCED, Profile::CAUTIOUS] {
        let found = (0..HUNT).any(|seed| silences(profile, seed) > 0);
        assert!(
            found,
            "{}: the opportunistic bump never happened in {HUNT} seeds — \
             a zero here is a broken policy, not a temperament",
            profile.name,
        );
    }
    for profile in [Profile::AGGRESSIVE, Profile::CARELESS] {
        let total: u32 = ZERO_SWEEP.map(|seed| silences(profile, seed)).sum();
        assert_eq!(
            total, 0,
            "{}: comms_reach 0 declines the verb outright",
            profile.name,
        );
    }
}

/// #405/§2 of `docs/bot-behaviour.md`: **rules asked of core, never re-implemented.**
///
/// The trigger is [`Affordance::SilenceRadio`] out of [`State::affordances`], not a
/// private scan of the four neighbours — so "is this console still live, is it in
/// view, would the bump land" is answered once, by the game. This walks a real run
/// and pins the two halves of that agreement on every single turn:
///
/// - the bot presses the silence **only** on a turn core offers the affordance;
/// - and when it does, it steps in **the direction core named**.
///
/// A hand-rolled neighbour scan would drift from core's answer the first time the
/// preconditions moved (a spent console, a fogged one), and this is what would catch
/// it. It also gets §11.5a for free: `affordances` is FOV-gated, so the console must
/// have been *seen*.
#[test]
fn the_silence_is_the_cores_affordance_and_never_a_terrain_scan() {
    let mut silences = 0;
    for seed in 0..120 {
        let (mut state, _) = boot(seed);
        let mut bot = StealthBot::with_profile(Profile::BALANCED);
        for _ in 0..DEFAULT_INPUT_CAP {
            if state.outcome() != Outcome::Playing {
                break;
            }
            // What core says is on offer, read *before* the bot is asked.
            let offered: Option<Direction> = state
                .affordances()
                .into_iter()
                .find(|&(_, a)| a == Affordance::SilenceRadio)
                .map(|(dir, _)| dir);
            let input = bot.decide(&state);
            state.step(input);
            if state
                .last_events()
                .iter()
                .any(|e| matches!(e, Event::CommsSilenced { .. }))
            {
                assert_eq!(
                    input,
                    Input::Step(offered.expect(
                        "the switch was thrown on a turn core offered no SilenceRadio \
                         affordance — the bot is scanning terrain of its own",
                    )),
                    "seed {seed}: the bump must go where core said it was",
                );
                silences += 1;
            }
        }
    }
    assert!(
        silences > 0,
        "the sweep never threw the switch — the assertions above passed vacuously",
    );
}

/// #405: **it never detours.** No goal, no cost-field term, no frontier bias toward
/// the console — a seed where the bot ends up beside one got there on the route it
/// was already walking.
///
/// Asserted rather than asserted-by-comment, and asserted as the strongest form of
/// the claim: on every turn where core is **not** offering the silence, a
/// `comms_reach: 1` bot and a `comms_reach: 0` bot — today's bot, unchanged — must
/// choose the *same input*. Any routing preference whatsoever, however slight, would
/// have to show up as a divergence on some turn of some seed, because a preference
/// that never changed a decision is not a preference.
///
/// The declining bot is polled rather than driven, so it sees exactly the same states
/// in the same order; that is only self-consistent while the two agree, which is
/// precisely what is being asserted.
#[test]
fn the_bot_never_detours_to_the_comms_console() {
    let mut compared = 0;
    for seed in 0..40 {
        let (mut state, _) = boot(seed);
        let mut opts_in = StealthBot::with_profile(Profile::BALANCED);
        let mut declines = StealthBot::with_profile(Profile {
            comms_reach: 0,
            ..Profile::BALANCED
        });
        for turn in 0..DEFAULT_INPUT_CAP {
            if state.outcome() != Outcome::Playing {
                break;
            }
            let offered = state
                .affordances()
                .iter()
                .any(|&(_, a)| a == Affordance::SilenceRadio);
            let taken = opts_in.decide(&state);
            let untaken = declines.decide(&state);
            if !offered {
                assert_eq!(
                    taken, untaken,
                    "seed {seed} turn {turn}: with no console under its hand, wanting \
                     the switch must change nothing about where the bot walks",
                );
                compared += 1;
            }
            state.step(taken);
        }
    }
    assert!(compared > 0, "the sweep compared nothing");
}

/// #405: **the silence is never taken while `Flee` or `TakeCover` applies.**
/// Silencing while a patrol is closing spends the turn that was the escape, which is
/// why the step sits below both of them in `decide`. Pinned by walking real runs and
/// checking that no turn ever both wants cover and takes the switch.
#[test]
fn a_closing_patrol_outranks_the_comms_switch() {
    for seed in 0..40 {
        let (mut state, _) = boot(seed);
        let mut bot = StealthBot::with_profile(Profile::CAUTIOUS);
        for _ in 0..DEFAULT_INPUT_CAP {
            if state.outcome() != Outcome::Playing {
                break;
            }
            let input = bot.decide(&state);
            state.step(input);
            let silenced = state
                .last_events()
                .iter()
                .any(|e| matches!(e, Event::CommsSilenced { .. }));
            if silenced {
                assert!(
                    !being_hunted(&state, &danger_cells(&state)),
                    "seed {seed}: the switch was thrown on a turn the bot was hunted",
                );
            }
        }
    }
}
