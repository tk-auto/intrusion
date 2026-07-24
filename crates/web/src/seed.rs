//! Seed sharing (§13.1 / §12.4 / #110): surface the run seed and let the player
//! load another, so a specific interesting level can be handed around and replayed
//! exactly. "Try seed 8371, it's brutal" becomes a real handoff — the sim prints a
//! seed, the player types it into the on-page box (or opens a `…#seed=8371` link)
//! and gets the **same** facility the bot played, because the shell and the headless
//! sim boot the identical path (§13.2).
//!
//! The seam is deliberately thin (§12.1): the core already guarantees a seed
//! reproduces a facility (`Rng::new(seed)` → generation → turn loop, one seed per
//! run) and owns every pixel of the board. This module only **reads and writes the
//! seed string** — from a baked-in build global, the URL, and the on-page bar — and
//! rebuilds the run through the same [`Game::reseed`](crate::Game::reseed)/
//! [`new_run`](crate::new_run) the boot uses. It never touches game logic, and the
//! seed bar's markup and styling live in `web/index.html`; here is only the wiring.
//!
//! Three ways a seed reaches the boot, in priority order (see [`initial_seed`]): a
//! **baked** `window.__intrusionSeed` the build stamped in (a seed-locked artifact),
//! then a `?seed=`/`#seed=` in the **URL** (a shared link where the host passes the
//! hash — e.g. the Pages deploy), then the **clock**. The bar loads any seed live.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlInputElement, KeyboardEvent, MouseEvent, PointerEvent};

use crate::Game;

/// The seed a fresh load starts from, in priority order:
///
/// 1. a **baked-in** seed ([`baked_seed`]) — a `window.__intrusionSeed` the build
///    stamped in, so a seed-locked artifact boots that facility with no URL and no
///    typing (the artifact host strips a `…#seed=N` hash before the page sees it,
///    so a shared *link* can't reach the framed page — a baked build can);
/// 2. an explicit `seed=` in the page **URL** ([`seed_from_url`]) — a shared link on
///    a host that passes the hash through, e.g. the Pages deploy;
/// 3. otherwise a fresh one off the **clock** — the shell's one impurity (§12.1).
///
/// An unparseable or absent value at every step falls through to the next, and an
/// empty box later rolls a fresh seed, so the run never errors on a bad seed (#110).
pub(crate) fn initial_seed() -> u64 {
    baked_seed()
        .or_else(seed_from_url)
        .unwrap_or_else(random_seed)
}

/// A seed the *build* stamped into the page as a `window.__intrusionSeed` global —
/// how a seed-locked artifact pins its facility (the artifact-build skill's
/// `assemble.py --seed N`). Read before the URL and the clock so the baked seed
/// always wins; absent (the normal build) it is simply `None`. Tolerates the value
/// being a JS string or a number.
fn baked_seed() -> Option<u64> {
    let window = web_sys::window()?;
    let value = js_sys::Reflect::get(&window, &JsValue::from_str("__intrusionSeed")).ok()?;
    if let Some(text) = value.as_string() {
        return parse_seed(&text);
    }
    value
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map(|n| n as u64)
}

/// A fresh seed off the wall clock — read **once** here and threaded through the
/// generation stream, never per-call: one seeded source per run (§12.4).
fn random_seed() -> u64 {
    js_sys::Date::now() as u64
}

/// Read a seed from the page URL — `?seed=N` first, then `#seed=N` — or `None` when
/// neither carries a parseable value. The hash form works on any static host (it
/// never reaches a server), which is what lets a shared `…#seed=N` link reproduce a
/// level even from a single-file page.
fn seed_from_url() -> Option<u64> {
    let location = web_sys::window()?.location();
    let query = location.search().ok();
    let hash = location.hash().ok();
    query
        .as_deref()
        .and_then(seed_in)
        .or_else(|| hash.as_deref().and_then(seed_in))
}

/// Find a `seed=<value>` field in a `?a=b&…` query or `#a=b&…` hash fragment and
/// parse it. Tolerates other fields around it and a leading `?`/`#`.
fn seed_in(fragment: &str) -> Option<u64> {
    fragment
        .trim_start_matches(['?', '#'])
        .split('&')
        .find_map(|pair| pair.strip_prefix("seed="))
        .and_then(parse_seed)
}

/// Parse a seed string — from the URL or the box — as a trimmed decimal `u64`, or
/// `None`. The `None` is what the graceful fallback (#110) turns into a fresh seed.
fn parse_seed(raw: &str) -> Option<u64> {
    raw.trim().parse::<u64>().ok()
}

/// Reflect the active seed into the URL hash, so the address bar is always a
/// shareable `…#seed=N` link and a reload replays the same level. Best-effort: a
/// host that refuses the navigation (a sandboxed frame) just keeps the on-page box,
/// which needs no URL at all.
fn reflect_seed(seed: u64) {
    if let Some(location) = web_sys::window().map(|w| w.location()) {
        let _ = location.set_hash(&format!("seed={seed}"));
    }
}

/// Wire the on-page seed bar (§13.1/#110): show the seed the run booted from, and
/// let the player load another from the box. The bar's markup lives in
/// `web/index.html`; a page hosted without it (or before it) simply gets no bar —
/// every element is optional and the shell wires only what is present.
///
/// Two hazards the wiring has to disarm, both because the game's own listeners live
/// on the `document` (§11.6): a **press** on the bar must not start a walk (the
/// gesture pump would read it as a swipe), and a **keystroke** while typing a seed
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

    // Show the seed the run booted from, and normalise the URL to it so the address
    // bar is a shareable link from the first frame.
    let seed = game.borrow().seed;
    value.set_text_content(Some(&seed.to_string()));
    reflect_seed(seed);

    // Loading a seed: parse the box (empty or invalid → a fresh random seed, #110),
    // rebuild the run, then reflect the *effective* seed into the display and URL.
    let load: Rc<dyn Fn()> = {
        let game = game.clone();
        let input = input.clone();
        let value = value.clone();
        Rc::new(move || {
            let seed = parse_seed(&input.value()).unwrap_or_else(random_seed);
            if game.borrow_mut().reseed(seed).is_ok() {
                value.set_text_content(Some(&seed.to_string()));
                input.set_value("");
                reflect_seed(seed);
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
    // the document key pump (§11.6) sees them, and let Enter submit the seed.
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

    /// The URL reader accepts a seed from either the query or the hash, tolerates
    /// other fields around it, and rejects non-numeric or absent values — the
    /// graceful-fallback branch (#110) the boot turns into a fresh random seed.
    #[test]
    fn a_seed_is_read_from_a_query_or_hash_fragment() {
        assert_eq!(seed_in("?seed=8371"), Some(8371));
        assert_eq!(seed_in("#seed=8371"), Some(8371));
        assert_eq!(seed_in("?foo=1&seed=42&bar=2"), Some(42));
        assert_eq!(seed_in("#a=1&seed=0"), Some(0));
        assert_eq!(seed_in("?seed=notanumber"), None);
        assert_eq!(seed_in("#nothinghere"), None);
        assert_eq!(seed_in(""), None);
    }

    /// The box parser is the same tolerant decimal `u64`: surrounding whitespace is
    /// trimmed, and anything that is not a plain number is `None` (→ fresh seed).
    #[test]
    fn a_typed_seed_is_a_trimmed_decimal_u64() {
        assert_eq!(parse_seed("  8371 "), Some(8371));
        assert_eq!(parse_seed("0"), Some(0));
        assert_eq!(parse_seed(&u64::MAX.to_string()), Some(u64::MAX));
        assert_eq!(parse_seed(""), None);
        assert_eq!(parse_seed("   "), None);
        assert_eq!(parse_seed("-1"), None);
        assert_eq!(parse_seed("1.5"), None);
        assert_eq!(parse_seed("0x20"), None);
    }
}
