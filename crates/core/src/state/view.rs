//! The read surface of [`State`](super::State) (§11.1/§11.5, §13.2).
//!
//! Every question the renderer, the web shell and the §13.2 sim bot are allowed to
//! ask the running game, gathered in one place: geometry and the player's pose, what
//! the player can see ([`player_fov`](super::State::player_fov)) and remember
//! ([`memory`](super::State::memory)), what they merely *sense* through walls
//! ([`perceive_guard`](super::State::perceive_guard),
//! [`door_cues`](super::State::door_cues)), the danger overlay
//! ([`visible_cone_cells`](super::State::visible_cone_cells),
//! [`in_visible_danger`](super::State::in_visible_danger)), concealment
//! ([`hidden`](super::State::hidden), [`concealed_from`](super::State::concealed_from)),
//! the ability panel's rows, and the usable line's affordances.
//!
//! **Every method here takes `&self` and mutates nothing.** That is the point of
//! giving them one file: "what may a viewer legitimately know?" is answerable by
//! opening it. The renderer is a pure function of state (§12.1), and the bot plans
//! on the player's own channels (§13.2) — both read through exactly this surface, so
//! core and bot can never disagree about what is perceivable.

use super::*;

impl State {
    /// The level geometry (§10.5) — read-only outside the core.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Where the player stands.
    pub fn player(&self) -> Cell {
        self.player
    }

    /// The player's facing — the direction of their last successful step (§5).
    pub fn facing(&self) -> Direction {
        self.facing
    }

    /// Open a targeting session (§8.4) for `mode`, anchored on the player's cell
    /// and facing (§5). The shell drives the returned [`Targeting`] — steering the
    /// cursor with cardinals and confirming — while core owns validity: a `Tile`
    /// cursor is bounded to the §6.1 range box on this facility, and cancelling is
    /// just dropping the session (free, no turn — §4.4). Nothing here auto-targets;
    /// that absence is the whole point of building targeting up front (§8.4).
    pub fn begin_targeting(&self, mode: TargetingMode) -> Targeting {
        Targeting::begin(mode, self.player, self.facing)
    }

    /// Open a targeting session for `ability` by its declared [`TargetingMode`]
    /// (§8.4) — the seam a hotkey or an ability-panel click resolves an ability's
    /// target through, so no ability ever falls back to auto-targeting (the exact
    /// §8.4/§2.3 regression this system exists to prevent).
    ///
    /// `None` for a **passive** (#264): it is never activated, so there is nothing
    /// to aim and no session to open — the caller's key press is the free §4.4
    /// no-op it already is for an ability on cooldown.
    pub fn begin_ability_targeting(&self, ability: AbilityId) -> Option<Targeting> {
        Some(self.begin_targeting(ability.def().economy()?.targeting()))
    }

    /// The player's field of view (§6): the ~180° forward half-disc, or the full
    /// 360° on a turn spent waiting — the only way to see behind you (§8.3) —
    /// including the auto-peek around adjacent corners (#121,
    /// [`field_of_view_with_peek`]). What is
    /// in it renders lit and what is not renders dimmed (§11.5); the renderer
    /// reads this set for the live layer, and tile memory
    /// ([`memory`](Self::memory)) accumulates from it.
    pub fn player_fov(&self) -> &VisibleSet {
        &self.player_fov
    }

    /// The player's tile memory (§11.5a): every cell that has *ever* been inside
    /// their FOV this run, accumulated each sight phase and never forgotten. The
    /// fog renderer reads it to draw remembered contents — intel, hideouts —
    /// distinct from live and never-seen; live state (guards, door open/closed)
    /// deliberately never consults it, so nothing transient is ever "remembered".
    pub fn memory(&self) -> &VisibleSet {
        &self.memory
    }

    /// The duct the player is currently **inside** (§10.7), or `None` on open floor —
    /// the one the renderer lights as a connected `=` path while it is occupied. It is
    /// shown *only* while crawled: the interior path carries no tell on the base map
    /// and is never remembered once the player climbs out (§11.5a). Distinct from
    /// [`Layout::duct_containing`], which merely asks whether a *cell* lies on some
    /// duct's path — a question that no longer answers "is the player crawling", since
    /// an interior cell may overlie ordinary floor the player is simply walking.
    pub fn occupied_duct(&self) -> Option<&Duct> {
        self.in_duct.map(|i| &self.layout.ducts()[i])
    }

    /// Whether the player is concealed — standing inside a hideout (§10.3).
    ///
    /// This is the one concealment query everything reads: the loop refuses a
    /// guard's contact against a hidden player (§4.5/§7.6), the renderer recolours
    /// the occupied cupboard to Owned, and — once vision lands (§6) — a guard's
    /// detection set excludes a hidden player by AND-ing this in, so the danger
    /// overlay cannot claim the player is seen while hidden. It is *derived* from
    /// position rather than stored, so it can never desync: the only way onto a
    /// hideout cell is to bump into it, and moving off clears it.
    pub fn hidden(&self) -> bool {
        self.layout.facility().terrain(self.player) == Some(Terrain::Hideout)
    }

    /// Whether the player is **inside a duct** (§10.7) — crawling a crawlspace, as
    /// opposed to merely standing on a cell some duct's path happens to overlie. This
    /// is *stored* ([`in_duct`](Self::in_duct), the field), set by climbing in and
    /// cleared by climbing out, because a duct's interior may cross ordinary floor
    /// (§10.7 cross-room routing) and position alone can no longer distinguish the two.
    /// Inside, the player is concealed from every guard, contact-safe (guards route
    /// around the entries the duct threads through), and their perception is reduced to
    /// the mouth peek plus a shortened sense (§9.1/§10.7).
    pub fn in_duct(&self) -> bool {
        self.in_duct.is_some()
    }

    /// Whether the player is **crouched** behind partial cover (§10.3): they
    /// bumped a table to duck behind it and have not spent a turn on anything
    /// but waiting since. Crouching is weaker than the cupboard — concealment
    /// is directional, per-viewer, and only across the chosen table
    /// ([`concealed_from`](Self::concealed_from)) — and it is not contact-safe:
    /// a guard walking into a crouched player still captures (§4.5).
    pub fn crouched(&self) -> bool {
        self.crouched_behind.is_some()
    }

    /// The table the player ducked behind (§10.3), if any — the *anchor* naming
    /// the crouched-behind run. It stays the originally bumped cell while a
    /// crouch-walk moves the player along the bench, so it may no longer be the
    /// nearest table; the run it names is what matters. Rendering reads
    /// [`crouch_cover`](Self::crouch_cover) for the whole run; everything
    /// rule-side goes through [`concealed_from`](Self::concealed_from).
    pub fn crouched_behind(&self) -> Option<Cell> {
        self.crouched_behind
    }

    /// Whether the player is concealed from a viewer standing at `viewer` — the
    /// per-viewer concealment query the guard AI's detection will AND in and the
    /// danger overlay already honours (§11.5: the overlay must not claim the
    /// player is seen while they are not).
    ///
    /// Three ways to be concealed:
    /// - **In a cupboard** ([`hidden`](Self::hidden)): omnidirectional — no
    ///   viewer anywhere detects the player (§10.3).
    /// - **Camouflaged and still** (§8.3, [`Effect::ConcealWhileStill`]):
    ///   omnidirectional while the ability is active and the last spent turn
    ///   did not move the player; the turn they move, this clause lapses and
    ///   they are revealed for that turn. It resumes the next still turn. Like
    ///   every concealment it blocks *detection* only — never contact (§4.5):
    ///   invisible is not safe.
    /// - **Crouched behind a run of tables** ([`crouched`](Self::crouched)):
    ///   directional — from viewers whose line of sight to the player crosses
    ///   **any table of the run the player ducked behind** (the whole §10.1a
    ///   bench, not just the bumped cell — a guard cannot look down a bench and
    ///   see the player through its other tables), grazing a table's corner
    ///   included. Other runs the player happens to stand beside cover nothing.
    ///   Integer arithmetic throughout ([`cover::run_conceals`]), so it is
    ///   exactly deterministic (§12.4).
    ///
    /// Concealment is not cover from *contact*: a guard can still walk into a
    /// crouched player and capture (§4.5). And it composes with sight, not
    /// replaces it — a viewer that cannot see the player's cell at all needs no
    /// concealing.
    pub fn concealed_from(&self, viewer: Cell) -> bool {
        if self.hidden() || self.in_duct() {
            // A cupboard and a duct both conceal omnidirectionally — no viewer
            // anywhere detects the player through solid wall (§10.3/§10.7).
            return true;
        }
        if self.abilities.effect_active(Effect::ConcealWhileStill) && !self.moved_this_turn {
            return true;
        }
        let Some(anchor) = self.crouched_behind else {
            return false;
        };
        let run = cover::cover_run(self.layout.facility(), anchor);
        cover::run_conceals(&run, self.player, viewer)
    }

    /// Whether `guard` would **detect** the player *right now* — the live §7.2
    /// takedown gate, read from the guard's current cone rather than the
    /// [`detected`](Guard::detected_player) latch the sight phase leaves behind.
    ///
    /// The latch is set in phase 2's look (§4.2) and goes a turn stale for a guard
    /// that steps adjacent during phase 3: its cone is refreshed at the new cell
    /// ([`advance_to`](Guard::advance_to)) but its detection is not, so a guard that
    /// walks up to face the player point-blank still reports `detected_player() ==
    /// false` at the next player phase. Reading the latch there would wave through a
    /// takedown from directly in front — which §6.1's touching ring and §155's rear
    /// blind spot forbid: beside or in front is never a valid takedown, only the
    /// three rear cells are. Reading the cone live closes that window.
    ///
    /// The predicate is exactly [`Guard::see`]'s: the guard detects the player when
    /// its (rear-blind-spot-carved, §155) cone covers the player's cell and nothing
    /// [`conceals`](Self::concealed_from) them. Because the glimpse zone spans the
    /// whole cone (`GLIMPSE_RANGE == GUARD_SIGHT_RANGE`), any cell in the cone is in
    /// range, so cone membership alone settles it. Concealment still defeats the
    /// gate, so a *concealed* front takedown (crouched, cupboard, §7.2) is untouched
    /// — the bump is refused only against a guard that genuinely sees you.
    ///
    /// Public so the §13.2 sim bot can plan against the *same* gate: an unaware
    /// guard is a takedown target only while this is `false`, so the bot avoids
    /// walking into a guard the gate would refuse (#183) and can pick a safe strike.
    pub fn guard_detects_now(&self, guard: &Guard) -> bool {
        guard.fov().contains(self.player) && !self.concealed_from(guard.pos())
    }

    /// The cells of the partial-cover run the player is crouched behind (§10.3)
    /// — the whole §10.1a bench, in flood order — or empty when standing. The
    /// renderer recolours every cell of it to Owned (§11.3): the run is one
    /// piece of furniture, so it hides as one.
    pub fn crouch_cover(&self) -> Vec<Cell> {
        self.crouched_behind
            .map(|anchor| cover::cover_run(self.layout.facility(), anchor))
            .unwrap_or_default()
    }

    /// The guards, for rendering and tests.
    pub fn guards(&self) -> &[Guard] {
        &self.guards
    }

    /// The bodies takedowns have left (§7.2), for rendering and tests.
    pub fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    /// The cell of the body the player is dragging (§8.3), if any. The renderer
    /// recolours that `z` to Owned — the body in your hands, like the cupboard
    /// you hide in (§11.3) — and the ambient status reads the state from here.
    pub fn dragging(&self) -> Option<Cell> {
        self.dragging.map(|i| self.bodies[i].cell())
    }

    /// The live decoy's cell (§8.3), if one is out — the fake intruder the
    /// renderer draws as an Owned `@` (§10.3/§11.3: a thing you made, wearing
    /// your own glyph, which is the whole trick).
    pub fn decoy(&self) -> Option<Cell> {
        self.decoy
    }

    /// The player's current guard-sense range (§9.1): [`PLAYER_SENSE_RANGE`] normally,
    /// widened to [`PLAYER_SENSE_RANGE_WAITING`] on the turn the player's spent action
    /// was a Wait — the same `waited` signal that buys the 360° look (§8.3). A free
    /// action changes nothing, so a mis-input never widens or narrows the sense.
    ///
    /// **Inside a duct** the sense shrinks to [`DUCT_SENSE_RANGE`] and Wait no longer
    /// widens it (§10.7): the crawlspace's cost is degraded information, and taking
    /// stock of the whole area is exactly the open-floor affordance a duct removes.
    pub fn sense_range(&self) -> u32 {
        if self.in_duct() {
            DUCT_SENSE_RANGE
        } else if self.waited {
            PLAYER_SENSE_RANGE_WAITING
        } else {
            PLAYER_SENSE_RANGE
        }
    }

    /// The player's current **door-sense** range (§9.4/§10.4): [`DOOR_SENSE_RANGE`]
    /// on open floor, shrinking to [`DUCT_SENSE_RANGE`] inside a duct with the rest
    /// of the crawlspace's degraded perception (§10.7). Unlike the guard sense, a
    /// **Wait does not widen it** — a door change is already a loud, coarse event, so
    /// the "take careful stock" affordance that sharpens precise guard positions
    /// buys nothing here. A door change beyond this range leaves no cue.
    pub fn door_sense_range(&self) -> u32 {
        if self.in_duct() {
            DUCT_SENSE_RANGE
        } else {
            DOOR_SENSE_RANGE
        }
    }

    /// The cells lit by a live door-change cue (§9.4/§10.4): the **whole footprint**
    /// of every door that opened or shut away from the player recently enough to still
    /// show. The renderer paints each as a [`Category::Sensed`] background — the same
    /// "sensed through a wall" channel as a guard, a fading mark that "someone passed
    /// here", readable around a corner and out of FOV, position only (§10.4). A
    /// guard-driven or automatic door change within
    /// [`door_sense_range`](Self::door_sense_range) lights one; a door *you* operate
    /// does not (it keeps its quiet near-line self-narration, §11.7).
    pub fn door_cues(&self) -> impl Iterator<Item = Cell> + '_ {
        let regions = self.layout.regions();
        self.door_cues
            .iter()
            .flat_map(move |cue| regions.door(cue.door).cells())
    }

    /// The cells of the momentary **spot flash** (§11.5/§9.2/§7.6, #222): when a
    /// guard *freshly* detects the player from **outside the player's view**, the
    /// straight sightline from that guard to the player lights red
    /// ([`Category::Danger`]) for that one beat — the "a guard just saw you, and here
    /// is where it is" cue the loop was missing, so a spot is a direction to run
    /// *from* rather than a dice roll (§7.6). The line is honest danger: the spotting
    /// guard's cone genuinely watches those cells (it detected the player along them),
    /// so this is a strict, momentary *subset* of the danger overlay, never a new kind
    /// of claim — and it clears on the next action ([`spotters`](Self::spotters)).
    ///
    /// A guard the player *can* see is excluded: its facing and full cone already
    /// paint every turn (§9.2), so flashing a line to it would only double-draw. The
    /// flash is the **exception §9.2 permits** — a deliberate, detection-fired reveal
    /// of an unseen guard's line, never a standing cone for one that stays unseen.
    pub fn spot_flash(&self) -> impl Iterator<Item = Cell> + '_ {
        let player = self.player;
        self.spotters
            .iter()
            .filter_map(move |&index| self.guards.get(index))
            .filter(move |guard| self.perceive_guard(guard) != Some(GuardPerception::Seen))
            .flat_map(move |guard| guard.pos().line_to(player))
    }

    /// The cells the §11.5 danger overlay paints from guards the player can **see**
    /// — the union of every such guard's cone, the "detection set you can see"
    /// [SETTLED §11.5]. Defined once here so the renderer's cone pass and the
    /// held-movement guard ([`in_visible_danger`](Self::in_visible_danger), #223)
    /// read the *same* set: the picture and the rule can never disagree on what a
    /// visible cone is.
    ///
    /// The overlay's spares are applied here, not by the caller. The player's own
    /// cell drops out while they are [`concealed_from`](Self::concealed_from) that
    /// guard (§10.3 — red under you means detected, and a concealed player is not),
    /// and inside a duct only the mouth-peek window paints (§10.7/#134 — the rest of
    /// a cone is memory). The `always_show_vision_cones` modifier (§12.6) widens the
    /// set to every guard's cone, seen or not; it may only ever *widen* the overlay
    /// (§11.5 [SETTLED]), never drop a spare.
    pub fn visible_cone_cells(&self) -> impl Iterator<Item = Cell> + '_ {
        let show_all = self.modifiers.always_show_vision_cones;
        let in_duct = self.in_duct();
        let player = self.player;
        self.guards
            .iter()
            // A confused guard is blind (§8.3/#240): it detects nothing, so it has no
            // cone to paint — dropped here *before* the show-all widening, since the
            // modifier may only widen the *detection set you can see* (§11.5), and a
            // frozen guard is not detecting at all. This is the "cone off" #240 asks
            // for, read from the one [`guard_confused`](Self::guard_confused) query the
            // guard phase also uses, so picture and rule cannot disagree.
            .filter(move |guard| !self.guard_confused(guard))
            .filter(move |guard| show_all || self.player_fov.contains(guard.pos()))
            .flat_map(move |guard| {
                let spare_player = self.concealed_from(guard.pos());
                guard.fov().cells().filter(move |&cell| {
                    !(spare_player && cell == player)
                        && !(in_duct && !self.player_fov.contains(cell))
                })
            })
    }

    /// Whether the player currently sees `cell` painted by the §11.5 danger overlay:
    /// inside a seen guard's cone ([`visible_cone_cells`](Self::visible_cone_cells)),
    /// or on the momentary spot-flash sightline of a guard that has *just* detected
    /// them from out of view ([`spot_flash`](Self::spot_flash)/#250) — both are red
    /// the player can act on ("don't get blindly walked into detection"). Exposed so
    /// the shell's held-movement guard (#223) reuses the overlay's own set rather
    /// than recomputing detection: a held key or swipe stops auto-repeating the
    /// moment a step would carry the player into one of these cells, or they already
    /// stand in one.
    pub fn in_visible_danger(&self, cell: Cell) -> bool {
        self.visible_cone_cells().any(|c| c == cell) || self.spot_flash().any(|c| c == cell)
    }

    /// How the player perceives `guard` this frame (§9.2), or `None` if it is neither
    /// seen nor sensed (out of range — it draws nothing, live, and is not remembered,
    /// §11.5a). This is the pure §9 classification the renderer reads:
    ///
    /// - **Seen** — the guard's cell is in the player's FOV (§6): line of sight is
    ///   clear, so its facing, cone and danger overlay are all known.
    /// - **Sensed** — not in the FOV, but within the guard-sense box
    ///   ([`sense_range`](Self::sense_range)) measured by the §6.1 box metric
    ///   ([`sight_distance`](Cell::sight_distance)) **through walls**: the exact cell
    ///   is known, nothing about where it looks.
    ///
    /// Seen wins over Sensed by construction — a guard in the FOV is Seen even if it is
    /// also inside the (larger or smaller) sense box — so the dot never coexists with
    /// the full guard on the same cell.
    ///
    /// A guard visible only through the auto-peek (#121) is **Seen**, cone and
    /// all: the lean is a real line of sight, so it earns the full picture, not
    /// the sensed dot. The overlay that cone paints stays truthful — the guard's
    /// own detection uses its plain cast, which cannot see around the corner
    /// back ([`field_of_view_with_peek`]'s one-sidedness).
    pub fn perceive_guard(&self, guard: &Guard) -> Option<GuardPerception> {
        if self.player_fov.contains(guard.pos()) {
            Some(GuardPerception::Seen)
        } else if self.player.sight_distance(guard.pos()) <= self.sense_range() {
            Some(GuardPerception::Sensed)
        } else {
            None
        }
    }

    /// Whether `guard` is currently **confused** — blinded and frozen by an active
    /// Confusion (§8.3/#240): the ability is active ([`Effect::Confuse`]) and the
    /// guard is within [`CONFUSION_RADIUS`] of the player, measured by the §6.1 box
    /// metric ([`sight_distance`](Cell::sight_distance)) **through walls**, exactly
    /// like the guard sense (§9). The one query both the guard phase and the renderer
    /// read: a confused guard neither senses nor moves this turn ([`guard_phase`]),
    /// and its cone is dropped from the danger overlay ([`visible_cone_cells`]) — the
    /// "cone off" §11.5 requires. The freeze is a **pause**, not a reset: skipping the
    /// guard's sense leaves its state and lead untouched, so it resumes cleanly when
    /// the window ends (§8.2).
    ///
    /// [`Effect::Confuse`]: crate::Effect::Confuse
    /// [`guard_phase`]: Self::guard_phase
    /// [`visible_cone_cells`]: Self::visible_cone_cells
    pub fn guard_confused(&self, guard: &Guard) -> bool {
        self.abilities.effect_active(Effect::Confuse)
            && self.player.sight_distance(guard.pos()) <= CONFUSION_RADIUS
    }

    /// How many objectives are still out. The run can be won only at zero (§10.2).
    pub fn objectives_remaining(&self) -> usize {
        self.objectives.iter().filter(|o| !o.taken).count()
    }

    /// Whether the exit will accept the player — the run's **intel gate**
    /// (§10.2/§4.5), now a level modifier ([`IntelGate`](crate::IntelGate)/#244)
    /// rather than one fixed rule, so the three modes gate the same facility
    /// differently:
    ///
    /// - [`All`](crate::IntelGate::All) — quick play (#244): every objective must be
    ///   taken. Gather the intel, then get out (§10.2).
    /// - [`AtLeastOne`](crate::IntelGate::AtLeastOne) — the §4.5 [START] baseline and
    ///   the sim (§13.3): one intel in hand is a complete run, which keeps the bot's
    ///   outcome profile mixed (the all-intel march got it caught nearly every seed).
    /// - [`None`](crate::IntelGate::None) — campaign (§14 v3): intel is currency
    ///   (§2.2), not an exit key, so the exit never refuses.
    ///
    /// A level with no objectives is winnable at once under every gate (an empty
    /// `all` is vacuously satisfied).
    pub fn exit_ready(&self) -> bool {
        use crate::modifiers::IntelGate;
        match self.modifiers.intel_to_exit {
            IntelGate::None => true,
            IntelGate::AtLeastOne => {
                self.objectives.is_empty() || self.objectives.iter().any(|o| o.taken)
            }
            IntelGate::All => self.objectives.iter().all(|o| o.taken),
        }
    }

    /// The cells of consoles that have been **used up** — spent objectives (§11.2),
    /// plus a comms console whose radio net is already dead (§7.3/§7.7). Terrain alone
    /// can't tell a spent console from a live one (both keep their terrain kind); the
    /// `taken` flag and [`radio_silenced`](Self::radio_silenced) live here, so the
    /// renderer reads them to draw a used console as inert Neutral scenery rather than a
    /// live Interest `$`/`Ψ` (§11.2 "spent objectives" = Neutral).
    ///
    /// One list for both because the player reads them the same way — *there was
    /// something here, it's done* — and because a silenced comms console offers nothing
    /// on the usable line either, so its glyph should stop advertising itself too.
    pub fn spent_consoles(&self) -> impl Iterator<Item = Cell> + '_ {
        self.objectives
            .iter()
            .filter(|o| o.taken)
            .map(|o| o.cell)
            .chain(self.comms_console.filter(|_| self.radio_silenced))
    }

    /// The facility's comms console (§7.3/§7.7), or `None` for a facility without one.
    /// Its cell is static geometry; whether it has been used is
    /// [`radio_silenced`](Self::radio_silenced).
    pub fn comms_console(&self) -> Option<Cell> {
        self.comms_console
    }

    /// Whether the radio net has been killed for the rest of the level (§7.3/§7.7) —
    /// the player bumped the comms console. Once true it never goes back: control stops
    /// pinging downed guards, and both §7.7 cooperation call-ins stop firing.
    pub fn radio_silenced(&self) -> bool {
        self.radio_silenced
    }

    /// The count of completed turns (the startup turn is turn zero).
    pub fn turn(&self) -> u32 {
        self.turn
    }

    /// The facility-wide alert level (§7.3): how many times the radio net has
    /// escalated this run (a guard going fully silent). Read by the near line's
    /// ambient status (§11.4) and available to the shell and the sim's alert-peak
    /// metric (§13.2).
    pub fn alert(&self) -> u32 {
        self.alert
    }

    /// Whether the run is live, won, or lost (§4.5).
    pub fn outcome(&self) -> Outcome {
        self.outcome
    }

    /// The events of the player's most recent action — the near line's source
    /// (§11.7). Empty before the first input; frozen once the run ends.
    pub fn last_events(&self) -> &[Event] {
        &self.last_events
    }

    /// The economy state of ability `id` (§8.2), as the panel reads it (§11.4):
    /// `Ready`, `Active` with the duration left, or `Cooling` with the cooldown
    /// left — the exact number the player gets (§8.2 timing). The show-on-wait
    /// render ticket wires the panel to this; the display's contextual `Unusable`
    /// (a missing target) is not an economy state and is never returned here.
    pub fn ability_state(&self, id: AbilityId) -> AbilityState {
        self.abilities.state(id)
    }

    /// The run's ability line/panel (§11.4): one [`AbilityStatus`] per economy
    /// ability, in the fixed deck order ([`AbilityId::ALL`]), each carrying its
    /// real slot state ([`ability_state`](Self::ability_state)). This is what the
    /// always-on line and the deployed panel draw ([`render_screen`]) and what a
    /// click hit-tests against ([`ability_at`]) — assembled from live runtime, so
    /// there is no roster to drift from the economy.
    ///
    /// The set is exactly the *activated* abilities the time economy governs
    /// (§8.2). The innate **bump** verbs — Takedown and Drag (§7.2, §8.3) — are
    /// not here: they have no duration or cooldown to show and are not
    /// [`Input::Activate`]d but done by walking into their target, so their
    /// availability already speaks through the **usable line**
    /// ([`affordances`](Self::affordances)), not this line.
    ///
    /// [`render_screen`]: crate::render_screen
    /// [`ability_at`]: crate::ability_at
    pub fn ability_statuses(&self) -> Vec<AbilityStatus> {
        // Only the abilities the run actually holds (#244): the loadout, in the
        // fixed deck order. An ability the player was not granted is not a greyed
        // row — it is simply absent from the line, so the strip never advertises a
        // key that does nothing.
        self.abilities
            .loadout()
            .iter()
            .map(|id| AbilityStatus {
                id,
                state: self.ability_state(id),
            })
            .collect()
    }

    /// What a bump would do from here — the **usable line** (§11.4): each
    /// interaction orthogonally adjacent to the player, with the direction to
    /// bump it, in [`Direction::ALL`] order. The §10.6 one-usable guarantee
    /// keeps this to a single entry on generated boards; a hand-built state
    /// may list more, one per direction.
    ///
    /// This mirrors [`step`](Self::step)'s bump resolution case for case, so
    /// the line can never promise what a bump won't deliver: an **unaware**
    /// guard offers the takedown while an aware one offers nothing (§7.2), a
    /// spent console and an occupied cupboard are just solid, and door poses
    /// come from the same door graph the bump consults (§10.4). Each target must
    /// also be in the player's FOV — which the touching ring always is (§6.2) —
    /// so the line can never leak what the fog still hides (§11.5a).
    pub fn affordances(&self) -> Vec<(Direction, Affordance)> {
        let mut out = Vec::new();
        for dir in Direction::ALL {
            let Some(target) = self.player.step(dir) else {
                continue;
            };
            // The FOV gate is the predictor's alone — the line must never leak what the
            // fog hides (§11.5a); a bump itself needs no sight. What the *interaction*
            // is comes from the one shared ladder, so the label can't drift from it.
            if !self.player_fov.contains(target) {
                continue;
            }
            if let Some(a) = self.bump_kind(target).affordance() {
                out.push((dir, a));
            }
        }
        out
    }
}
