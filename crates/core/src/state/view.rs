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
    /// (§8.4) — the seam an ability key or a bar tap resolves an ability's
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

    /// How many more turns the player is **stunned** for (§8.3/#329) — zero when
    /// they are free to act. While it is non-zero every input is swallowed as a
    /// spent turn ([`step`](Self::step)), so this is the one query that answers
    /// "is the next key press mine?": the ambient status reads it for the near
    /// line's countdown, and the usable line goes quiet on it.
    pub fn stunned(&self) -> u32 {
        self.stunned
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
    ///   invisible is not safe. Stated once, in
    ///   [`camouflage_holding`](Self::camouflage_holding), so the effect mark that
    ///   reports it (#341) reads the rule rather than a copy of it.
    /// - **Crouched behind a run of tables** ([`crouched`](Self::crouched)):
    ///   directional — from viewers **across the furniture**, meaning on the far
    ///   side of one of the run's straight arms from the player, or with a table of
    ///   the run on the sight line between them (#377/§10.3). It is the whole
    ///   §10.1a bench that hides you, not just the bumped cell, so a guard cannot
    ///   look down a bench and see the player through its other tables; a guard
    ///   that has come round to the player's *own* side does see them. Other runs
    ///   the player happens to stand beside cover nothing. Integer arithmetic
    ///   throughout ([`cover::run_conceals`]), so it is exactly deterministic
    ///   (§12.4).
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
        if self.camouflage_holding() {
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

    /// Whether a crouch anchored on `table` would conceal a player standing at
    /// `from` from a viewer at `viewer` (§10.3) — the geometry half of
    /// [`concealed_from`](Self::concealed_from), asked of a stance the player has
    /// **not taken yet**.
    ///
    /// [`concealed_from`](Self::concealed_from) answers for the pose actually
    /// held; this answers *"if I ducked behind that bench from there, would he
    /// see me?"* — the question §10.3's half-plane rule was shaped to be readable
    /// at a glance (#377), and the one a player asks before spending the turn. It
    /// is the same [`cover::run_conceals`] the held pose is judged by, on the same
    /// whole §10.1a run, so a caller can never drift from the rule by planning
    /// against a private copy of it.
    ///
    /// Deliberately *only* the crouch's geometry: the cupboard, the duct and the
    /// cloak conceal by their own rules and have nothing to say about a table.
    /// `false` when `table` is not partial cover at all — nothing to duck behind is
    /// nothing to be hidden by.
    ///
    /// Public for the §13.2 sim bot, which plans its cover on the player's own
    /// channels (§11.5a — geometry is always known, and a perceived guard's cell
    /// with it) and must ask core rather than re-derive the half-plane.
    pub fn crouch_would_conceal(&self, table: Cell, from: Cell, viewer: Cell) -> bool {
        let run = cover::cover_run(self.layout.facility(), table);
        cover::run_conceals(&run, from, viewer)
    }

    /// Whether a crouch anchored on `table` **survives** a plain step to `to`
    /// (§10.3) — the crouch-walk: the pose is held for exactly as long as the
    /// player keeps hugging the anchored run, its diagonal corners included, so
    /// the walk can round the end of a bench without standing up.
    ///
    /// The companion to [`crouch_would_conceal`](Self::crouch_would_conceal): one
    /// says whether a cell is *hidden* by the bench, the other whether stepping
    /// there keeps you *behind* it, and a crouch-walk needs both. It answers for
    /// the plain move only — an interaction that spends the turn in place, or any
    /// other spent action, stands the player up whatever this says (see the turn
    /// loop's `crouch_walked`).
    pub fn crouch_holds(&self, table: Cell, to: Cell) -> bool {
        cover::run_hugs(&cover::cover_run(self.layout.facility(), table), to)
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
        self.sense_range_after(self.waited)
    }

    /// The guard-sense range an **action taken now** reads (§9.1/§8.3, #325/#345):
    /// [`sense_range`](Self::sense_range) with §9.1's widened Wait already spent.
    ///
    /// Acting is not waiting. A Wait on turn T widens the sense for the look it
    /// bought; an action on turn T+1 is not that Wait, so by the time it resolves the
    /// flag is down — and a reach measured off the widened box would be measuring a
    /// look the player has already spent. Confusion's blast is read through this
    /// ([`confusion_blast`](Self::confusion_blast)), which is what makes the reach a
    /// **pure** function of the board rather than of when it is asked: the ability
    /// bar may ask on the very frame after a Wait (#345), and must get the same
    /// answer the press would.
    fn sense_range_after(&self, waited: bool) -> u32 {
        if self.in_duct() {
            DUCT_SENSE_RANGE
        } else if waited {
            PLAYER_SENSE_RANGE_WAITING
        } else {
            PLAYER_SENSE_RANGE
        }
    }

    /// The guard-sense range with the widening Wait spent — see
    /// [`sense_range_after`](Self::sense_range_after).
    pub(super) fn acting_sense_range(&self) -> u32 {
        self.sense_range_after(false)
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

    /// Everything the sense channel currently marks (§9/§9.4, #192), as a cell and how
    /// many turns old the mark is: the **trail** of each guard felt through a wall — the
    /// cell it stands in this turn and the ones it just left — and the **whole
    /// footprint** of every door that opened or shut away from the player recently
    /// enough to still show.
    ///
    /// One query, because it is one channel: the renderer paints every mark as a
    /// [`Category::Sensed`] background and shades it by age (§11.2), so a "sensed, and
    /// fading" cue reads the same whether the fact behind it was a guard or a door. Both
    /// are position only — where something was felt, never who or which way (§9.2) — and
    /// both are painted through walls and out of the FOV.
    ///
    /// Door marks come first and guard marks second, so a guard standing on a door that
    /// just changed reads as the guard: the renderer resolves the overlap by paint
    /// order, and the guard is the sharper claim.
    pub fn sense_marks(&self) -> impl Iterator<Item = SenseMark> + '_ {
        let regions = self.layout.regions();
        let doors = self
            .sense_cues
            .iter()
            .filter_map(|cue| match cue.source {
                SenseSource::Door(door) => Some((door, cue.age())),
                SenseSource::Guard(_) => None,
            })
            .flat_map(move |(door, age)| {
                regions
                    .door(door)
                    .cells()
                    .map(move |cell| SenseMark { cell, age })
            });
        let guards = self.sense_cues.iter().filter_map(|cue| match cue.source {
            SenseSource::Guard(cell) => Some(SenseMark {
                cell,
                age: cue.age(),
            }),
            SenseSource::Door(_) => None,
        });
        doors.chain(guards)
    }

    /// The cells lit by a live **door-change** cue (§9.4/§10.4): the whole footprint of
    /// every door that opened or shut away from the player recently enough to still
    /// show. The door half of [`sense_marks`](Self::sense_marks), for the callers that
    /// ask about doors specifically. A guard-driven or automatic door change within
    /// [`door_sense_range`](Self::door_sense_range) lights one; a door *you* operate
    /// does not (it keeps its quiet near-line self-narration, §11.7).
    pub fn door_cues(&self) -> impl Iterator<Item = Cell> + '_ {
        let regions = self.layout.regions();
        self.sense_cues
            .iter()
            .filter_map(|cue| match cue.source {
                SenseSource::Door(door) => Some(door),
                SenseSource::Guard(_) => None,
            })
            .flat_map(move |door| regions.door(door).cells())
    }

    /// The cells of the **watcher lines** (§11.5/§9.2/§7.6, #222/#465): for every
    /// guard that detects the player **right now** and that the player cannot *see*,
    /// the straight sightline from that guard to the player lights red
    /// ([`Category::Danger`]) — the "something is watching you, and here is where it
    /// is" cue, so being spotted is a direction to run *from* rather than a dice roll
    /// (§7.6). The line is honest danger: the watching guard's cone genuinely covers
    /// those cells (it is detecting the player along them), so this stays a strict
    /// *subset* of the danger overlay, never a new kind of claim.
    ///
    /// It is **standing**, not a flash (#465). Derived live each frame, it is drawn on
    /// every turn the watcher has the player and gone the turn that stops being true,
    /// so it answers *"it can see you right now"* — the question the player has for the
    /// whole encounter — and never *"it is after you"*. A chaser that has lost the
    /// player draws nothing. That continuity is what makes §11.5's **[SETTLED]**
    /// promise — *if your cell isn't red, no guard detects you* — hold against a guard
    /// watching from a room the player cannot see into, which paints no overlay at all.
    ///
    /// The set is derived, never recorded, and each guard is judged by three questions:
    ///
    /// - It detects the player, read **live** through
    ///   [`guard_detects_now`](Self::guard_detects_now) rather than the per-turn
    ///   [`detected_player`](Guard::detected_player) latch. The two agree except for a
    ///   guard that moved during phase 3, whose latch is a turn stale; the live read is
    ///   the one that matches what the line claims, and it is the same predicate the
    ///   overlay's cones are drawn from, so picture and rule cannot disagree. It also
    ///   carries the overlay's own §10.3 spare for free: a player
    ///   [`concealed_from`](Self::concealed_from) that guard — cupboard, crouch,
    ///   camouflage — is not detected, so no line is drawn.
    /// - It is not [`confused`](Self::guard_confused). A dazed guard is blind
    ///   (§8.3/#240) and takes no part in the sense pass, so its cone is last turn's
    ///   frozen reading and it has nothing honest to draw.
    /// - The player does not *see* it. A seen guard's facing and full cone already
    ///   paint every turn (§9.2), so a line to it would only double-draw.
    ///
    /// The line is the **exception §9.2 permits**, and it is worth naming: while an
    /// unseen guard is looking at you, you get its exact position, through walls, at
    /// any distance, for free — a deliberate breach of the §9 bound on what may be
    /// known about a guard ([`sense_range`](Self::sense_range)). §2.2/§2.3 buy it: you
    /// may not be caught by something you could not perceive, and a guard with eyes on
    /// you is the definition of something about to catch you. It is still a line to a
    /// *watcher*, never a standing cone for a guard that merely stays unseen.
    pub fn watcher_lines(&self) -> impl Iterator<Item = Cell> + '_ {
        let player = self.player;
        self.guards
            .iter()
            .filter(move |guard| self.guard_detects_now(guard))
            .filter(move |guard| !self.guard_confused(guard))
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
    /// or on the standing sightline of a guard that is detecting them from out of view
    /// ([`watcher_lines`](Self::watcher_lines)/#250/#465) — both are red
    /// the player can act on ("don't get blindly walked into detection"). Exposed so
    /// the shell's held-movement guard (#223) reuses the overlay's own set rather
    /// than recomputing detection: a held key or swipe stops auto-repeating the
    /// moment a step would carry the player into one of these cells, or they already
    /// stand in one.
    pub fn in_visible_danger(&self, cell: Cell) -> bool {
        self.visible_cone_cells().any(|c| c == cell) || self.watcher_lines().any(|c| c == cell)
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

    /// Whether `guard` is currently **dazed** — blinded and frozen by a Confusion blast
    /// it was caught in (§8.3/#240/#325). The one query both the guard phase and the
    /// renderer read: a dazed guard neither senses nor moves this turn ([`guard_phase`]),
    /// and its cone is dropped from the danger overlay ([`visible_cone_cells`]) — the
    /// "cone off" §11.5 requires. The freeze is a **pause**, not a reset: skipping the
    /// guard's sense leaves its state and lead untouched, so it resumes cleanly when the
    /// count runs out (§8.2).
    ///
    /// It asks the **guard**, and nothing else. Since #325 the blast decides its set
    /// once, at the moment it fires, and each guard carries its own countdown from
    /// there — so this says nothing about where the player is standing now, and neither
    /// running away from a dazed guard nor walking toward an undazed one changes the
    /// answer. Where the blast *landed* is a separate fact with a separate life — its
    /// own momentary mark on the §11.5 effect layer
    /// ([`effect_cell_marks`](Self::effect_cell_marks)).
    ///
    /// [`guard_phase`]: Self::guard_phase
    /// [`visible_cone_cells`]: Self::visible_cone_cells
    pub fn guard_confused(&self, guard: &Guard) -> bool {
        guard.is_dazed()
    }

    /// How many objectives are still out. The run can be won only at zero (§10.2).
    pub fn objectives_remaining(&self) -> usize {
        self.objectives.iter().filter(|o| !o.taken).count()
    }

    /// How much intel is actually **in hand** — consoles taken. The other half of the
    /// objective picture from [`intel_needed_to_exit`](Self::intel_needed_to_exit):
    /// what the player holds and what the gate still wants are different questions, and
    /// under [`None`](crate::IntelGate::None) the exit is open while this is zero.
    pub fn intel_in_hand(&self) -> usize {
        self.objectives.len() - self.objectives_remaining()
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
        self.intel_needed_to_exit() == 0
    }

    /// How many **more** consoles the run must take before the exit will open — the
    /// gate's own answer, which is not the objective tally
    /// ([`objectives_remaining`](Self::objectives_remaining)): under
    /// [`AtLeastOne`](crate::IntelGate::AtLeastOne) with three consoles out, three
    /// are *remaining* but only one is *needed*.
    ///
    /// The distinction is what the messaging layer needs and the tally could not give
    /// it (#310): every objective line derives from this — or equivalently from
    /// [`exit_ready`](Self::exit_ready), which is just this at zero — rather than from
    /// any fixed intel count, so no message can promise an exit that will refuse.
    pub fn intel_needed_to_exit(&self) -> usize {
        use crate::modifiers::IntelGate;
        let out = self.objectives_remaining();
        match self.modifiers.intel_to_exit {
            IntelGate::None => 0,
            IntelGate::AtLeastOne if self.intel_in_hand() > 0 => 0,
            // One console is the whole requirement — and none at all on a facility with
            // no consoles, which is vacuously satisfied under every gate.
            IntelGate::AtLeastOne => out.min(1),
            IntelGate::All => out,
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

    /// The facility alert **rung**, 0..=3 (§7.3): how far up the ladder this raid has
    /// pushed the facility — 1 from a confirmed sighting or a post going quiet, 2 from
    /// three sightings or a console tampered with while already seen, 3 from a found
    /// body or a second quiet post. It never falls within a level (no decay), and its
    /// effects are cumulative. Read by the near line's ambient status (§11.4) and
    /// available to the shell and the sim's alert-peak metric (§13.2).
    pub fn alert(&self) -> u32 {
        self.alert.rung()
    }

    /// What the help panel says about the facility alert (§7.3/#375): the rung, and
    /// the retaliation that rung has **in force**, generated from the ladder itself.
    ///
    /// The near line states an escalation the turn it happens and is then overwritten
    /// by anything louder (§11.7), so this is the standing surface the player can go
    /// and read — without it the ladder is perceptible for one turn and inert after
    /// (§2.2). Rendering stays a pure function of state (§11.1): the readout is threaded
    /// into [`render_screen`](crate::render_screen) like any other world fact.
    pub fn alert_readout(&self) -> AlertReadout {
        self.alert.readout()
    }

    /// Whether the run is live, won, or lost (§4.5).
    pub fn outcome(&self) -> Outcome {
        self.outcome
    }

    /// **Why** the run ended (§14 v2/#138), or `None` while it is live — the terminal
    /// event as it was latched the turn it fired.
    ///
    /// The pairing with [`outcome`](Self::outcome) is exact: this is `Some` precisely
    /// when the outcome is no longer `Playing`, so an in-progress run has no verdict
    /// to draw and a finished one always does.
    pub fn ending(&self) -> Option<Ending> {
        self.ending
    }

    /// What this run amounts to so far (§14 v2/#138): the five numbers the end screen
    /// reads. Live at any point — the run simply stops changing them once it ends.
    pub fn run_stats(&self) -> RunStats {
        RunStats {
            turns: self.turn,
            intel: self.intel_in_hand(),
            intel_total: self.objectives.len(),
            takedowns: self.takedowns,
            detections: self.detections,
            // The ladder never decays (§7.3), so the rung standing now is the run's
            // peak — no separate high-water mark to keep, and none to let drift.
            alert_peak: self.alert(),
        }
    }

    /// The finished run as the end screen reads it (§14 v2/#138) — why it ended and
    /// what it cost — or `None` while it is still being played, which is what makes
    /// "an in-progress state renders neither screen" a property of the state rather
    /// than a check the renderer has to remember.
    pub fn verdict(&self) -> Option<Verdict> {
        self.ending().map(|ending| Verdict {
            ending,
            stats: self.run_stats(),
        })
    }

    /// The events of the player's most recent action — the near line's source
    /// (§11.7). Empty before the first input; frozen once the run ends.
    pub fn last_events(&self) -> &[Event] {
        &self.last_events
    }

    /// What the near line said **before** the current action (§11.7/#300) — the
    /// bounded ring the deployed log stacks under the live block, newest first. Empty
    /// before the second action, and empty again on a fresh level. The near line does
    /// not read it: its clear-on-action rule is untouched.
    pub fn message_history(&self) -> &MessageHistory {
        &self.message_history
    }

    /// The state of ability `id` as the bar draws it (§11.4): the §8.2 economy —
    /// `Ready`, `Active` with the duration left, `Cooling` with the cooldown left,
    /// `Limited` with the uses left, the exact number the player gets (§8.2 timing) —
    /// and, over the top of it, the **contextual** `Unusable` (#345): a press that
    /// would be refused for want of a target, from the one precondition ladder
    /// ([`aim`](Self::aim)) the turn loop itself gates on.
    ///
    /// # The precedence rule
    ///
    /// **The economy is asked first, and the context only speaks when the economy has
    /// nothing left to say.** `Active`, `Cooling`, `Exhausted` and `Passive` are
    /// returned untouched, whatever the surroundings; only `Ready` and `Limited` —
    /// the two that mean *press it now* — can be overruled into `Unusable`.
    ///
    /// One rule for every pair, and the reasoning is the same one that ranks the
    /// economy's own states ([`Deck::state`](crate::ability::Deck::state)): each
    /// state should report the fact that actually governs the ability right now.
    ///
    /// - **`Active` + no target.** The effect is *running*. A missing target is a
    ///   fact about the *next* press, and blanking the window the player is currently
    ///   playing off to say so would be a lie about the ability's own state.
    /// - **`Cooling` + no target.** The clock leads, because it carries a number and
    ///   `Unusable` carries a dash. Both gates are shut; only one of them can be
    ///   counted, and the other the player fixes with a step.
    /// - **`Exhausted` + no target.** Exhausted, which draws identically (`—`) and is
    ///   the deeper fact: no target is about this cell, a spent budget is about the
    ///   rest of the facility.
    /// - **`Ready`/`Limited` + no target → `Unusable`.** Here and only here the bar
    ///   would otherwise say *press me* about a press that cannot fire — `Bore(3)`
    ///   from the middle of a room — which is exactly §8.2's advertised-vs-real gap,
    ///   transposed from a number to a state. The uses count is worth losing for that:
    ///   it is one step away from being shown again, and a supply is no use where the
    ///   ability cannot be spent anyway.
    ///
    /// The press itself is **unchanged** either way (§4.4): an `Unusable` entry's key
    /// still resolves to the activation that refuses for free, and still speaks
    /// (§11.7). This decides what the player is told, never what they are charged.
    pub fn ability_state(&self, id: AbilityId) -> AbilityState {
        match self.abilities.state(id) {
            // `Ready`/`Limited` are the states that promise the press does something.
            // They are the only ones the context may overrule, and it overrules them
            // to the state the catalog has always documented as "discoverable, but
            // greyed" (§11.4).
            AbilityState::Ready | AbilityState::Limited { .. } if !self.would_fire(id) => {
                AbilityState::Unusable
            }
            economy => economy,
        }
    }

    /// What pressing ability `id`'s shortcut does **right now** (§11.6/#304): the
    /// one place the `Activate`/`Deactivate` choice is made.
    ///
    /// §4.4 grants two free actions, and one of them is toggling an ability *off*,
    /// so a held ability's key is a toggle: an [`Active`](AbilityState::Active)
    /// ability switches off ([`Input::Deactivate`]), anything else switches on
    /// ([`Input::Activate`]). A `Cooling` or `Unusable` press resolves to the
    /// activation that refuses for free, exactly as it did before, and a **passive**
    /// (§8.2/#264) resolves to `Activate` too — it can never be switched off, and
    /// its activation is the free no-op it has always been, so the `(on)` marker
    /// never becomes a toggle. A budgeted ability (§8.2/#302) is the same shape: with
    /// uses left it activates, and [`Exhausted`](AbilityState::Exhausted) it resolves
    /// to the activation that refuses for free, exactly as `Unusable` does.
    ///
    /// Both input paths call this — a digit after
    /// [`ability_slot_for_code`](crate::ability_slot_for_code), a tap after
    /// [`ability_at`](crate::ability_at), the two of them meeting at
    /// [`ability_in_slot`](crate::ability_in_slot) — so a key and the bar entry it
    /// names can never disagree (§11.4/#359) and neither shell duplicates the rule.
    /// The key's live meaning is already on screen before it is pressed: the bar
    /// draws `Run[3]` while active and `Run` while ready
    /// ([`AbilityStatus::bar_entry`]).
    pub fn ability_input(&self, id: AbilityId) -> Input {
        match self.ability_state(id) {
            AbilityState::Active { .. } => Input::Deactivate(id),
            AbilityState::Ready
            | AbilityState::Cooling { .. }
            | AbilityState::Limited { .. }
            | AbilityState::Exhausted
            | AbilityState::Passive
            | AbilityState::Unusable => Input::Activate(id),
        }
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

    /// What the next press would do from here — the **usable line** (§11.4): each
    /// interaction available where the player stands, with the direction to press
    /// for it, in [`Direction::ALL`] order. The §10.6 one-usable guarantee keeps
    /// this to a single entry on generated boards; a hand-built state may list
    /// more, one per direction.
    ///
    /// This mirrors [`step`](Self::step)'s bump resolution case for case, so
    /// the line can never promise what a bump won't deliver: an **unaware**
    /// guard offers the takedown while an aware one offers nothing (§7.2), a
    /// spent console and an occupied cupboard are just solid, and door poses
    /// come from the same door graph the bump consults (§10.4). Each target must
    /// also be in the player's FOV — which the touching ring always is (§6.2) —
    /// so the line can never leak what the fog still hides (§11.5a).
    ///
    /// # Why the direction is optional (#451)
    ///
    /// Every entry used to be a bump, so every entry had a direction. Taking hold of
    /// a body is the first that is not: it is a **wait**, and it is about the cell the
    /// player is standing **on** (§8.3). `None` says exactly that — *here, no
    /// direction* — rather than picking a neighbour the press has nothing to do with,
    /// which is the one thing §11.4 forbids this line: it must never promise what the
    /// next press will not deliver. The renderer reads the `None` and draws the entry
    /// without an arrow (§11.4/#384); nothing else in the game has to care.
    ///
    /// The mirror-the-bump discipline is unweakened, only widened from *bump* to
    /// *press*: the standing-on entry is derived from the same state the
    /// [`Input::Wait`](crate::Input::Wait) arm acts on, so it appears exactly when the
    /// wait would take hold and never otherwise.
    pub fn affordances(&self) -> Vec<(Option<Direction>, Affordance)> {
        // A stunned player bumps nothing (§8.3/#329): every input is swallowed, so
        // offering an interaction here would promise exactly what the next press will
        // not deliver (§2.3). The same silence a phased player already gets, for the
        // same reason — and it covers the standing-on entry too, since a stunned wait
        // is not a wait at all (it takes hold of nothing).
        if self.stunned > 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        // The cell underfoot comes first: it is the only entry that is not aimed, and
        // reading it before the ring keeps the line's order a fact about the state
        // rather than about the loop.
        if self.take_body_offered() {
            out.push((None, Affordance::TakeBody));
        }
        for dir in Direction::ALL {
            // The way out (§4.5/#466): from the exit tunnel's border cell, the step
            // that leaves the board is the run's last decision, and it is the first
            // affordance whose arrow points **off** the grid (§11.4/#384). It carries
            // no FOV gate — there is no cell out there to have seen, and the tunnel
            // you dug yourself is not something the fog can hide from you.
            //
            // It is held back until the player has **been inside** though
            // ([`entered_the_facility`]): the run opens standing on that very cell, and a
            // row whose only entry is *leave* is the wrong first thing to say to someone
            // who has not been in yet. Like the FOV gate below, this is the predictor's
            // alone — the bump still answers (§4.5), so pressing outward anyway is
            // told why rather than silently refused.
            //
            // [`entered_the_facility`]: Self::entered_the_facility
            if !self.entered_the_facility {
                continue;
            }
            if let Some(kind) = self.way_out_kind(dir) {
                if let Some(a) = kind.affordance() {
                    out.push((Some(dir), a));
                }
                continue;
            }
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
                out.push((Some(dir), a));
            }
        }
        out
    }

    /// Whether a wait from here would **take hold of a body** (§8.3/#451) — the
    /// predicate behind [`Affordance::TakeBody`], and the same one the
    /// [`Input::Wait`](crate::Input::Wait) arm acts on, so the line and the press
    /// cannot disagree (§11.4).
    ///
    /// Free hands and a loose body underfoot, and that is the whole rule. It needs no
    /// FOV gate: the player's own cell is never fogged, and a body you are standing on
    /// is not something the line could leak. **Phased**, it is silent along with
    /// everything else — a phased player passes through the body rather than standing
    /// on it, and cannot bump or grab anything (§8.3) — which falls out of
    /// [`can_rematerialize`](Self::can_rematerialize) being the wrong question here and
    /// the phase check being the right one.
    fn take_body_offered(&self) -> bool {
        if self.dragging.is_some() || self.abilities.effect_active(Effect::Phase) {
            return false;
        }
        self.body_at(self.player).is_some()
    }
}
