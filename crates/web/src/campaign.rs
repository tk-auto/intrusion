//! The shell's **campaign driver** (§14 v3/§12.7, #208): the loop above the turn loop.
//!
//! Quick play is one facility, so the shell's whole life is *menu → run → end screen*.
//! A campaign puts a second loop around that — *map → raid → map → raid → …* — and this
//! module is the four lines of bookkeeping it takes:
//!
//! | | |
//! |---|---|
//! | [`start_campaign`](Game::start_campaign) | Story mode: a fresh run, and the map over it |
//! | [`raid`](Game::raid) | a row was fired: move if it is a move, then boot the facility |
//! | [`campaign_verdict`](Game::campaign_verdict) | a raid ended: bank it, and raise the map or the end screen |
//!
//! Every rule it applies belongs to [`Campaign`], not here: which facilities are on
//! offer, whether a node may be taken, what a completed raid does to the run. The shell
//! contributes the one thing the core cannot — **a reading of the clock** for the run
//! seed — and otherwise only asks and draws.
//!
//! # Where the end screen went
//!
//! A campaign facility that is **escaped** does not raise the end screen. The map comes
//! up instead, with the haul already banked, because a completed facility is not the end
//! of anything — it is the middle of the run, and a "you won" card between every raid
//! would be seven endings in a game that has one (§2.2).
//!
//! The end screen keeps the endings that *are* endings: **captured**, which is terminal
//! for the whole run wherever it happened, and **the archive left behind**, which is the
//! run won. Both are already what [`CampaignStage::is_over`] answers, so the shell asks
//! that rather than deciding for itself.

use intrusion_core::{Campaign, CampaignStage, MapHit, MapNav, MapUi, NodeId, ScreenUi, Verdict};

use crate::menu::{SCREEN_MAP, SCREEN_PLAY};
use crate::{seed, Game};

impl Game {
    /// **Start a campaign** (§14 v3): a fresh run off the clock, with the map over it.
    ///
    /// The seed is the shell's one impurity, exactly as quick play's is ([`seed`]), and
    /// it fixes the **whole country** — every facility, every flavour, every choice
    /// point (§12.4). Nothing is generated yet: the first facility is carved when the
    /// player picks a row, and the graph grows a node at a time under them.
    pub(crate) fn start_campaign(&mut self) {
        self.campaign = Some(Campaign::new(seed::clock_seed()));
        self.show_map();
    }

    /// Raise the campaign map over whatever is on screen and paint it.
    ///
    /// The marker resets to the top row every time, deliberately: the list is a
    /// different list at every choice point, so carrying an index across would put the
    /// marker somewhere the player did not leave it.
    ///
    /// **The title screen is dropped as the map goes up**, and that is not tidiness: the
    /// screens are modal, and each pump asks *is the menu up?* before it asks *is the map
    /// up?*. A menu left set under the map would keep the keyboard while the map had the
    /// canvas — the screen you can see and the screen you are typing at would be two
    /// different screens, which is the trap §11.6 is about. The one place that decides a
    /// screen is showing is the one place that must clear the last one.
    pub(crate) fn show_map(&mut self) {
        self.ui = ScreenUi {
            map: Some(MapUi::default()),
            menu: None,
            ..self.ui
        };
        self.draw();
        crate::menu::set_screen(SCREEN_MAP);
    }

    /// Apply a [`MapNav`] from the map screen — walk the list, or raid the marked
    /// facility.
    pub(crate) fn apply_map_nav(&mut self, nav: MapNav) {
        let Some(ahead) = self.campaign.as_ref().map(|run| run.ahead()) else {
            return;
        };
        let Some(map) = self.ui.map else { return };
        match nav {
            MapNav::Prev => self.select_facility(map.prev(&ahead)),
            MapNav::Next => self.select_facility(map.next(&ahead)),
            MapNav::Activate => {
                if let Some(&offer) = ahead.get(map.selected(&ahead)) {
                    self.apply_map_hit(intrusion_core::hit_of(offer));
                }
            }
            MapNav::ToggleTheme => {
                self.ui.theme = self.ui.theme.toggled();
                self.draw();
            }
        }
    }

    /// Apply a press on the map screen (§11.6) — the touch half of
    /// [`apply_map_nav`](Self::apply_map_nav).
    ///
    /// A tap on a row **raids it outright**, exactly as a tap on a menu entry starts a
    /// run: the row is the target, and the arm-on-press / fire-on-lift path the tap
    /// pump already enforces is what keeps a stray press off it (#306).
    pub(crate) fn apply_map_hit(&mut self, hit: MapHit) {
        match hit {
            MapHit::Facility(node) => self.raid(node),
            MapHit::Unlock(node) => self.unlock(node),
            MapHit::ToggleTheme => {
                self.ui.theme = self.ui.theme.toggled();
                self.draw();
            }
        }
    }

    /// **Buy the alternative route** (§14 v3/#212) and say what happened.
    ///
    /// The rule is [`Campaign::unlock`]'s — whether the row is a locked offer, whether the
    /// wallet covers it, what comes out — and the shell's whole part is putting the answer
    /// somewhere the player reads it. Paid or refused, the map redraws with the outlay on
    /// the wallet line; a paid one has already flipped the row to takeable underneath it,
    /// so the next press raids where this one bought.
    ///
    /// The map does **not** come back up from scratch: `show_map` resets the marker, and a
    /// purchase should leave the player on the row they just bought rather than back at the
    /// top of a list they were part-way down.
    fn unlock(&mut self, node: NodeId) {
        let Some(run) = self.campaign.as_mut() else {
            return;
        };
        let outlay = run.unlock(node);
        if let Some(map) = self.ui.map {
            self.ui.map = Some(map.saying(outlay));
        }
        self.draw();
    }

    /// Move the marker and repaint.
    fn select_facility(&mut self, map: MapUi) {
        self.ui.map = Some(map);
        self.draw();
    }

    /// **Raid `node`**: take the edge to it if it is one, then boot the facility and drop
    /// the map.
    ///
    /// The two halves the map screen shows as one row are separated here and nowhere
    /// else: at a choice point the run has to *move* first ([`Campaign::choose`]), and on
    /// the approach it is already standing where it means to raid. Either way the boot is
    /// [`Campaign::enter`], which hands out the facility's [`LevelSeed`] with the node's
    /// flavour resolved into it.
    ///
    /// A refused choice or a refused entry leaves the map exactly as it was rather than
    /// dropping the player onto a broken board — the same belt-and-braces
    /// [`start_run`](crate::menu) uses, and the reason the core answers with `bool` and
    /// `Option` rather than asserting.
    pub(crate) fn raid(&mut self, node: NodeId) {
        let Some(run) = self.campaign.as_mut() else {
            return;
        };
        if run.stage() == CampaignStage::Choosing && !run.choose(node) {
            return;
        }
        let Some(level) = run.enter() else { return };
        // The run's **framing** (§2.2/appendix 31), taken from the campaign rather than
        // assumed: it is what gates the exits the end screen will offer if this raid is
        // the one that ends the run.
        let run_options = run.run_options();
        // `reseed` clears the view state, which is what drops the map — one place decides
        // that a fresh facility shows no chrome (#473).
        if self.reseed(level).is_ok() {
            self.ui.end.options = run_options;
            seed::reflect_level(&level);
            crate::menu::set_screen(SCREEN_PLAY);
        } else {
            // The facility did not carve, so the run never entered it: put the campaign
            // back at the choice point it was at rather than leaving it standing inside
            // a facility that does not exist.
            self.show_map();
        }
    }

    /// Fold a finished raid into the campaign, and decide what the player sees next
    /// (§12.7).
    ///
    /// Called at the one seam a verdict can appear at — the end of a step — and a no-op
    /// outside a campaign, so quick play keeps the end screen it has always had. A
    /// **finished** campaign is likewise no longer listening: the run is dropped when the
    /// player leaves for the menu ([`show_menu`](crate::menu)), so a later quick-play
    /// verdict cannot arrive at a layer that has nothing to do with it.
    ///
    /// - **Escaped**, with the run still going: the haul banks and the **map** comes up.
    /// - **Escaped from the archive**, or **captured**: the run is over, and the end
    ///   screen draws itself off the state's own verdict as it always does.
    pub(crate) fn campaign_verdict(&mut self, verdict: &Verdict) {
        let Some(run) = self.campaign.as_mut() else {
            return;
        };
        if run.stage() != CampaignStage::Inside || run.complete(verdict).is_over() {
            return;
        }
        self.show_map();
    }

    /// Whether a campaign is under way and its map is the surface showing — the one
    /// question the paint loop and the input pumps both ask.
    pub(crate) fn map_open(&self) -> bool {
        self.ui.map.is_some() && self.campaign.is_some()
    }
}
