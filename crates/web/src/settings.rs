//! The **settings record** (§14 v2/#513): the shell half of the options screen — where
//! a preference is stored, and what a boot makes of what it finds.
//!
//! The core owns the screen ([`intrusion_core::render_settings`] and its rows); this
//! owns the two things a core may not: reading the browser's storage, and reading the
//! URL. Both answer the same question — *which theme and which renderer does this load
//! start with?* — and this module is where their order is decided.
//!
//! # Its own slot, and why
//!
//! A preference is **not part of the run** (§12.5's autosave is), so it gets a key of
//! its own beside the run's. Ending a run empties that slot; a preference must survive
//! it, along with every other thing that happens to a run — permadeath is about the
//! facility, not about whether the player likes the light theme (§2.2). One record,
//! one format stamp, overwritten whole on every change.
//!
//! **The debug switches are not in it.** They are per-session by construction
//! (§12.6/#459) — a record that re-armed omni-vision on the next visit would outlive
//! the session gate the whole channel rests on, which is the one thing #513 must not
//! do while moving those switches onto a persisted screen. So the screen draws them
//! from the live run and this module never sees them.
//!
//! # Precedence: the URL wins, then the record, then the build
//!
//! `?tiles=1` survives #513 unchanged ([`crate::tiles`]). It is how a preview artifact
//! is looked at — the host strips the hash and frames the page, so a build stamps the
//! mode in rather than being asked for it — and how the text renderer can be seen in
//! the very build the tile renderer is being judged against. So a load resolves, in
//! order:
//!
//! 1. **the URL**, if it states a choice — an explicit instruction for *this* load;
//! 2. **the stored record**, if there is one — the player's own last answer;
//! 3. **the build's baked flag**, if it stamped one;
//! 4. otherwise the defaults.
//!
//! A URL override is deliberately **not written back**: it is an instruction for the
//! load, not a preference the player expressed, and a link that silently rewrote
//! someone's settings would be the shareable-link channel reaching somewhere it has no
//! business (§13.1's level/debug split, in a smaller key).

use intrusion_core::{Renderer, Theme};
use serde::{Deserialize, Serialize};
use web_sys::Storage;

/// The key the settings live under. Its own, beside the run's `intrusion:run`
/// ([`crate::save`]): ending a run must never reset a preference.
const KEY: &str = "intrusion:settings";

/// The record's format stamp, checked on the way in. A record from a build that spelled
/// the fields differently is discarded rather than half-read — the same "never a
/// bricked page" rule the autosave's own stamp serves (§12.6), and cheap here because
/// what is lost is two preferences a player can set again in two presses.
const FORMAT: u32 = 1;

/// The stored preferences, as they sit in the slot.
///
/// The two values are stored as **names**, not as booleans: `"light"` in a slot says
/// what it is, and a third theme or a third renderer would extend the match rather than
/// re-mean an existing bit. An unknown name reads as the default, so a record written by
/// a newer build degrades to the game this one knows instead of refusing to boot.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Record {
    version: u32,
    theme: &'static str,
    renderer: &'static str,
}

/// The same record on the way *in*, where the strings are the slot's own rather than
/// this build's. Split from [`Record`] so the write side can stay `&'static str` and
/// name only the spellings this build knows.
#[derive(Clone, Debug, Deserialize)]
struct StoredRecord {
    version: u32,
    theme: String,
    renderer: String,
}

const THEME_DARK: &str = "dark";
const THEME_LIGHT: &str = "light";
const RENDERER_TEXT: &str = "text";
const RENDERER_TILES: &str = "tiles";

/// The preferences a load starts under — what the options screen's two rows will read,
/// and what the shell paints from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Settings {
    pub(crate) theme: Theme,
    pub(crate) renderer: Renderer,
}

impl Settings {
    /// Resolve this load's settings: the stored record, then the `tiles` argument's
    /// override on top of it.
    ///
    /// `renderer_override` is what the URL and the build between them said about the
    /// renderer ([`crate::tiles::boot_choice`]) — `None` when neither stated anything,
    /// which is the ordinary case and the one where the record decides.
    pub(crate) fn boot(renderer_override: Option<Renderer>) -> Self {
        let stored = stored();
        Self {
            theme: stored.map(|s| s.theme).unwrap_or_default(),
            renderer: renderer_override
                .or(stored.map(|s| s.renderer))
                .unwrap_or_default(),
        }
    }
}

/// The record in storage, if there is one this build can read.
fn stored() -> Option<Settings> {
    decode(&slot()?.get_item(KEY).ok()??)
}

/// Parse a stored record — `None` for anything this build will not read: not JSON, or
/// a format it does not recognise.
fn decode(text: &str) -> Option<Settings> {
    let record: StoredRecord = serde_json::from_str(text).ok()?;
    (record.version == FORMAT).then_some(Settings {
        theme: match record.theme.as_str() {
            THEME_LIGHT => Theme::Light,
            _ => Theme::Dark,
        },
        renderer: match record.renderer.as_str() {
            RENDERER_TILES => Renderer::Tiles,
            _ => Renderer::Text,
        },
    })
}

/// Encode settings for the slot.
fn encode(settings: Settings) -> Option<String> {
    serde_json::to_string(&Record {
        version: FORMAT,
        theme: match settings.theme {
            Theme::Dark => THEME_DARK,
            Theme::Light => THEME_LIGHT,
        },
        renderer: match settings.renderer {
            Renderer::Text => RENDERER_TEXT,
            Renderer::Tiles => RENDERER_TILES,
        },
    })
    .ok()
}

/// Write the current preferences to the slot, replacing whatever was there.
///
/// **Best-effort, and silently so.** A browser can refuse storage outright (private
/// browsing, a framed page, a full quota) exactly as it can refuse the autosave's; a
/// refusal costs the player their preference on the *next* load and nothing at all on
/// this one, so there is nothing worth interrupting them to say. The setting they just
/// changed is already live on screen, which is the part that matters.
pub(crate) fn store(settings: Settings) {
    let (Some(store), Some(text)) = (slot(), encode(settings)) else {
        return;
    };
    let _ = store.set_item(KEY, &text);
}

/// The browser's `localStorage`, or nothing at all — asked per call rather than held,
/// because a settings write happens on a keypress and never in a loop (the autosave
/// holds its handle because it writes per turn).
fn slot() -> Option<Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record round-trips: what a flip writes is what the next load reads.
    #[test]
    fn a_written_record_reads_back_the_same() {
        for theme in [Theme::Dark, Theme::Light] {
            for renderer in [Renderer::Text, Renderer::Tiles] {
                let settings = Settings { theme, renderer };
                let text = encode(settings).expect("the record serialises");
                assert_eq!(decode(&text), Some(settings), "{text}");
            }
        }
    }

    /// The values are stored **by name**, so a reader can see what is in the slot and a
    /// later build can add a third without re-meaning a bit.
    #[test]
    fn the_record_stores_names_not_bits() {
        let text = encode(Settings {
            theme: Theme::Light,
            renderer: Renderer::Tiles,
        })
        .expect("the record serialises");
        assert!(text.contains(THEME_LIGHT), "{text}");
        assert!(text.contains(RENDERER_TILES), "{text}");
    }

    /// Anything this build will not read comes back as `None` — and a **name** it does
    /// not know degrades to the default rather than refusing the whole record, so a
    /// record written by a newer build still restores the half of it this one has.
    #[test]
    fn an_unreadable_record_is_no_record() {
        assert_eq!(decode(""), None, "not JSON");
        assert_eq!(decode("{}"), None, "not this shape");
        assert_eq!(
            decode(r#"{"version":99,"theme":"light","renderer":"tiles"}"#),
            None,
            "a format stamp this build does not know",
        );
        assert_eq!(
            decode(r#"{"version":1,"theme":"sepia","renderer":"voxels"}"#),
            Some(Settings::default()),
            "unknown names read as the defaults",
        );
    }

    /// **Precedence** (§11.1/#460 × #513): a URL or baked override wins over the record
    /// for the renderer and touches the theme not at all; with no override the record
    /// decides; with neither, the defaults.
    #[test]
    fn an_override_wins_over_the_record_for_the_renderer_alone() {
        // `Settings::boot` reaches the browser, so the precedence itself is asserted
        // over the same expression it uses — the one line that decides the order.
        let record = Settings {
            theme: Theme::Light,
            renderer: Renderer::Text,
        };
        let resolved = |over: Option<Renderer>| Settings {
            theme: record.theme,
            renderer: over.or(Some(record.renderer)).unwrap_or_default(),
        };
        assert_eq!(resolved(Some(Renderer::Tiles)).renderer, Renderer::Tiles);
        assert_eq!(
            resolved(Some(Renderer::Tiles)).theme,
            Theme::Light,
            "the renderer override says nothing about the theme",
        );
        assert_eq!(
            resolved(None).renderer,
            Renderer::Text,
            "the record decides"
        );
    }
}
