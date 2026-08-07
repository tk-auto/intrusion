//! Handing a measured run back to a human (§13.1/#572): the **play link**.
//!
//! Everything else the sim prints is for a machine — one JSON row per run, a summary
//! row, the `{seed,inputs}` pair `--emit-replay` writes to stdout — and the playtest
//! skill parses all of it. But two of its outputs are prose meant to be read by a
//! person deciding what to go and play: `--inspect`'s narration and the one-line
//! summary `--emit-replay` puts on stderr. Those used to name a run by its token
//! alone, which was a fair handoff while the web build had a box to type one into.
//!
//! It hasn't since #572 — **sharing is the URL** — so a token offered to a human is
//! now a string they cannot do anything with except rebuild a link out of by hand.
//! This module builds the link instead.
//!
//! It is the sim's **one platform fact**. The web shell composes its links off the
//! address of the page it is running on ([`crate::…`] has no such thing: a native
//! binary is nowhere), so the base has to be named, and the only base worth naming is
//! where the game is actually published. The *format* around the token is still the
//! core's ([`level_fragment`]), shared with the address bar and the help panel's
//! `copy [c]`, so there is exactly one spelling of what a shared level looks like.

use intrusion_core::{level_fragment, LevelSeed};

/// Where a link the sim hands over points: the published build (`pages.yml`). A sim
/// run reproduces in the browser because both boot the identical path
/// ([`start_level`](intrusion_core::start_level), §12.1/§13.2), so the deploy will
/// play the run these numbers came from — which is the whole reason to print a link
/// beside them.
///
/// It carries its trailing slash, so a fragment appends cleanly.
pub const PLAY_BASE: &str = "https://tk-auto.github.io/intrusion/";

/// A link that opens `level` in the published build, or `None` for a config no token
/// can hold (#333) — the sim's presets are all well inside the §8.3 cap, so in
/// practice this is `Some`.
///
/// The link names the **level**, not the run: it hands over the facility, the
/// modifiers and the loadout the sim measured, and leaves the playing to the person
/// who clicked. A link that carries the bot's inputs too is a different thing and has
/// its own flag (`--emit-replay`, §12.4/#411).
pub fn play_link(level: &LevelSeed) -> Option<String> {
    Some(format!("{PLAY_BASE}#{}", level_fragment(&level.encode()?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use intrusion_core::parse_replay_link;

    /// **The link the sim prints opens the level it measured** — asserted through the
    /// core's own reader, the same one the browser boots a pasted link with, so the
    /// handoff cannot drift from the thing that receives it.
    #[test]
    fn a_play_link_opens_the_level_it_names() {
        for level in [
            LevelSeed::sim(0),
            LevelSeed::sim(61),
            LevelSeed::quick_play(8371),
        ] {
            let link = play_link(&level).expect("a sim preset always encodes");
            assert!(link.starts_with(PLAY_BASE), "{link} is not the deploy");
            assert_eq!(
                parse_replay_link(&link),
                Ok((level, Vec::new())),
                "{link} does not name the level it was built from",
            );
        }
    }

    /// The base is the deploy, spelled once and with its slash — a link missing it
    /// would point at the repository's parent path rather than at the game.
    #[test]
    fn the_play_base_is_the_deploy() {
        assert!(
            PLAY_BASE.ends_with('/'),
            "the base takes a fragment cleanly"
        );
        assert_eq!(
            play_link(&LevelSeed::sim(42)).as_deref(),
            Some("https://tk-auto.github.io/intrusion/#seed=nfdxttsytrdorexcqn"),
        );
    }
}
