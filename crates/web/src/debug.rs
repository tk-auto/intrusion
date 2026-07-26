//! Debug switches baked into a **build** (§12.6) — the playtest channel, kept
//! deliberately separate from the level channel next door in [`seed`](crate::seed).
//!
//! A level is shared: it travels as a level-seed string in a link, a typed token or a
//! `window.__intrusionSeed` global, and everything it carries is part of what the run
//! *is* (#245). A [`DebugModifiers`] switch is the opposite of shared — it changes
//! only what the player perceives for whoever is watching, never the facility or the
//! guards, and it must never ride along with a level someone hands on. So it has its own
//! carrier, `window.__intrusionDebug`, which **only a build can stamp**: the
//! artifact-build skill's `assemble.py --debug reveal`. There is deliberately no URL
//! form and no in-game surface — a `?debug=` parameter would make the fog liftable by
//! anyone who read a link, which is exactly what the split exists to prevent.
//!
//! The global is a comma-separated list of flag names (`"reveal"`), so a second switch
//! is a name in the list rather than a second global. An unknown name is ignored here;
//! `assemble.py` validates against the same set at build time, so a typo fails loudly
//! where it is typed rather than silently doing nothing in the browser.

use intrusion_core::DebugModifiers;
use wasm_bindgen::JsValue;

/// The flag name for [`DebugModifiers::reveal_whole_level`] — the player's sight
/// becomes the whole facility. Kept beside the parser so the string the build stamps
/// and the string the shell reads are one fact.
const REVEAL: &str = "reveal";

/// The debug switches this build was baked with, all off when nothing stamped the
/// global (every real build, and every shared link).
pub(crate) fn baked_debug() -> DebugModifiers {
    let Some(window) = web_sys::window() else {
        return DebugModifiers::default();
    };
    js_sys::Reflect::get(&window, &JsValue::from_str("__intrusionDebug"))
        .ok()
        .and_then(|value| value.as_string())
        .map(|list| flags_from(&list))
        .unwrap_or_default()
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
}
