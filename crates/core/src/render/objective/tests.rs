//! The objective derivation as a function (§4.5/§12.1) — the words each gate produces,
//! before any card has drawn them.
//!
//! What the *cards* do with these lines is tested where they are drawn: the level-start
//! splash in [`super::super::splash`]'s tests, the Level info tab in
//! [`super::super::help`]'s.

use super::*;
use crate::place::LevelConfig;

/// Every gate, over a facility that holds both kinds of objective — the envelope the
/// lines are written for.
const GATES: [IntelGate; 3] = [IntelGate::All, IntelGate::AtLeastOne, IntelGate::None];

/// **Each gate says its own thing** (§4.5/#244). Three settings, three sentences: a line
/// shared between two of them would tell a player they were playing the other one's
/// rule.
#[test]
fn every_gate_states_its_own_objective() {
    let mut seen: Vec<String> = Vec::new();
    for gate in GATES {
        let line = take_line(gate, 3, 0);
        assert!(!seen.contains(&line), "{gate:?} reuses another gate's line");
        seen.push(line);
    }
}

/// **The minimum haul names the crate exactly where there is one** (#574).
///
/// The widened gate is met by an intel console *or* an equipment cache, and the player
/// has no other way to learn that: nothing on the board says a crate would do. So the
/// line says so on a facility that hides crates — and does not on one that hides none,
/// because naming a crate that is not in the building is a promise it cannot keep. The
/// distinction is only `AtLeastOne`'s: [`IntelGate::All`] is the complete-the-set
/// objective and a crate is no part of the set, so its line is the console count either
/// way.
#[test]
fn the_haul_line_names_the_crate_only_where_there_are_crates() {
    assert_eq!(take_line(IntelGate::AtLeastOne, 3, 0), HAUL_INTEL);
    assert_eq!(take_line(IntelGate::AtLeastOne, 3, 1), HAUL_ANY);
    assert_eq!(take_line(IntelGate::AtLeastOne, 3, 3), HAUL_ANY);
    assert!(HAUL_ANY.contains("crate"), "it names the crate: {HAUL_ANY}");

    for caches in [0, 3] {
        assert_eq!(
            take_line(IntelGate::All, 3, caches),
            take_line(IntelGate::All, 3, 0),
            "the complete-the-set gate does not count crates",
        );
    }
}

/// **A facility with nothing to take says so** (§2.3), and *nothing* is read through the
/// gate: under the minimum haul a facility with no consoles but a crate in it still has
/// something to reach for, so the empty line would be a lie there. Under
/// [`IntelGate::All`] the crate does not count and the row names the emptiness.
///
/// The row also loses its reward colour when there is nothing to reach for — Ground, the
/// reading the modifier list's own *none active* row takes.
#[test]
fn nothing_to_take_says_so_and_reads_as_empty() {
    for gate in GATES {
        assert_eq!(take_line(gate, 0, 0), NO_INTEL, "{gate:?} on a bare room");
        assert_eq!(objective_line(gate, 0, 0).1, Category::Ground, "{gate:?}");
        assert_eq!(objective_line(gate, 2, 0).1, Category::Interest, "{gate:?}");
    }
    assert_eq!(take_line(IntelGate::All, 0, 2), NO_INTEL);
    assert_ne!(
        take_line(IntelGate::AtLeastOne, 0, 2),
        NO_INTEL,
        "a crate is something to take under the minimum haul",
    );
    assert_eq!(
        objective_line(IntelGate::AtLeastOne, 0, 2).1,
        Category::Interest,
    );
}

/// **No objective line is clipped on the board** (§11.4's row-fits rule). The fixed lines
/// are bounded at compile time; this is the variable one's companion, measured over the
/// whole console envelope the §12.6 knob can reach — and then some, because a hand-built
/// state is not bounded by it — crossed with the crate counts the [`CacheCount`] knob can
/// ask for.
///
/// [`CacheCount`]: crate::CacheCount
#[test]
fn no_objective_line_is_clipped_on_the_board() {
    let room = (LevelConfig::V1.width - help::CONTENT_INDENT - 1) as usize;
    for intel in 0..=LevelConfig::INTEL_MAX + 20 {
        for caches in 0..=5 {
            for gate in GATES {
                let line = take_line(gate, intel, caches);
                assert!(
                    line.chars().count() <= room,
                    "{line:?} runs past the board's last column",
                );
            }
        }
    }
}
