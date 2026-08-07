//! Level sharing (§13.1 / §12.4 / #110 / #244 / #245 / #572): **where a level comes
//! from and where its link goes**, so a specific interesting level can be handed
//! around and replayed exactly.
//!
//! **Sharing is the URL** (#572). "Try this one, it's brutal" is a `…#seed=<token>`
//! link the recipient clicks, and it boots the **same** run the token names because
//! the shell and the headless sim boot the identical path ([`start_level`], §13.2).
//! There is nowhere to type a token: the menu's seed prompt and its DOM text box are
//! gone, so the token's remaining job is to be *displayed* — short enough to read off
//! a character grid and reconstruct a link from by hand when one arrives mangled.
//!
//! The seam is deliberately thin (§12.1): the core owns the whole reproducible config
//! and its serialisation — a [`LevelSeed`] is `(seed, modifiers, abilities)` (#245),
//! [`LevelSeed::encode`]/[`decode`](LevelSeed::decode) are the *only* place the string
//! form is defined, and [`level_fragment`](intrusion_core::level_fragment) is the only
//! place the field around it is. This module just **reads and writes that string** —
//! from a baked-in build global and the URL, back out to the URL and onto the
//! clipboard — and never touches game logic.
//!
//! Every link this module writes is built on [`page_base`], which is the page's own
//! address with **both** the fragment and the query stripped. That is what keeps a
//! shared link clean: a session activated with `?debug=intruded` (§12.6) or opened
//! from someone else's `…#seed=…&inputs=…` must not pass either on, and the strip is
//! one function rather than a rule each caller has to remember.
//!
//! A **bare decimal seed is not a level** (#333): it named this build's quick-play
//! preset applied to a number rather than a run, so a link carrying one re-resolved
//! into a different run whenever the preset moved. Old `?seed=N` links therefore stop
//! decoding — they fall to the menu like any unreadable value, which is the honest
//! outcome, since what they named had already drifted. Two ways a level reaches the
//! boot **explicitly** (see
//! [`explicit_level`]): a **baked** `window.__intrusionSeed` the build stamped in, or
//! a `?seed=`/`#seed=` in the **URL**. Absent both, the load has no run in mind and
//! opens on the menu ([`crate::menu`]), which rolls its own seed off the clock when
//! the player chooses one — so a shared link still boots straight into its run, and
//! only a bare load stops at the front door (#268).

use intrusion_core::{level_fragment, Difficulty, LevelSeed};
use wasm_bindgen::JsValue;

/// The level this load was **told** to play, or `None` when nothing named one. Two
/// sources, in priority order:
///
/// 1. a **baked-in** token ([`baked_level`]) — a `window.__intrusionSeed` the build
///    stamped in, so a seed-locked artifact boots that run with no URL and no typing
///    (the artifact host strips a `…#seed=<token>` hash before the page sees it, so a
///    shared *link* can't reach the framed page — a baked build can);
/// 2. an explicit `seed=` in the page **URL** ([`level_from_url`]) — a shared link on
///    a host that passes the hash through, e.g. the Pages deploy.
///
/// An unparseable value at either step falls through to the next, and a load with
/// neither answers `None`: the boot then opens on the menu (#268) rather than
/// guessing, so a bad token can never brick the page (#110) — it simply lands the
/// player at the front door.
pub(crate) fn explicit_level() -> Option<LevelSeed> {
    baked_level().or_else(level_from_url)
}

/// A level the *build* stamped into the page as a `window.__intrusionSeed` global —
/// how a seed-locked artifact pins its run (the artifact-build skill's
/// `assemble.py --seed <token>`). Read before the URL and the clock so the baked
/// value always wins; absent (the normal build) it is simply `None`.
///
/// The baked value is a **token string** and nothing else (#333). A bare number used
/// to be accepted here too, which meant a seed-locked artifact pinned a *preset*
/// rather than a run — the artifact would drift with the build it was rebuilt from.
/// Decoded through the one core parser, so the artifact and a shared link agree.
fn baked_level() -> Option<LevelSeed> {
    let window = web_sys::window()?;
    let value = js_sys::Reflect::get(&window, &JsValue::from_str("__intrusionSeed")).ok()?;
    LevelSeed::decode(&value.as_string()?)
}

/// A fresh quick-play run off the wall clock (#244) — the seed is read **once** here
/// and threaded through the generation stream, never per-call: one seeded source per
/// run (§12.4). The shell's one impurity (§12.1), and what the menu's Quick play
/// rolls when the player asks for a new facility.
///
/// The clock reading is **narrowed to the token's seed width** (#333). It is far
/// wider than that field — milliseconds since the epoch is past forty bits — and a
/// run the token cannot express is a run that cannot be shared or reflected into the
/// address bar. Every run the shell creates has to be sayable, so the narrowing
/// happens here, at the source, rather than being discovered downstream.
pub(crate) fn random_level() -> LevelSeed {
    random_level_at(Difficulty::Standard)
}

/// A fresh quick-play run off the wall clock at a chosen **difficulty** (§12.6/#298)
/// — what the level-options dialog's *Play* control rolls.
///
/// The clock is read here and nowhere else, so the difficulty draw and the facility
/// take the *same* seed and the run stays one seeded source (§12.4). The draw resolves
/// before the run boots, so what the returned [`LevelSeed`] carries — and what the
/// address bar then reflects — is the resolved modifier set: a link shared from a run
/// started at `Much harder` hands over that run, not a re-roll at that setting.
pub(crate) fn random_level_at(difficulty: Difficulty) -> LevelSeed {
    LevelSeed::quick_play_at(clock_seed(), difficulty)
}

/// A fresh run's seed, off the wall clock and **narrowed to the token's seed width**
/// (#333) — the shell's one impurity (§12.1), named so the callers that need a seed
/// without needing a whole preset can share it.
///
/// The end screen's *new run* is the second such caller (§14 v2/#138): what a fresh
/// run *is* — the preset, the options carried over — is the core's rule
/// ([`EndExit::level`](intrusion_core::EndExit::level)), and the only thing it cannot
/// supply is the reading of the clock.
pub(crate) fn clock_seed() -> u64 {
    LevelSeed::narrow_seed(js_sys::Date::now() as u64)
}

/// Read a level from the page URL — `?seed=<token>` first, then `#seed=<token>` — or
/// `None` when neither carries a decodable value. The hash form works on any static
/// host (it never reaches a server), which is what lets a shared `…#seed=<token>`
/// link reproduce a level even from a single-file page.
fn level_from_url() -> Option<LevelSeed> {
    let location = web_sys::window()?.location();
    let query = location.search().ok();
    let hash = location.hash().ok();
    query
        .as_deref()
        .and_then(level_in)
        .or_else(|| hash.as_deref().and_then(level_in))
}

/// Find a `seed=<value>` field in a `?a=b&…` query or `#a=b&…` hash fragment and
/// decode it as a level-seed token ([`LevelSeed::decode`]). Tolerates other fields
/// around it (the shared `inputs=` among them) and a leading `?`/`#`. `pub(crate)`
/// so the copy-replay round trip ([`crate::replay`], #411) is asserted against this
/// real reader rather than a test's re-implementation of it.
pub(crate) fn level_in(fragment: &str) -> Option<LevelSeed> {
    intrusion_core::field_in(fragment, "seed").and_then(LevelSeed::decode)
}

/// Reflect the active level into the URL hash, so the address bar is always a
/// shareable `…#seed=<token>` link and a reload replays the same run — the surface
/// the run's own token stays readable and copyable from (#110), alongside the help
/// card's Level info tab (#281). Called the moment a run starts, never while the menu
/// is up: an address bar that named a level nobody had chosen to play would be a lie.
///
/// Best-effort: a host that refuses the navigation (a sandboxed frame) simply keeps
/// the run it has — the panel's `copy [c]` is the surface that matters for sharing,
/// and it builds its own link. A config with no token (one no run can hold, #333)
/// leaves the hash alone for the same reason — better a stale address bar than one
/// naming a run nobody can boot.
pub(crate) fn reflect_level(level: &LevelSeed) {
    let (Some(location), Some(token)) = (web_sys::window().map(|w| w.location()), level.encode())
    else {
        return;
    };
    let _ = location.set_hash(&level_fragment(&token));
}

/// **The link the Level info tab's `copy [c]` hands over** (§13.1/#353/#572): this
/// page's own address with a fresh `#seed=<token>` on it, so what the player copies is
/// something the person they send it to can *open*.
///
/// It replaces the bare token that control used to write. A token was a fair thing to
/// copy while the menu had a box to paste it into; with the box gone (#572) it is a
/// string the recipient can do nothing with but retype into a URL by hand, which is
/// the step that gets done wrong. The panel still **prints** the token one row above —
/// display is the short form, the copy is the link, and that split is the whole
/// decision.
///
/// The base is [`page_base`]'s, so the link is the origin and path alone: **no
/// `?debug=` activation and no inherited `&inputs=` script** ride along with a level
/// somebody is being invited to play (§12.6 — a shared link never carries a session).
///
/// `None` when the page has no usable address (a hostless test harness) or the run has
/// no token to name (#333) — the control is not drawn in the second case anyway.
pub(crate) fn level_url(token: &str) -> Option<String> {
    Some(format!("{}#{}", page_base()?, level_fragment(token)))
}

/// This page's own address, **fragment and query stripped** — the base every link the
/// shell writes is built on ([`level_url`], [`crate::replay::replay_url`]).
///
/// Both halves have to go, and for different reasons: the **fragment** is whatever run
/// the page is currently showing, which a fresh link is about to replace, and the
/// **query** is how a debug session was activated (§12.6/#478). Neither belongs to the
/// person on the other end. The strip is plain string work on `href` rather than
/// `origin` + `pathname`, so it behaves the same on the hosts that have no meaningful
/// origin — a framed artifact, a `file://` open — as it does on the Pages deploy.
pub(crate) fn page_base() -> Option<String> {
    let href = web_sys::window()?.location().href().ok()?;
    Some(base_of(&href).to_string())
}

/// The base of `href`: everything before the first `#` or `?`, whichever comes first.
/// Pure, so the one rule that keeps a shared link clean is pinned by a native test
/// rather than by reading the browser's answer back.
pub(crate) fn base_of(href: &str) -> &str {
    let cut = href.find(['#', '?']).unwrap_or(href.len());
    &href[..cut]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The URL reader accepts a level from either the query or the hash, tolerates
    /// other fields around it, and rejects non-decodable or absent values — the
    /// graceful-fallback branch (#110) the boot turns into a fresh run.
    #[test]
    fn a_level_is_read_from_a_query_or_hash_fragment() {
        let level = LevelSeed::quick_play(8371);
        let token = level.encode().expect("a config a run can hold");
        assert_eq!(level_in(&format!("?seed={token}")), Some(level));
        assert_eq!(level_in(&format!("#seed={token}")), Some(level));
        assert_eq!(level_in(&format!("?foo=1&seed={token}&bar=2")), Some(level));

        // A non-default config survives the URL surface identically — one form.
        let sim = LevelSeed::sim(7);
        let sim_token = sim.encode().expect("a config a run can hold");
        assert_eq!(level_in(&format!("#a=1&seed={sim_token}")), Some(sim));

        // A bare seed is no longer a level (#333): the old link falls through to the
        // menu rather than resolving to whatever this build's preset means today.
        assert_eq!(level_in("?seed=8371"), None, "the old bare-seed link");
        assert_eq!(level_in("#seed=8371"), None);
        assert_eq!(level_in("?seed=notatoken"), None);
        assert_eq!(level_in("#nothinghere"), None);
        assert_eq!(level_in(""), None);
    }

    /// **The copied link round-trips** (§13.1/#572): what `copy [c]` writes, pasted
    /// into an address bar, boots that exact level — asserted through the *real* boot
    /// reader ([`level_in`]) rather than a test's re-spelling of it, so the two halves
    /// of the handoff cannot drift apart.
    #[test]
    fn the_copied_link_opens_the_level_it_names() {
        for level in [
            LevelSeed::quick_play(8371),
            LevelSeed::sim(7),
            LevelSeed::quick_play_at(42, Difficulty::Standard),
        ] {
            let token = level.encode().expect("a config a run can hold");
            let link = format!(
                "{}#{}",
                "https://tk-auto.github.io/intrusion/",
                level_fragment(&token)
            );
            let hash = &link[link.find('#').expect("the link has a fragment")..];
            assert_eq!(
                level_in(hash),
                Some(level),
                "the link a player copies does not open its own level: {link}",
            );
        }
    }

    /// **A shared link is clean** (§12.6/#572): the base is the page's origin and path
    /// alone, so neither a `?debug=` activation nor the `&inputs=` of a replay the
    /// player happens to be watching can ride along with a level someone is being
    /// invited to play.
    ///
    /// Walked over the shapes the address bar actually holds — a fresh load, a run
    /// reflected into the hash, a debug session, a pasted replay link, and a host's own
    /// query in front of all of it.
    #[test]
    fn a_shared_link_carries_no_session_and_no_replay() {
        let deploy = "https://tk-auto.github.io/intrusion/";
        for href in [
            deploy.to_string(),
            format!("{deploy}#seed=prbjdokbxcqgjnrnco"),
            format!("{deploy}?debug=intruded"),
            format!("{deploy}?debug=intruded#seed=prbjdokbxcqgjnrnco&inputs=NNE.."),
            format!("{deploy}?__frame_t=1730#seed=prbjdokbxcqgjnrnco&inputs=NNE.."),
        ] {
            assert_eq!(base_of(&href), deploy, "the base of {href}");
        }

        // …and the link built on it carries the token and nothing else.
        let token = "prbjdokbxcqgjnrnco";
        let dirty = format!("{deploy}?debug=intruded#seed=other&inputs=NNE..");
        let link = format!("{}#{}", base_of(&dirty), level_fragment(token));
        assert_eq!(link, format!("{deploy}#seed={token}"));
        for leaked in ["debug", "inputs", "__frame_t"] {
            assert!(!link.contains(leaked), "{link} leaks `{leaked}`");
        }

        // A page served from a path, and one with no meaningful origin at all — the
        // framed artifact and a `file://` open — keep their path rather than losing it.
        assert_eq!(
            base_of("http://localhost:8000/dist/?a=1"),
            "http://localhost:8000/dist/"
        );
        assert_eq!(
            base_of("file:///tmp/intrusion.html#seed=x"),
            "file:///tmp/intrusion.html"
        );
        assert_eq!(base_of(""), "");
    }
}
