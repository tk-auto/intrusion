//! Level sharing (§13.1 / §12.4 / #110 / #244 / #245): **where a level comes from and
//! where its token goes**, so a specific interesting level can be handed around and
//! replayed exactly. "Try `prbjdokbxcqgjnrnco`, it's brutal" becomes a real handoff — the
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
    LevelSeed::quick_play(LevelSeed::narrow_seed(js_sys::Date::now() as u64))
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
/// the run it has, and the seed prompt needs no URL at all. A config with no token
/// (one no run can hold, #333) leaves the hash alone for the same reason — better a
/// stale address bar than one naming a run nobody can boot.
pub(crate) fn reflect_level(level: &LevelSeed) {
    let (Some(location), Some(token)) = (web_sys::window().map(|w| w.location()), level.encode())
    else {
        return;
    };
    let _ = location.set_hash(&format!("seed={token}"));
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
}
