//! Pure, deterministic game logic for Intrusion.
//!
//! This crate is the load-bearing half of the architecture (§12.1): it knows
//! nothing about rendering, input, the DOM, the clock, or the platform. Its
//! whole contract is `state × input → state, events`, and it must be testable
//! natively in milliseconds with no browser.
//!
//! # The layers
//!
//! Read bottom-up; each rests on the one before it.
//!
//! **Substrate.** The seeded PRNG ([`Rng`], §12.4) — the one primitive every other
//! system builds on — and the grid ([`Cell`], [`Direction`], §4.1/§4.3): 4-directional
//! movement with Manhattan distance. On it, the facility ([`Facility`], [`Terrain`]):
//! the §10.3 terrain table as exhaustive property matches, the cell-capacity occupancy
//! query, and the indestructible border the level guarantees (§10.6).
//!
//! **Space.** The region graph ([`RegionGraph`], §10.5) that gives corridors and rooms
//! a name, the corridor-first partition that carves them ([`generate`], §10.1), the
//! hinged doors cut where rooms meet corridors (§10.4), the sightline rule that
//! guarantees cover on every long straight (§10.1a), and the player-only duct
//! crawlspaces threaded through the walls ([`Duct`], §10.7).
//!
//! **Sight.** Symmetric-shadowcast field of view ([`field_of_view`], §6): the player's
//! half-disc — 360° on a turn spent waiting — and each guard's ~90° wedge, recomputed
//! every turn from its *current* pose. Guards detect on **vision alone** (§9
//! **[SETTLED]** — there is no sound, no hearing).
//!
//! **The loop.** [`State`] and `state × input → state, events` (§4.2): player, sight,
//! guards, in that order, under the turn-cost rule (§4.4) and the two win/lose
//! conditions (§4.5). Around it sit the guard mind ([`Guard`], §7 — patrol, the two
//! detection zones, the bounded search, takedowns and the radio net) and the ability
//! economy ([`Ability`], §8.1/§8.2): a data-driven catalog plus turn cost, duration
//! and cooldown, with the `duration + cooldown` lockout emergent rather than stored.
//!
//! **Presentation.** [`render`] (§11.1): a pure function of state producing the glyph
//! grid, drawn through the §11.5a fog — geometry always, contents once seen then
//! remembered, live state only in the current FOV. Colour is named only as a
//! [`Category`] (§11.2); the platform shell owns the concrete table.
//!
//! **The run.** [`Campaign`] (§14 v3/§2.2): the layer above a single level — a forward
//! walk through the facility map ([`FacilityMap`]), each facility seeded from `(run
//! seed, node id)`, with the salvaged tech and the intel the run carries between them.
//! The map is a **graph with real edges, grown lazily**: an open edge only ever reaches
//! an adjacent lane, so where the run stands decides what is in front of it, and each
//! offer names its facility's [`Flavour`] outright. Nothing survives the run itself,
//! which is what permadeath means here (§2.2). Intel is that run's **currency** — a
//! [`Wallet`] banked at every completed raid and spent at the map between facilities
//! (§14 v3), never at the exit, which in a campaign never refuses.
//!
//! **Configuration.** [`LevelModifiers`] (§12.6) resolved once per run, and
//! [`LevelSeed`] composing seed, modifiers and loadout into one shareable token — the
//! same entry point the shell and the §13.2 sim both boot through, which is what makes
//! a seed the bot flagged a level you can play by hand.

#![forbid(unsafe_code)]

mod ability;
mod alert;
mod beat;
mod body;
mod campaign;
mod category;
mod cell;
mod control;
mod cover;
mod difficulty;
mod door;
mod duct;
mod exchange;
mod facility;
mod generate;
mod guard;
mod input;
mod level_seed;
mod mnemonic;
mod modifiers;
mod path;
mod place;
mod radio;
mod region;
mod render;
mod replay;
mod rng;
mod salvage;
mod state;
mod status;
mod targeting;
#[cfg(test)]
mod test_support;
mod verdict;
mod vision;

pub use ability::{
    Ability, AbilityId, AbilityMode, AbilityState, AbilityStatus, Behaviour, Economy, Effect,
    Loadout, TargetingMode,
};
pub use alert::{AlertEffect, AlertReadout, AlertTrigger, AlertTuning, TOP_RUNG};
pub use body::Body;
pub use campaign::map::{DEPTH_SPACING, LANES, LANE_SPACING};
pub use campaign::{
    facility_seed, Campaign, CampaignStage, FacilityMap, Flavour, Loudness, MapPos, NodeId, Offer,
    Outlay, Wallet, ALERTS_ALL, ALERTS_ONE, DEPTH_TO_ARCHIVE, ROUTE_UNLOCK_COST,
};
pub use category::{Category, Theme};
pub use cell::{Cell, Direction};
pub use control::{remote_kind, transfers_control, Remote, RemoteKind, DRONE_SIGHT_RANGE};
pub use difficulty::{Difficulty, SPAN as DIFFICULTY_SPAN};
pub use door::DoorAction;
pub use duct::Duct;
pub use exchange::{Choice, Exchange};
pub use facility::{Facility, Terrain};
pub use generate::{generate, generate_level, GenError, Layout, SIGHTLINE_MAX_RUN};
pub use guard::{Guard, GuardState};
pub use input::{
    ability_slot_for_code, declines_exchange, end_nav_for_gesture, end_nav_for_key,
    help_nav_for_gesture, help_nav_for_key, input_for_gesture, input_for_key, key_for_code,
    map_nav_for_gesture, map_nav_for_key, menu_nav_for_gesture, menu_nav_for_key,
    ui_command_for_key, EndNav, Gesture, HelpNav, MapNav, MenuNav, UiCommand,
};
pub use level_seed::{start_level, start_level_with, LevelSeed};
pub use modifiers::{
    ActiveModifier, CacheCount, DebugModifiers, GuardCount, IntelCount, IntelGate, LayoutKnowledge,
    LevelModifiers, ModifierDirection, ModifierSources,
};
pub use place::{LevelConfig, Placement};
pub use region::{
    Door, DoorCell, DoorId, DoorKind, DoorLock, Region, RegionGraph, RegionId, RegionKind,
};
pub use render::{
    ability_at, ability_in_slot, ability_mnemonic, ability_slot_for_letter, ascii_grid,
    flavour_glyph, help_hit, hit_of, is_help_button, is_message_button, map_hit, menu_hit,
    message_log_rows, render, render_map, render_screen, verdict_hit, EndUi, Fill, GlyphCell, Grid,
    HelpHit, HelpTab, InputModality, MapHit, MapUi, MenuEntry, MenuHit, MenuScreen, MenuUi,
    OptionsControl, ScreenUi, SeedCopy, Surface, Visibility, BOTTOM_ROWS, TOP_ROWS,
};
pub use replay::{
    ability_script_letter, field_in, input_token, parse_replay_link, parse_script, replay_fragment,
    to_script,
};
pub use rng::Rng;
pub use salvage::cache_contents;
pub use state::{
    phase_eject_stun, Affordance, BoreRefusal, EffectArea, Event, GuardPerception, Input, Outcome,
    SenseMark, State, DOOR_CUE_DECAY_TURNS, DOOR_SENSE_RANGE, EFFECT_FLASH_TURNS,
    GUARD_CUE_DECAY_TURNS, LOCKDOWN_RADIUS, PHASE_EJECT_STUN_BASE, PLAYER_SENSE_RANGE,
    PLAYER_SENSE_RANGE_WAITING,
};
pub use status::{live_messages, message_for, near_line, Message, MessageHistory, HISTORY_ACTIONS};
pub use targeting::{within_range, Target, Targeting, TileCursor};
pub use verdict::{EndExit, Ending, RunMode, RunOptions, RunStats, Verdict};
pub use vision::{
    field_of_view, field_of_view_with_blind_spot, field_of_view_with_peek, BlindPolicy, BlindTier,
    VisibleSet, ENHANCED_SIGHT_RANGE, FULL_SIGHT_ARC, GUARD_SIGHT_ARC, GUARD_SIGHT_RANGE,
    PLAYER_SIGHT_ARC, PLAYER_SIGHT_RANGE,
};
