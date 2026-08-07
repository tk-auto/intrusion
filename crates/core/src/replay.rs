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
//!
//! **The link is here for the same reason the notation is** (#411). A replay travels
//! as `seed=<token>&inputs=<script>` — the level's token paired with this notation —
//! and that pairing now has three consumers: the web shell *writes* it from the help
//! panel's copy control and *reads* it at boot, and the sim *reads* it back to
//! narrate a run someone pasted. Spelled in one crate and re-derived in another it
//! would be a format that can drift; spelled here, [`replay_fragment`] and
//! [`parse_replay_link`] are the only two definitions of it.

use crate::ability::AbilityId;
use crate::cell::Direction;
use crate::level_seed::LevelSeed;
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
        // `e` for *escape*: the seal took `s` first, and a passive's letter is never
        // written into a script anyway (it has no activation to spell) — it exists so
        // the map stays exhaustive over the catalogue, as Vision's does.
        "Saver" => 'e',
        // `o` for the middle of *drone*: `d` is the Decoy's and the letters have to
        // stay unique for [`parse_script`] to be unambiguous.
        "Drone" => 'o',
        // `f` for *forged* — and for the initial, which for once is free.
        "False Call" => 'f',
        // `u` for the middle of *Guide*: `g` is the Drag's. Like Vision's and the
        // Saver's it is never written into a script — a passive has no activation to
        // spell — and exists so the map stays exhaustive over the catalogue.
        "Guide" => 'u',
        // `n` for *needle*, on the "`s` for *seal*" precedent: neither of the Dart's own
        // letters is free — `d` is the Decoy's and `t` the Takedown's — and the letters
        // have to stay unique for [`parse_script`] to be unambiguous.
        "Dart" => 'n',
        _ => return None,
    })
}

/// One [`Input`] as its script token: a single letter for a move or wait, and a
/// signed `<+|-><letter>` pair for an ability activate/deactivate.
///
/// - `Step` → `N`/`E`/`S`/`W`; `Wait` → `.`.
/// - `Activate(id)` → `+` then the ability's [script letter](ability_script_letter);
///   `Deactivate(id)` → `-`.
/// - `Discard(id)` → `!` then the letter — the exchange's answer (§8.3/#266), which
///   names the ability **dropped**. A third sign rather than a reuse of `-`: a
///   toggle-off and a trade are different actions on the same ability, and a script
///   that spelled them alike would replay one as the other.
pub fn input_token(input: Input) -> String {
    match input {
        Input::Step(Direction::North) => "N".to_string(),
        Input::Step(Direction::East) => "E".to_string(),
        Input::Step(Direction::South) => "S".to_string(),
        Input::Step(Direction::West) => "W".to_string(),
        Input::Wait => ".".to_string(),
        Input::Activate(id) => format!("+{}", id.script_letter()),
        Input::Deactivate(id) => format!("-{}", id.script_letter()),
        Input::Discard(id) => format!("!{}", id.script_letter()),
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
/// activates, `-<letter>` deactivates and `!<letter>` discards (#266) the ability with
/// that script letter.
/// Whitespace between tokens is ignored, so a long stream can be wrapped for
/// reading. An unknown character, or a `+`/`-`/`!` not followed by a known letter, is
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
            '+' | '-' | '!' => {
                let key = chars
                    .next()
                    .ok_or_else(|| format!("replay: `{c}` needs an ability letter after it"))?;
                let id = ability_for_script_letter(key.to_ascii_lowercase()).ok_or_else(|| {
                    format!("replay: `{c}{key}` is not an ability (want +/-/! r/c/d/x)")
                })?;
                match c {
                    '+' => Input::Activate(id),
                    '!' => Input::Discard(id),
                    _ => Input::Deactivate(id),
                }
            }
            w if w.is_whitespace() => continue,
            other => {
                return Err(format!(
                    "replay: unknown move {other:?} (want N/E/S/W/. or +/-/! an ability key)"
                ))
            }
        };
        inputs.push(input);
    }
    Ok(inputs)
}

/// The fragment a replay travels as (§12.4/§13.1/#411): `seed=<token>&inputs=<script>`
/// — the level-seed token paired with the input stream, the two carriers a boot
/// already reads, and the string the help panel's copy control puts on the clipboard.
///
/// Both halves are fragment-safe as spelt — a token is lowercase letters and the
/// notation is `NESW.`, `+`/`-` and letters, so neither carries an `&`, a `#` or
/// anything a browser rewrites — which is what lets the whole thing ride in a URL
/// hash with no percent-encoding in the loop.
pub fn replay_fragment(token: &str, script: &str) -> String {
    format!("seed={token}&inputs={script}")
}

/// The fragment a **level** travels as (§13.1/#572): `seed=<token>`, the shorter half
/// of [`replay_fragment`] and the whole of what a shared link needs to name a run
/// nobody has played yet.
///
/// Spelled here rather than at its three sites — the address bar the shell reflects
/// into, the Level info tab's `copy [c]` link, and the sim's play link — because a
/// level link and a replay link that disagreed about the field name would be two
/// carriers pretending to be one (§12.4). The same fragment-safety argument holds: a
/// token is eighteen lowercase letters, so nothing here needs percent-encoding.
pub fn level_fragment(token: &str) -> String {
    format!("seed={token}")
}

/// Read one `name=<value>` field out of a `?a=b&…` / `#a=b&…` fragment, tolerating a
/// leading `?`/`#` and any other fields around it. The single splitter every reader
/// of the pair shares, so no consumer re-derives what a field looks like.
pub fn field_in<'a>(fragment: &'a str, name: &str) -> Option<&'a str> {
    fragment
        .trim_start_matches(['?', '#'])
        .split('&')
        .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
        .filter(|value| !value.is_empty())
}

/// Parse a **pasted replay link** back into the run it names (#411) — the read half
/// of [`replay_fragment`], and the inverse of what the help panel copies.
///
/// It takes the link as pasted, not a hand-split pair: a whole URL, the fragment
/// alone, with `#` or `?`, in either field order, with unrelated fields alongside.
/// That tolerance is the point — the URL a player copies out of a hosted build
/// carries a query of the host's own (`?__frame_t=…#seed=…`), and requiring a human
/// to cut the fragment off it by eye is precisely the step that gets done wrong.
/// **The hash wins** where both are present, because that is where the copy control
/// puts the pair and what a static host passes through untouched.
///
/// The `inputs=` field may be **absent**, which is a link naming a *level* and no
/// run — a perfectly ordinary thing to be handed. It reads as a replay of length
/// zero, so the caller shows the opening facility; the caller is told how many
/// inputs there were and can say so. A missing or undecodable `seed=` is a hard
/// error, as is a malformed script: a link that cannot be reproduced exactly must
/// not resolve to a plausible near-miss (§12.4).
pub fn parse_replay_link(text: &str) -> Result<(LevelSeed, Vec<Input>), String> {
    let fragment = link_fragment(text.trim());
    let token = field_in(fragment, "seed")
        .ok_or_else(|| "replay link: no `seed=<token>` field in it".to_string())?;
    let level = LevelSeed::decode(token)
        .ok_or_else(|| format!("replay link: not a level-seed token: {token}"))?;
    let inputs = match field_in(fragment, "inputs") {
        Some(script) => parse_script(script)?,
        None => Vec::new(),
    };
    Ok((level, inputs))
}

/// The part of a pasted link the fields live in: everything after the first `#` if
/// there is one, else after the first `?`, else the whole string. Splitting on the
/// **first** of each is what keeps a host's own query out of the answer.
fn link_fragment(text: &str) -> &str {
    if let Some((_, hash)) = text.split_once('#') {
        return hash;
    }
    match text.split_once('?') {
        Some((_, query)) => query,
        None => text,
    }
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
            all.push(Input::Discard(id));
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

    /// **The three signs are three actions** (#266): `+`, `-` and `!` on the same letter
    /// parse to activate, deactivate and discard, and never to each other. A script that
    /// spelled a trade like a toggle-off would replay the run as a different one — the
    /// ability would still be held, and every later token would be fed to a loadout the
    /// original never had.
    #[test]
    fn the_three_ability_signs_stay_apart() {
        let id = AbilityId::Camouflage;
        let letter = id.script_letter();
        for (sign, expected) in [
            ('+', Input::Activate(id)),
            ('-', Input::Deactivate(id)),
            ('!', Input::Discard(id)),
        ] {
            let token = format!("{sign}{letter}");
            assert_eq!(
                parse_script(&token).as_deref(),
                Ok(&[expected][..]),
                "{token}"
            );
            assert_eq!(input_token(expected), token);
        }
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

    /// The link round-trips: what the copy control writes is what a paste reads
    /// back, level and inputs both (§12.4/#411).
    #[test]
    fn a_replay_link_round_trips() {
        let level = LevelSeed::quick_play(8371);
        let inputs = parse_script("NN+rE.-rSS").expect("a legal script");
        let token = level.encode().expect("a config a run can hold");
        let fragment = replay_fragment(&token, &to_script(&inputs));
        assert_eq!(
            parse_replay_link(&fragment),
            Ok((level, inputs)),
            "the pair survives its own spelling",
        );
    }

    /// A **level** link round-trips through the same reader (§13.1/#572): what
    /// `copy [c]` writes is what a paste reads back, and it names the level with no
    /// run attached — a replay of length zero, which is exactly what being handed a
    /// facility nobody has played yet means.
    ///
    /// The two fragments are asserted to agree on the field, because the whole reason
    /// the shorter one is spelled beside the longer is that they must.
    #[test]
    fn a_level_link_round_trips_and_names_no_run() {
        let level = LevelSeed::quick_play(8371);
        let token = level.encode().expect("a config a run can hold");
        let fragment = level_fragment(&token);
        assert_eq!(fragment, format!("seed={token}"));
        assert_eq!(
            parse_replay_link(&fragment),
            Ok((level, Vec::new())),
            "a level link is a replay of length zero",
        );
        assert!(
            replay_fragment(&token, "N").starts_with(&fragment),
            "the level fragment is the replay fragment's own first field",
        );
    }

    /// **A link is read as pasted** (#411): a whole URL — hash, and a host's own
    /// query in front of it — the fragment alone, either field order, and companions
    /// alongside. This is the tolerance that means nobody has to cut a fragment off a
    /// URL by eye, so it is walked over the real shapes rather than the tidy one.
    #[test]
    fn a_pasted_link_is_read_in_every_shape_it_arrives_in() {
        let level = LevelSeed::quick_play(8371);
        let token = level.encode().expect("a config a run can hold");
        let expected = Ok((level, vec![Input::Step(Direction::North)]));

        for link in [
            // The fragment alone, and with each sigil.
            format!("seed={token}&inputs=N"),
            format!("#seed={token}&inputs=N"),
            format!("?seed={token}&inputs=N"),
            // Field order is not part of the format.
            format!("#inputs=N&seed={token}"),
            // A real hosted URL: the host's *own* query sits before the hash, and the
            // hash is what carries the run. The query must not be mistaken for it.
            format!("https://example.test/_f/1785420630-6a43/?__frame_t=abc.def&__frame_v=manifest.json#seed={token}&inputs=N"),
            // The Pages form, and an unrelated field beside the pair.
            format!("https://tk-auto.github.io/intrusion/#seed={token}&inputs=N"),
            format!("#a=1&seed={token}&inputs=N&b=2"),
            // Whitespace from a paste.
            format!("  #seed={token}&inputs=N  "),
        ] {
            assert_eq!(parse_replay_link(&link), expected, "link {link:?}");
        }

        // A level link with no inputs is a replay of length zero — the opening
        // facility — not an error: being handed one is ordinary.
        assert_eq!(
            parse_replay_link(&format!("#seed={token}")),
            Ok((level, Vec::new())),
        );
    }

    /// A link that cannot be reproduced **exactly** is refused rather than resolved
    /// to a near-miss (§12.4): no seed field, a seed that is not a token (the old
    /// bare-number form among them, #333), and a script with a bad token in it.
    #[test]
    fn an_unreproducible_link_is_refused() {
        for link in [
            "#inputs=NN",
            "https://example.test/",
            "#seed=8371&inputs=NN",
            "#seed=notatoken",
        ] {
            assert!(parse_replay_link(link).is_err(), "link {link:?}");
        }
        let token = LevelSeed::quick_play(1).encode().expect("a token");
        assert!(
            parse_replay_link(&format!("#seed={token}&inputs=NQ")).is_err(),
            "a malformed script is an error, not a truncated run",
        );
    }

    /// A **real link, pasted from a real build** — the one this feature was first
    /// asked to read, kept verbatim as the fixture. It pins the whole chain end to
    /// end: the host's query is skipped, the token decodes to the run that was
    /// played, and the script is the thirteen inputs that were pressed.
    #[test]
    fn the_first_link_anyone_pasted_still_reads() {
        let link = "https://6dcafcf6-cb7b-4f80-9d6f-db85c4366efa.frame.claudeusercontent.com\
                    /_f/1785420630-6a43/?__frame_t=uUXWTnWCKDUNRtSJccvSP10v.3dcc06db-1137-4123\
                    -b2ec-7027f73c03ca.fd285f8d-60c8-459b-8dcc-abc57cc530f5.1785424456\
                    &__frame_v=manifest.ec369d2e020e53f6.json\
                    #seed=hwqcwzlhzanrdsdfzd&inputs=NNNNNEEEEEESS";
        let (level, inputs) = parse_replay_link(link).expect("a real pasted link");
        assert_eq!(level.seed, 18900);
        assert_eq!(inputs.len(), 13);
        assert_eq!(to_script(&inputs), "NNNNNEEEEEESS");
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
