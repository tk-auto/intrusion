//! Level sharing (§13.1 / §12.4 / #110 / #244 / #245): **where a level comes from and
//! where its token goes**, so a specific interesting level can be handed around and
//! replayed exactly. "Try `L1-8371-…`, it's brutal" becomes a real handoff — the
//! player types the token into the menu's seed prompt (or opens a `…#seed=<token>`
//! link) and gets the **same** run the token names, because the shell and the headless
//! sim boot the identical path ([`start_level`], §13.2).
//!
//! The seam is deliberately thin (§12.1): the core owns the whole reproducible config
//! and its serialisation — a [`LevelSeed`] is `(seed, modifiers, abilities)` (#245),
//! and [`LevelSeed::encode`]/[`decode`](LevelSeed::decode) are the *only* place the
//! string form is defined. This module just **reads and writes that string** — from a
//! baked-in build global and the URL, back out to the URL — and never touches game
//! logic. The *surface* that takes a typed token is the menu's seed prompt
//! ([`crate::menu`], #268); this is the plumbing under it.
//!
//! A bare decimal seed still works everywhere (backward compatible, #110): it decodes
//! to quick play (#244), so every existing `?seed=N` link and typed number keeps
//! reproducing its level. Two ways a level reaches the boot **explicitly** (see
//! [`explicit_level`]): a **baked** `window.__intrusionSeed` the build stamped in, or
//! a `?seed=`/`#seed=` in the **URL**. Absent both, the load has no run in mind and
//! opens on the menu ([`crate::menu`]), which rolls its own seed off the clock when
//! the player chooses one — so a shared link still boots straight into its run, and
//! only a bare load stops at the front door (#268).

use intrusion_core::LevelSeed;
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
/// `assemble.py --seed N`). Read before the URL and the clock so the baked value
/// always wins; absent (the normal build) it is simply `None`. Tolerates the value
/// being a JS string (a full level-seed string or a bare seed) or a number (a bare
/// seed), decoded through the one core parser.
fn baked_level() -> Option<LevelSeed> {
    let window = web_sys::window()?;
    let value = js_sys::Reflect::get(&window, &JsValue::from_str("__intrusionSeed")).ok()?;
    if let Some(text) = value.as_string() {
        return LevelSeed::decode(&text);
    }
    value
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map(|n| LevelSeed::quick_play(n as u64))
}

/// A fresh quick-play run off the wall clock (#244) — the seed is read **once** here
/// and threaded through the generation stream, never per-call: one seeded source per
/// run (§12.4). The shell's one impurity (§12.1), and what the menu's Quick play
/// rolls when the player asks for a new facility.
pub(crate) fn random_level() -> LevelSeed {
    LevelSeed::quick_play(js_sys::Date::now() as u64)
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
/// decode it as a level-seed string ([`LevelSeed::decode`]). Tolerates other fields
/// around it (the shared `inputs=` among them) and a leading `?`/`#`.
fn level_in(fragment: &str) -> Option<LevelSeed> {
    fragment
        .trim_start_matches(['?', '#'])
        .split('&')
        .find_map(|pair| pair.strip_prefix("seed="))
        .and_then(LevelSeed::decode)
}

/// Reflect the active level into the URL hash, so the address bar is always a
/// shareable `…#seed=<token>` link and a reload replays the same run — the surface
/// the run's own token stays readable and copyable from (#110), alongside the help
/// card's Level info tab (#281). Called the moment a run starts, never while the menu
/// is up: an address bar that named a level nobody had chosen to play would be a lie.
///
/// Best-effort: a host that refuses the navigation (a sandboxed frame) simply keeps
/// the run it has, and the seed prompt needs no URL at all.
pub(crate) fn reflect_level(level: &LevelSeed) {
    if let Some(location) = web_sys::window().map(|w| w.location()) {
        let _ = location.set_hash(&format!("seed={}", level.encode()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The URL reader accepts a level from either the query or the hash, tolerates
    /// other fields around it, and rejects non-decodable or absent values — the
    /// graceful-fallback branch (#110) the boot turns into a fresh quick-play run. A
    /// bare seed still decodes (backward compatible), as does the structured token.
    #[test]
    fn a_level_is_read_from_a_query_or_hash_fragment() {
        assert_eq!(level_in("?seed=8371"), Some(LevelSeed::quick_play(8371)));
        assert_eq!(level_in("#seed=8371"), Some(LevelSeed::quick_play(8371)));
        assert_eq!(
            level_in("?foo=1&seed=42&bar=2"),
            Some(LevelSeed::quick_play(42))
        );
        assert_eq!(level_in("#a=1&seed=0"), Some(LevelSeed::quick_play(0)));
        // A structured token round-trips through the URL surface too.
        let token = LevelSeed::sim(7).encode();
        assert_eq!(
            level_in(&format!("#seed={token}")),
            Some(LevelSeed::sim(7)),
            "the structured token survives the URL surface",
        );
        assert_eq!(level_in("?seed=notatoken"), None);
        assert_eq!(level_in("#nothinghere"), None);
        assert_eq!(level_in(""), None);
    }
}
