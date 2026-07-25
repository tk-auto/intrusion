//! Level sharing (§13.1 / §12.4 / #110 / #244 / #245): surface the run's level-seed
//! string and let the player load another, so a specific interesting level can be
//! handed around and replayed exactly. "Try `L1-8371-…`, it's brutal" becomes a real
//! handoff — the shell shows the token, the player types it into the on-page box (or
//! opens a `…#seed=<token>` link) and gets the **same** run the token names, because
//! the shell and the headless sim boot the identical path ([`start_level`], §13.2).
//!
//! The seam is deliberately thin (§12.1): the core owns the whole reproducible config
//! and its serialisation — a [`LevelSeed`] is `(seed, modifiers, abilities)` (#245),
//! and [`LevelSeed::encode`]/[`decode`](LevelSeed::decode) are the *only* place the
//! string form is defined. This module just **reads and writes that string** — from a
//! baked-in build global, the URL, and the on-page bar — and rebuilds the run through
//! the same [`Game::reseed`](crate::Game::reseed)/[`new_run`](crate::new_run) the boot
//! uses. It never touches game logic, and the seed bar's markup and styling live in
//! `web/index.html`; here is only the wiring.
//!
//! A bare decimal seed still works everywhere (backward compatible, #110): it decodes
//! to quick play (#244), so every existing `?seed=N` link and typed number keeps
//! reproducing its level. Three ways a level reaches the boot, in priority order (see
//! [`initial_level`]): a **baked** `window.__intrusionSeed` the build stamped in, then
//! a `?seed=`/`#seed=` in the **URL**, then the **clock** (a fresh quick-play run).

use std::cell::RefCell;
use std::rc::Rc;

use intrusion_core::LevelSeed;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlInputElement, KeyboardEvent, MouseEvent, PointerEvent};

use crate::Game;

/// The level a fresh load starts from, in priority order:
///
/// 1. a **baked-in** token ([`baked_level`]) — a `window.__intrusionSeed` the build
///    stamped in, so a seed-locked artifact boots that run with no URL and no typing
///    (the artifact host strips a `…#seed=<token>` hash before the page sees it, so a
///    shared *link* can't reach the framed page — a baked build can);
/// 2. an explicit `seed=` in the page **URL** ([`level_from_url`]) — a shared link on
///    a host that passes the hash through, e.g. the Pages deploy;
/// 3. otherwise a fresh **quick-play** run off the **clock** — the shell's one
///    impurity (§12.1).
///
/// An unparseable or absent value at every step falls through to the next, and an
/// empty box later rolls a fresh seed, so the run never errors on a bad token (#110).
pub(crate) fn initial_level() -> LevelSeed {
    baked_level()
        .or_else(level_from_url)
        .unwrap_or_else(random_level)
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
/// run (§12.4).
fn random_level() -> LevelSeed {
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
/// shareable `…#seed=<token>` link and a reload replays the same run. Best-effort: a
/// host that refuses the navigation (a sandboxed frame) just keeps the on-page box,
/// which needs no URL at all.
fn reflect_level(level: &LevelSeed) {
    if let Some(location) = web_sys::window().map(|w| w.location()) {
        let _ = location.set_hash(&format!("seed={}", level.encode()));
    }
}

/// Wire the on-page seed bar (§13.1/#110/#245): show the level-seed string the run
/// booted from, and let the player load another from the box. The bar's markup lives
/// in `web/index.html`; a page hosted without it (or before it) simply gets no bar —
/// every element is optional and the shell wires only what is present.
///
/// Two hazards the wiring has to disarm, both because the game's own listeners live
/// on the `document` (§11.6): a **press** on the bar must not start a walk (the
/// gesture pump would read it as a swipe), and a **keystroke** while typing a token
/// must not drive the turn loop (the key pump would read `w`/arrows as moves). Both
/// are stopped from bubbling to the document before those pumps see them; nothing is
/// `preventDefault`ed, so the box still focuses and types normally.
pub(crate) fn install(document: &Document, game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    let value = document.get_element_by_id("seed-value");
    let input = document
        .get_element_by_id("seed-input")
        .and_then(|e| e.dyn_into::<HtmlInputElement>().ok());
    let go = document.get_element_by_id("seed-go");
    let bar = document.get_element_by_id("seedbar");
    let (Some(value), Some(input), Some(go), Some(bar)) = (value, input, go, bar) else {
        return Ok(()); // no seed-bar markup on this page — nothing to wire
    };

    // Show the level-seed string the run booted from, and normalise the URL to it so
    // the address bar is a shareable link from the first frame.
    let token = game.borrow().level.encode();
    value.set_text_content(Some(&token));
    reflect_level(&game.borrow().level);

    // Loading a token: decode the box (empty or invalid → a fresh quick-play run,
    // #110), rebuild the run, then reflect the *effective* level into the display and
    // URL.
    let load: Rc<dyn Fn()> = {
        let game = game.clone();
        let input = input.clone();
        let value = value.clone();
        Rc::new(move || {
            let level = LevelSeed::decode(&input.value()).unwrap_or_else(random_level);
            if game.borrow_mut().reseed(level).is_ok() {
                value.set_text_content(Some(&level.encode()));
                input.set_value("");
                reflect_level(&level);
            }
        })
    };

    // The play button.
    {
        let load = load.clone();
        let cb = Closure::<dyn FnMut(MouseEvent)>::new(move |_e: MouseEvent| load());
        go.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // Keys inside the bar are the box's own, never the game's: swallow them before
    // the document key pump (§11.6) sees them, and let Enter submit the token.
    {
        let load = load.clone();
        let cb = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
            e.stop_propagation();
            if e.key() == "Enter" {
                load();
            }
        });
        bar.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // A press on the bar is a UI interaction, not a swipe: keep it from the gesture
    // pump on the document, which would otherwise start walking the player.
    {
        let cb =
            Closure::<dyn FnMut(PointerEvent)>::new(move |e: PointerEvent| e.stop_propagation());
        bar.add_event_listener_with_callback("pointerdown", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    Ok(())
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
