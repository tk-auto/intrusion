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
//! signed `+<key>`/`-<key>` activates/deactivates the ability with that settled
//! §11.6 hotkey (`+r` Run, `-c` Camouflage, …). Every [`Input`] variant has a
//! token, so a captured stream re-feeds byte-for-byte — the determinism property,
//! asserted below and end-to-end in the sim. The takedown-bump and the drag-grab
//! are steps *into* a target (§7.2/§8.3), so `N`/`E`/`S`/`W` already spell them;
//! they need no token of their own.

use crate::ability::AbilityId;
use crate::cell::Direction;
use crate::state::Input;

/// One [`Input`] as its script token: a single letter for a move or wait, and a
/// signed `<+|-><hotkey>` pair for an ability activate/deactivate.
///
/// - `Step` → `N`/`E`/`S`/`W`; `Wait` → `.`.
/// - `Activate(id)` → `+` then the ability's §11.6 hotkey; `Deactivate(id)` → `-`.
pub fn input_token(input: Input) -> String {
    match input {
        Input::Step(Direction::North) => "N".to_string(),
        Input::Step(Direction::East) => "E".to_string(),
        Input::Step(Direction::South) => "S".to_string(),
        Input::Step(Direction::West) => "W".to_string(),
        Input::Wait => ".".to_string(),
        Input::Activate(id) => format!("+{}", id.hotkey()),
        Input::Deactivate(id) => format!("-{}", id.hotkey()),
    }
}

/// A whole input stream in the script notation — every token concatenated, the
/// exact string [`parse_script`] round-trips back to the same stream.
pub fn to_script(inputs: &[Input]) -> String {
    inputs.iter().copied().map(input_token).collect()
}

/// The ability an activate/deactivate token names, by its §11.6 hotkey letter —
/// the reverse of [`AbilityId::hotkey`], so the notation and the keyboard agree.
fn ability_for_hotkey(ch: char) -> Option<AbilityId> {
    AbilityId::ALL.into_iter().find(|id| id.hotkey() == ch)
}

/// Parse the script notation to an [`Input`] stream (the inverse of [`to_script`]).
///
/// One input per token: `N`/`E`/`S`/`W` step (case folded), `.` waits, `+<key>`
/// activates and `-<key>` deactivates the ability with that §11.6 hotkey.
/// Whitespace between tokens is ignored, so a long stream can be wrapped for
/// reading. An unknown character, or a `+`/`-` not followed by a known hotkey, is
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
                    .ok_or_else(|| format!("replay: `{c}` needs an ability key after it"))?;
                let id = ability_for_hotkey(key.to_ascii_lowercase()).ok_or_else(|| {
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
    /// sign with no key, an unknown ability key, and an unknown character all fail.
    #[test]
    fn malformed_notation_is_rejected() {
        assert!(parse_script("+").is_err(), "a bare sign needs a key");
        assert!(parse_script("+z").is_err(), "z is no ability hotkey");
        assert!(parse_script("Nq").is_err(), "q is not a move");
    }
}
