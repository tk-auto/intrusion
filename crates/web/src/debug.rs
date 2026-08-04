//! The **debug session** (§12.6/#459) — the playtest channel, kept deliberately
//! separate from the level channel next door in [`seed`](crate::seed).
//!
//! A level is shared: it travels as a level-seed token in a link, a typed token or a
//! `window.__intrusionSeed` global, and everything it carries is part of what the run
//! *is* (#245). A [`DebugModifiers`] switch is the opposite of shared — it changes
//! only what the player perceives for whoever is watching, never the facility or the
//! guards, and it must never ride along with a level someone hands on. So it has its
//! own carrier and its own activation, and neither is ever a field of the level.
//!
//! # Two things, and only one of them is a switch
//!
//! **Debug *mode*** is whether this session has the help panel's Debug tab at all —
//! the surface the switches live on. **The switches** are [`DebugModifiers`]: what the
//! session starts with them set to. A build stamps both through the one global
//! `window.__intrusionDebug`, whose *presence* is the mode and whose value is the
//! comma-separated flag list (the artifact-build skill's `assemble.py`, which stamps it
//! on every artifact and adds flags for `--debug reveal`). The preview channel exists
//! for looking at the game, so its builds arrive with the tab already there.
//!
//! # The URL form, and why it is safe now when it was not before
//!
//! This module used to say there was deliberately no URL form, because a `?debug=`
//! parameter would make the fog liftable by anyone who read a link — which is exactly
//! what the level/debug split exists to prevent. #459 reverses that, deliberately: a
//! deployed build you cannot inspect is a build you cannot debug, and rebuilding to
//! look at a run that misbehaved on the live page is not a debugging loop.
//!
//! Three things keep the reversal honest, and all three are load-bearing:
//!
//! 1. **The parameter is a shibboleth, not a documented switch** — `?debug=intruded`,
//!    not `?debug=1`. A link-reader does not arrive at it by guessing.
//! 2. **It is stripped the moment it is consumed** ([`activate_from_url`]), with
//!    `history.replaceState`, so the address bar goes straight back to the clean
//!    `…#seed=<token>` link [`seed`](crate::seed) reflects. Activation is a thing you
//!    *do*, not a thing the page then carries: copy the URL after activating and you
//!    hand over the run, never the mode.
//! 3. **Nothing behind the gate may touch the facility.** The parameter is a
//!    convention, not a mechanism — anyone reading the shipped wasm can find the
//!    string — so what it unlocks has to be things that only alter the picture
//!    (§12.6). A switch that bent a rule would belong in the level-seed token with the
//!    rest of the run's identity, not here.

use intrusion_core::DebugModifiers;
use wasm_bindgen::JsValue;

/// The flag name for [`DebugModifiers::reveal_whole_level`] — the player's sight
/// becomes the whole facility. Kept beside the parser so the string the build stamps
/// and the string the shell reads are one fact.
const REVEAL: &str = "reveal";

/// The URL field that activates a debug session, and the one value it answers to
/// (#459). The value is the whole mitigation: `?debug=1` is the parameter a curious
/// reader tries, and it does nothing.
const DEBUG_FIELD: &str = "debug";
const ACTIVATION: &str = "intruded";

/// How this page boots its debug channel: whether this is a debug session at all, and
/// the switches it starts with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DebugBoot {
    /// Whether the help panel carries its Debug tab
    /// ([`ScreenUi::debug_mode`](intrusion_core::ScreenUi)).
    pub(crate) mode: bool,
    /// The switches the run starts under — the *initial value* of the panel's
    /// omni-vision toggle, not a fixed state (#459).
    pub(crate) flags: DebugModifiers,
}

/// Read the debug channel for this load: a **baked** build's global first, then the
/// URL's activation parameter — and, when the URL is what activated it, strip the
/// parameter before returning, so the address bar carries no trace of it.
///
/// A build that stamped the global wins outright, and its flags come with it. A page
/// with neither is the ordinary game: no tab, no switches, and a panel identical to the
/// one that shipped before this existed.
pub(crate) fn boot_debug() -> DebugBoot {
    if let Some(flags) = baked() {
        return DebugBoot { mode: true, flags };
    }
    DebugBoot {
        mode: activate_from_url(),
        flags: DebugModifiers::default(),
    }
}

/// The switches a *build* stamped into the page as `window.__intrusionDebug`, or
/// `None` when nothing stamped it. The global's **presence** is the mode — an empty
/// value is a debug session with every switch off, which is what an artifact built
/// without `--debug` gets.
fn baked() -> Option<DebugModifiers> {
    let window = web_sys::window()?;
    let list = js_sys::Reflect::get(&window, &JsValue::from_str("__intrusionDebug"))
        .ok()?
        .as_string()?;
    Some(flags_from(&list))
}

/// Parse the baked flag list — comma-separated names, whitespace tolerated, unknown
/// names ignored (a build stamped by a newer `assemble.py` degrades to the switches
/// this shell knows rather than refusing to boot).
fn flags_from(list: &str) -> DebugModifiers {
    let named = |name: &str| list.split(',').any(|flag| flag.trim() == name);
    DebugModifiers {
        reveal_whole_level: named(REVEAL),
    }
}

/// Whether the page URL activated a debug session — and, if it did, **strip the
/// parameter** (#459).
///
/// The strip is the guard the whole URL form rests on. `seed::reflect_level` reflects
/// the live run into the hash to keep the address bar shareable, and a query parameter
/// survives that untouched; without the strip, "copy the URL and send it to someone"
/// would hand over the debug session along with the run. So the parameter is consumed
/// exactly once, at boot, and `history.replaceState` puts the address bar back to the
/// page's own path — no reload, no navigation, nothing for the run to notice.
///
/// A wrong or absent value activates nothing **and is left alone**: it is somebody
/// else's parameter, and the strip is a consequence of consuming this one.
fn activate_from_url() -> bool {
    let Some(location) = web_sys::window().map(|w| w.location()) else {
        return false;
    };
    let query = location.search().unwrap_or_default();
    if !activates(&query) {
        return false;
    }
    strip_parameter(&query);
    true
}

/// Whether a `?a=b&…` query carries the activation — the exact value and nothing else
/// (#459), read through the same field parser the seed reader uses so the two channels
/// agree about what a field even is.
fn activates(query: &str) -> bool {
    intrusion_core::field_in(query, DEBUG_FIELD) == Some(ACTIVATION)
}

/// Rewrite the address bar without the activation parameter, best-effort: a host that
/// refuses the call (a sandboxed frame) simply keeps the URL it had, and the session is
/// already activated either way.
fn strip_parameter(query: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let location = window.location();
    let path = location.pathname().unwrap_or_default();
    let hash = location.hash().unwrap_or_default();
    let url = format!("{path}{}{hash}", strip_field(query, DEBUG_FIELD));
    if let Ok(history) = window.history() {
        let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&url));
    }
}

/// A `?a=b&…` query with every `name=…` field removed, re-spelled as a query (leading
/// `?`) or as the empty string when nothing is left — so a page whose only parameter
/// was the activation ends up with a bare path rather than a dangling `?`.
///
/// Pure, so the shape of what lands in the address bar is pinned by a native test
/// rather than by a browser.
fn strip_field(query: &str, name: &str) -> String {
    let kept: Vec<&str> = query
        .trim_start_matches('?')
        .split('&')
        .filter(|pair| !pair.is_empty())
        // The field's *name* is what identifies it, so a valued `debug=…` and a bare
        // `debug` both go — leaving a bare one behind would leave the tell.
        .filter(|pair| pair.split('=').next() != Some(name))
        .collect();
    if kept.is_empty() {
        String::new()
    } else {
        format!("?{}", kept.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list parses by name, tolerates spacing and companions, and ignores what it
    /// does not know — while a page with no baked global gets the plain game.
    #[test]
    fn the_flag_list_is_read_by_name() {
        assert!(flags_from(REVEAL).reveal_whole_level);
        assert!(flags_from(" reveal ,something-later").reveal_whole_level);
        assert!(!flags_from("").reveal_whole_level);
        assert!(
            !flags_from("revealed").reveal_whole_level,
            "no prefix match"
        );
        assert!(!flags_from("something-later").reveal_whole_level);
        assert_eq!(flags_from(""), DebugModifiers::default());
    }

    /// **The parameter is a shibboleth** (#459): only the exact value activates, so a
    /// reader who tries the obvious `?debug=1` gets the ordinary game. The field is
    /// read among others, in either position, because a hosted page's URL carries the
    /// host's own parameters.
    #[test]
    fn only_the_activation_value_opens_a_debug_session() {
        assert!(activates("?debug=intruded"));
        assert!(activates("?__frame_t=17&debug=intruded"));
        assert!(activates("?debug=intruded&seed=prbjdokbxcqgjnrnco"));

        assert!(!activates("?debug=1"), "the obvious guess does nothing");
        assert!(!activates("?debug="), "and an empty value is not a value");
        assert!(!activates("?debug"), "nor a bare field");
        assert!(!activates("?debug=Intruded"), "the value is exact");
        assert!(!activates("?debugging=intruded"), "no prefix match");
        assert!(!activates("?seed=prbjdokbxcqgjnrnco"));
        assert!(!activates(""));
    }

    /// **Activation leaves no trace in the address bar** (#459): the parameter is
    /// stripped whole, whatever sat around it, and a query that held nothing else
    /// becomes no query at all rather than a dangling `?`. This is what keeps "copy the
    /// URL and send it" handing over a level and never a debug session.
    #[test]
    fn the_activation_parameter_is_stripped_whole() {
        assert_eq!(strip_field("?debug=intruded", DEBUG_FIELD), "");
        assert_eq!(strip_field("?debug=intruded&a=1", DEBUG_FIELD), "?a=1");
        assert_eq!(strip_field("?a=1&debug=intruded", DEBUG_FIELD), "?a=1");
        assert_eq!(
            strip_field("?a=1&debug=intruded&b=2", DEBUG_FIELD),
            "?a=1&b=2"
        );
        // A bare `?debug` (no value) is the same field and goes with it.
        assert_eq!(strip_field("?debug&a=1", DEBUG_FIELD), "?a=1");
        // Everything else is somebody else's parameter and survives untouched.
        assert_eq!(strip_field("?a=1&b=2", DEBUG_FIELD), "?a=1&b=2");
        assert_eq!(strip_field("", DEBUG_FIELD), "");
        assert_eq!(strip_field("?", DEBUG_FIELD), "");
    }

    /// The two carriers are separate on purpose: a *level* can never turn a debug
    /// session on, and the debug global is never read as a level (§12.6/#245). The
    /// field names alone are the guard, so they are pinned here.
    #[test]
    fn a_level_link_carries_no_debug_state() {
        let level = intrusion_core::LevelSeed::quick_play(8371);
        let token = level.encode().expect("a config a run can hold");
        assert!(!activates(&format!("?seed={token}")));
        assert!(!activates(&format!("#seed={token}&inputs=ssww")));
        // …and the reflected link a run leaves in the address bar is a level alone.
        assert_eq!(
            crate::seed::level_in(&format!("#seed={token}")),
            Some(level)
        );
        assert!(!activates(&format!("#seed={token}")));
    }
}
