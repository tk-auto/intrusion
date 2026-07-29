//! The replay script notation (§12.4): the textual form of a run's `[inputs]`.
//!
//! §12.4 [SETTLED] — *"A replay is `(seed, [inputs])`. Nothing else."* The seed
//! reproduces the facility; this module is the *inputs* half in text — the string
//! that spells an [`Input`] stream, both ways. It lives in the core, beside the
//! §11.6 [`input`](crate::input) mapping, because every consumer needs the same
//! one spelling: the sim captures a bot run into it (`--emit-replay`), the web
//! shell parses it to play a replay back, and an Artifact bakes it in — all off a
//! single definition, so the notation can never drift between them.
//!
//! It is a superset of a bare move list: `N`/`E`/`S`/`W` step, `.` waits, and a
//! signed `+<letter>`/`-<letter>` activates/deactivates the ability with that
//! [script letter](ability_script_letter) (`+r` Run, `-c` Camouflage, …). Every
//! [`Input`] variant has a token, so a captured stream re-feeds byte-for-byte —
//! the determinism property, asserted below and end-to-end in the sim. The
//! takedown-bump and the drag-grab are steps *into* a target (§7.2/§8.3), so
//! `N`/`E`/`S`/`W` already spell them; they need no token of their own.
//!
//! **The letters are the notation's, not the keyboard's** (#359). They were the
//! §11.6 hotkeys until the keys moved to the ability bar's slots; a script needs a
//! spelling that is a fact about the *ability* rather than about one run's loadout,
//! so the identity map came here, to the consumer it still has.

use crate::ability::AbilityId;
use crate::cell::Direction;
use crate::state::Input;

/// The script letter of a §8.3 ability, by name — the notation's identity-keyed
/// spelling (§12.4).
///
/// The assignment is a `match` on the ability's identity: there is no list to be
/// ordered, so no reordering, insertion or removal can ever move a letter, and a
/// script captured from one run reads the same in every other. The tests pin each
/// pair, so even an *edit* here is a visible decision rather than a silent shift —
/// which matters more for a stored replay than it ever did for a key, since a moved
/// letter would make an old script name a different ability without erroring.
pub fn ability_script_letter(ability: &str) -> Option<char> {
    Some(match ability {
        "Run" => 'r',
        "Takedown" => 't',
        "Drag" => 'g',
        "Camouflage" => 'c',
        "Decoy" => 'd',
        "Phase Out" => 'x',
        "Autodoors" => 'a',
        "Confusion" => 'z',
        "Vision" => 'v',
        "Pierce Wall" => 'b',
        // `s` for *seal*, not for the initial: `d` was already Decoy's, and the
        // letters have to stay unique for [`parse_script`] to be unambiguous.
        "Lockdown" => 's',
        _ => return None,
    })
}

/// One [`Input`] as its script token: a single letter for a move or wait, and a
/// signed `<+|-><letter>` pair for an ability activate/deactivate.
///
/// - `Step` → `N`/`E`/`S`/`W`; `Wait` → `.`.
/// - `Activate(id)` → `+` then the ability's [script letter](ability_script_letter);
///   `Deactivate(id)` → `-`.
pub fn input_token(input: Input) -> String {
    match input {
        Input::Step(Direction::North) => "N".to_string(),
        Input::Step(Direction::East) => "E".to_string(),
        Input::Step(Direction::South) => "S".to_string(),
        Input::Step(Direction::West) => "W".to_string(),
        Input::Wait => ".".to_string(),
        Input::Activate(id) => format!("+{}", id.script_letter()),
        Input::Deactivate(id) => format!("-{}", id.script_letter()),
    }
}

/// A whole input stream in the script notation — every token concatenated, the
/// exact string [`parse_script`] round-trips back to the same stream.
pub fn to_script(inputs: &[Input]) -> String {
    inputs.iter().copied().map(input_token).collect()
}

/// The ability an activate/deactivate token names, by its script letter — the
/// reverse of [`AbilityId::script_letter`], so the two halves of the notation
/// cannot disagree.
fn ability_for_script_letter(ch: char) -> Option<AbilityId> {
    AbilityId::ALL
        .into_iter()
        .find(|id| id.script_letter() == ch)
}

/// Parse the script notation to an [`Input`] stream (the inverse of [`to_script`]).
///
/// One input per token: `N`/`E`/`S`/`W` step (case folded), `.` waits, `+<letter>`
/// activates and `-<letter>` deactivates the ability with that script letter.
/// Whitespace between tokens is ignored, so a long stream can be wrapped for
/// reading. An unknown character, or a `+`/`-` not followed by a known letter, is
/// a hard error — a malformed replay must not silently drop an input (§12.4).
pub fn parse_script(text: &str) -> Result<Vec<Input>, String> {
    let mut inputs = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        let input = match c.to_ascii_uppercase() {
            'N' => Input::Step(Direction::North),
            'E' => Input::Step(Direction::East),
            'S' => Input::Step(Direction::South),
            'W' => Input::Step(Direction::West),
            '.' => Input::Wait,
            '+' | '-' => {
                let key = chars
                    .next()
                    .ok_or_else(|| format!("replay: `{c}` needs an ability letter after it"))?;
                let id = ability_for_script_letter(key.to_ascii_lowercase()).ok_or_else(|| {
                    format!("replay: `{c}{key}` is not an ability (want +/- r/c/d/x)")
                })?;
                if c == '+' {
                    Input::Activate(id)
                } else {
                    Input::Deactivate(id)
                }
            }
            w if w.is_whitespace() => continue,
            other => {
                return Err(format!(
                    "replay: unknown move {other:?} (want N/E/S/W/. or +/- an ability key)"
                ))
            }
        };
        inputs.push(input);
    }
    Ok(inputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `Input` variant round-trips through the notation: serialise to a
    /// token, parse it back, land on the same input. This is what makes a captured
    /// stream re-feedable (§12.4) — a variant with no token would be silently lost.
    #[test]
    fn every_input_round_trips_through_the_notation() {
        let mut all = vec![
            Input::Step(Direction::North),
            Input::Step(Direction::East),
            Input::Step(Direction::South),
            Input::Step(Direction::West),
            Input::Wait,
        ];
        for id in AbilityId::ALL {
            all.push(Input::Activate(id));
            all.push(Input::Deactivate(id));
        }
        for input in all {
            let token = input_token(input);
            assert_eq!(
                parse_script(&token).as_deref(),
                Ok(&[input][..]),
                "{input:?} did not round-trip through {token:?}"
            );
        }
    }

    /// The whole-stream round-trip, and that whitespace between tokens is ignored
    /// so an emitted stream can be wrapped and still parse.
    #[test]
    fn a_stream_round_trips_and_ignores_whitespace() {
        let stream = vec![
            Input::Step(Direction::North),
            Input::Activate(AbilityId::Run),
            Input::Step(Direction::East),
            Input::Wait,
            Input::Deactivate(AbilityId::Run),
            Input::Step(Direction::South),
        ];
        let script = to_script(&stream);
        assert_eq!(script, "N+rE.-rS");
        assert_eq!(parse_script(&script), Ok(stream.clone()));
        assert_eq!(parse_script("N +r\nE . -r S"), Ok(stream));
    }

    /// Case folding: a lowercase move letter and an uppercase ability key both
    /// resolve, so a hand-typed script is as valid as an emitted one.
    #[test]
    fn the_notation_is_case_insensitive() {
        assert_eq!(
            parse_script("nes"),
            Ok(vec![
                Input::Step(Direction::North),
                Input::Step(Direction::East),
                Input::Step(Direction::South),
            ])
        );
        assert_eq!(
            parse_script("+R"),
            Ok(vec![Input::Activate(AbilityId::Run)])
        );
    }

    /// Malformed notation is a hard error, never a silently dropped input: a bare
    /// sign with no key, an unknown ability letter, and an unknown character all fail.
    #[test]
    fn malformed_notation_is_rejected() {
        assert!(parse_script("+").is_err(), "a bare sign needs a letter");
        assert!(parse_script("+q").is_err(), "q is no ability letter");
        assert!(parse_script("Nq").is_err(), "q is not a move");
    }

    /// The notation's letters, pinned pair by pair (moved here with the map, #359).
    /// A stored script is read back by these, so a letter moving would make an old
    /// replay name a *different* ability and never error — if any of these fails, it
    /// must be because someone decided to break that, not as a side effect.
    #[test]
    fn every_script_letter_is_pinned() {
        for (ability, letter) in [
            ("Run", 'r'),
            ("Takedown", 't'),
            ("Drag", 'g'),
            ("Camouflage", 'c'),
            ("Decoy", 'd'),
            ("Phase Out", 'x'),
            ("Autodoors", 'a'),
            ("Confusion", 'z'),
            ("Vision", 'v'),
            ("Pierce Wall", 'b'),
            ("Lockdown", 's'),
        ] {
            assert_eq!(ability_script_letter(ability), Some(letter), "{ability}");
        }
        assert_eq!(
            ability_script_letter("Teleport"),
            None,
            "not in the catalogue"
        );
    }

    /// The letters are keyed by **identity**, so no two abilities share one — the
    /// collision that would make [`parse_script`] ambiguous, silently resolving a
    /// token to whichever id came first in [`AbilityId::ALL`].
    #[test]
    fn no_two_abilities_share_a_script_letter() {
        let letters: Vec<char> = AbilityId::ALL
            .into_iter()
            .map(|id| id.script_letter())
            .collect();
        for (i, a) in letters.iter().enumerate() {
            for b in &letters[i + 1..] {
                assert_ne!(a, b, "two abilities spell as {a:?}");
            }
        }
    }
}
