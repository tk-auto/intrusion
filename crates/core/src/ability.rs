//! The ability panel's display vocabulary (§11.4) — **what the show-on-wait
//! panel says**, and, for now, a placeholder that stands in for the runtime the
//! ability model will eventually feed it.
//!
//! §11.4 leaves *where* ability state lives on screen **[OPEN]** (§15 Q9). The
//! first experiment is **show-on-wait**: while the player is waiting — the one
//! turn they have already spent to look around (§8.3) — the render exposes each
//! ability's state, so peeking rides on a cost the player already paid (§2.3,
//! cost is load-bearing). This module owns the *display* half of that: the four
//! states an ability reads as ([`AbilityState`]) and how each formats, plus the
//! list the panel draws ([`AbilityStatus`]).
//!
//! # This is the display seam, not the ability model
//!
//! The per-ability **runtime** — which ability is active, how many turns of
//! duration or cooldown remain, whether it is usable right now — lands with the
//! ability model (§8.1/§8.2, its own ticket). That runtime does not exist yet, so
//! [`sample_panel`] supplies a **placeholder** set chosen to exercise all four
//! display states at once. Its whole job is to let the panel's *placement and
//! content* be judged now (§15 Q9's "first experiment … keep it swappable"),
//! before the model is built. When the model lands, the panel reads real state
//! and this placeholder is deleted — the display types below are what stay.
//!
//! Two things are already real, not placeholder, and must survive that swap:
//!
//! - **Hotkeys come from [`ability_hotkey`](crate::input::ability_hotkey)**, the
//!   settled §11.6 identity→letter map — never from the panel's row order. A key
//!   is a fixed fact about an ability, so reordering or trimming the list can
//!   never move one (the §11.6 regression this repo already designed out).
//! - **The number shown is the number the player gets** (§8.2 timing): the panel
//!   formats exactly the value it is handed and advertises nothing else, so it
//!   cannot re-introduce the old advertised-vs-real discrepancy.

use crate::input::ability_hotkey;

/// The runtime state of one ability, as the player reads it (§11.4): the four
/// cases the panel must keep discoverable — ready, active, cooling, unusable.
///
/// The numbers are turn counts under the §8.2 economy — a duration ticking down
/// while active, a cooldown draining once inactive — and [`AbilityState::suffix`]
/// renders them in the design's notation (`[N]` active, `/N/` cooling).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AbilityState {
    /// Available to use this turn.
    Ready,
    /// Switched on, with `remaining` turns of duration left — shown `[N]` (§11.4).
    Active { remaining: u32 },
    /// Recharging, with `remaining` turns of cooldown left — shown `/N/` (§11.4).
    Cooling { remaining: u32 },
    /// Not usable right now for a reason other than cooldown — no adjacent target
    /// for a takedown, no body to drag (§8.3). Discoverable, but greyed.
    Unusable,
}

impl AbilityState {
    /// The state's notation appended after the ability name (§11.4): `[N]` while
    /// active, `/N/` while cooling, a lone `—` while unusable, and nothing at all
    /// when ready — a ready ability needs no decoration, only its name and key.
    ///
    /// The number is rendered verbatim from the state, so what the panel shows is
    /// exactly what the player gets (§8.2) — the advertised-vs-real gap the old UI
    /// had cannot open here.
    pub fn suffix(self) -> String {
        match self {
            AbilityState::Ready => String::new(),
            AbilityState::Active { remaining } => format!("[{remaining}]"),
            AbilityState::Cooling { remaining } => format!("/{remaining}/"),
            AbilityState::Unusable => "—".to_string(),
        }
    }
}

/// One row of the show-on-wait panel: an ability's hotkey, its name, and the
/// state it is in. Assembled by [`sample_panel`] today; assembled from real
/// per-ability runtime once the ability model lands, unchanged in shape.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AbilityStatus {
    /// The explicit §11.6 hotkey, from [`ability_hotkey`](crate::input::ability_hotkey).
    pub hotkey: char,
    /// The ability's display name (§8.3).
    pub name: &'static str,
    /// What state the ability is in right now (§11.4).
    pub state: AbilityState,
}

impl AbilityStatus {
    /// The one line the deployed panel draws for this ability: `<key> <Name>` with
    /// the state notation tacked on when there is one — `r Run`, `c Camouflage [7]`,
    /// `d Decoy /12/`, `t Takedown —`.
    pub fn label(&self) -> String {
        let suffix = self.state.suffix();
        if suffix.is_empty() {
            format!("{} {}", self.hotkey, self.name)
        } else {
            format!("{} {} {}", self.hotkey, self.name, suffix)
        }
    }

    /// The compact readout for the **always-on ability line** (§11.4): just the
    /// hotkey, with the active/cooling number tucked inline (`c[7]`, `d/12/`).
    /// Ready and unusable abilities show the bare key — their state is carried by
    /// colour alone, keeping the strip to one glyph each so the whole set fits a
    /// single row. The full name lives only in the deployed panel ([`Self::label`]).
    pub fn compact(&self) -> String {
        match self.state {
            AbilityState::Active { .. } | AbilityState::Cooling { .. } => {
                format!("{}{}", self.hotkey, self.state.suffix())
            }
            AbilityState::Ready | AbilityState::Unusable => self.hotkey.to_string(),
        }
    }
}

/// **PLACEHOLDER (§15 Q9).** The panel the show-on-wait experiment draws, until
/// the ability model (§8.1/§8.2) exists to supply real runtime state.
///
/// The rows are the §8.3 starting set, each keyed by its settled §11.6 hotkey
/// (looked up by *name*, never derived from this list's order — reordering the
/// rows below moves no key), with a fixed sample state per ability chosen so the
/// panel shows **all four** [`AbilityState`]s together: two ready, one active,
/// one cooling, two unusable. That is the whole point — it makes the panel's
/// placement and contents concrete enough to judge (§15 Q9) before there is a
/// model to read. The sample numbers are pinned by a test so a later edit is a
/// visible decision; the real values arrive with the model, not here.
pub fn sample_panel() -> Vec<AbilityStatus> {
    // (name, placeholder state). Names must match the ability_hotkey identities
    // (§11.6); the key is fetched from that map, so this array's order is display
    // order only and carries no hotkey meaning.
    const SAMPLE: [(&str, AbilityState); 6] = [
        ("Run", AbilityState::Ready),
        ("Takedown", AbilityState::Unusable),
        ("Drag", AbilityState::Unusable),
        ("Camouflage", AbilityState::Active { remaining: 7 }),
        ("Decoy", AbilityState::Cooling { remaining: 12 }),
        ("Dephase", AbilityState::Ready),
    ];
    SAMPLE
        .into_iter()
        .map(|(name, state)| AbilityStatus {
            hotkey: ability_hotkey(name).expect("every §8.3 ability has a settled §11.6 hotkey"),
            name,
            state,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The design's notation, pinned (§11.4): ready shows only the name, active is
    /// `[N]`, cooling is `/N/`, unusable is a lone dash — and the number is the
    /// state's own, rendered verbatim (§8.2).
    #[test]
    fn each_state_formats_in_the_design_notation() {
        assert_eq!(AbilityState::Ready.suffix(), "");
        assert_eq!(AbilityState::Active { remaining: 3 }.suffix(), "[3]");
        assert_eq!(AbilityState::Cooling { remaining: 2 }.suffix(), "/2/");
        assert_eq!(AbilityState::Unusable.suffix(), "—");
    }

    /// A status line reads `<key> <Name>` with the notation appended only when
    /// there is one — a ready ability carries no trailing space or bracket.
    #[test]
    fn a_status_line_joins_key_name_and_notation() {
        let ready = AbilityStatus {
            hotkey: 'r',
            name: "Run",
            state: AbilityState::Ready,
        };
        assert_eq!(ready.label(), "r Run");

        let cooling = AbilityStatus {
            hotkey: 'd',
            name: "Decoy",
            state: AbilityState::Cooling { remaining: 12 },
        };
        assert_eq!(cooling.label(), "d Decoy /12/");
    }

    /// The always-on line's compact form (§11.4): active and cooling tuck the
    /// number against the key, ready and unusable are the bare key (colour carries
    /// their state) — one glyph-group each so the whole set fits a single row.
    #[test]
    fn the_compact_readout_is_the_key_and_any_number() {
        let compact = |state| {
            AbilityStatus {
                hotkey: 'c',
                name: "Camouflage",
                state,
            }
            .compact()
        };
        assert_eq!(compact(AbilityState::Ready), "c");
        assert_eq!(compact(AbilityState::Unusable), "c");
        assert_eq!(compact(AbilityState::Active { remaining: 7 }), "c[7]");
        assert_eq!(compact(AbilityState::Cooling { remaining: 12 }), "c/12/");
    }

    /// The hotkey on every panel row is the **explicit** §11.6 assignment, taken
    /// from `ability_hotkey` by identity — not the row's position. This is the
    /// contract the ticket must not break: were the keys derived from list order,
    /// reordering the sample would shuffle them.
    #[test]
    fn panel_hotkeys_come_from_the_explicit_mapping() {
        for status in sample_panel() {
            assert_eq!(
                Some(status.hotkey),
                ability_hotkey(status.name),
                "{}'s key must be its settled hotkey",
                status.name
            );
        }
    }

    /// The placeholder set exercises **all four** display states, so the panel a
    /// player sees while waiting shows every case at once — the reason the
    /// placeholder exists (§15 Q9). Pinned so a later edit to the sample is a
    /// deliberate, visible change, not a silent drift of what the experiment shows.
    #[test]
    fn the_sample_panel_shows_every_state() {
        let panel = sample_panel();
        assert_eq!(panel.len(), 6);
        assert!(panel.iter().any(|s| s.state == AbilityState::Ready));
        assert!(panel
            .iter()
            .any(|s| matches!(s.state, AbilityState::Active { .. })));
        assert!(panel
            .iter()
            .any(|s| matches!(s.state, AbilityState::Cooling { .. })));
        assert!(panel.iter().any(|s| s.state == AbilityState::Unusable));

        // The exact placeholder values, pinned (§8.2: the shown number is the
        // given number). Real runtime replaces these with the model.
        let by_name = |name| panel.iter().find(|s| s.name == name).unwrap().state;
        assert_eq!(by_name("Camouflage"), AbilityState::Active { remaining: 7 });
        assert_eq!(by_name("Decoy"), AbilityState::Cooling { remaining: 12 });
    }
}
