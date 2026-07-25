//! Level modifiers — the §12.6 seam.
//!
//! A **level modifier** is a named toggle or bounded knob that shifts a
//! facility's *baseline* difficulty or rules *before the level begins*. Each one
//! flips a rule an existing system already owns — the §7.6 search, the §11.5
//! danger overlay — rather than adding a parallel one. The whole point is that
//! the coupling stays **visible** (§12.3): every modifier is a field on one plain
//! [`LevelModifiers`] value, resolved once at facility start, and every system
//! reads *that* value — never a global bool queried in ten places.
//!
//! # The source → modifier → config flow
//!
//! The *mechanism* here is shared; the *sources* that turn modifiers on are
//! separate and stack on top of it (§14 v3). There are three intended sources,
//! kept deliberately distinct:
//!
//! - **Choice** (exogenous) — the player's chosen or seeded baseline. The only
//!   source this crate ships; it is what a mode preset (#244) and a shared token
//!   (#245) set.
//! - **Alert** (endogenous) — the campaign alert (#210): a loud raid raises the
//!   alert, and a higher alert switches on harder modifiers for later facilities.
//!   [`ModifierSources`] exposes the hook it plugs into; it is `None` here.
//! - **Flavour** (per-node) — a facility's own character (#207), a third future
//!   source.
//!
//! They all resolve into the *same* [`LevelModifiers`] the systems read
//! ([`ModifierSources::resolve`]), so a new source is a new field and a line in
//! `resolve`, never a new difficulty path. Determinism (§12.4) is preserved
//! because the resolved set is plain [`Copy`] data threaded through the same seed
//! and inputs: same seed + same modifiers + same inputs → identical run.

/// The set of level modifiers active for a facility — resolved once at facility
/// start (§12.3) into the one value guards, vision, and render branch on.
///
/// Plain, heterogeneous data: a toggle is a `bool`, a bounded knob is a small
/// enum or clamped integer (none yet — the two shipped modifiers are both
/// toggles). Adding a modifier is adding a field, and the compiler then
/// enumerates every read site that must handle it (§12.2). Every field carries a
/// documented **direction** — *harder* raises pressure, *easier* lowers it — so a
/// directional assertion (§2.3, the anti-facade guard) can prove it bites.
///
/// [`Default`] is the **baseline**: every modifier off, the game exactly as it
/// plays without the system. Quick play (#244) boots from this default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelModifiers {
    /// **Harder.** Force the §7.6 search to flush an occupied hideout within its
    /// disc unconditionally — not only a *body* search (§10.3/#219). Baseline, a
    /// guard that merely lost a chase leaves the cupboard the safe wait-out it is
    /// (the "hold still, watch the cone sweep past" payoff, §7.6); with this on,
    /// any active search that sweeps over the cupboard you dived into flushes it.
    pub guards_always_search_hideouts: bool,
    /// **Easier.** Paint the §11.5 danger overlay in full — the cone of *every*
    /// guard, not only the ones you can currently see. This only ever **widens**
    /// what is revealed; it never hides the red detection set, so the §11.5
    /// [SETTLED] contract ("if your cell isn't red, no guard detects you") is
    /// kept and, if anything, strengthened.
    pub always_show_vision_cones: bool,
}

impl LevelModifiers {
    /// Compose two contributions into one active set. A toggle is active if
    /// **any** source requests it (field-wise OR) — sources add pressure, they do
    /// not cancel each other. When bounded knobs arrive they compose
    /// *harder-ward* (take the value further in its documented direction); add
    /// that here field by field as each knob lands, so the rule stays one place.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            guards_always_search_hideouts: self.guards_always_search_hideouts
                || other.guards_always_search_hideouts,
            always_show_vision_cones: self.always_show_vision_cones
                || other.always_show_vision_cones,
        }
    }
}

/// The independent *sources* that contribute modifiers to a facility, composed
/// into one resolved [`LevelModifiers`] at facility start.
///
/// This is the **activation hook** (§12.6): the campaign alert (#210) owns the
/// *mapping* from alert level to a modifier contribution and drops it into
/// [`alert`](Self::alert); node flavour (#207) is a future field. Each stays a
/// distinct source — alert is endogenous, choice is exogenous, flavour is
/// per-node — and [`resolve`](Self::resolve) is the single place they merge, so
/// no source grows a private knob set the seam should own.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModifierSources {
    /// **Choice** — the player's chosen or seeded baseline (this crate's source).
    pub chosen: LevelModifiers,
    /// **Alert** — the campaign-alert contribution (#210), or `None` when no
    /// campaign layer is driving difficulty (all of v1 quick play).
    pub alert: Option<LevelModifiers>,
    // Flavour (#207) is the third source: `pub flavour: Option<LevelModifiers>`.
}

impl ModifierSources {
    /// The choice source alone — the v1 quick-play path: a chosen baseline, no
    /// alert, no flavour. `ModifierSources::chosen(LevelModifiers::default())` is
    /// the plain default game.
    #[must_use]
    pub fn chosen(chosen: LevelModifiers) -> Self {
        Self {
            chosen,
            alert: None,
        }
    }

    /// Resolve every source into the one active set the systems read. Sources
    /// compose by [`LevelModifiers::union`]; a new source is composed in here.
    #[must_use]
    pub fn resolve(self) -> LevelModifiers {
        let mut active = self.chosen;
        if let Some(alert) = self.alert {
            active = active.union(alert);
        }
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_baseline_default_has_every_modifier_off() {
        let baseline = LevelModifiers::default();
        assert!(!baseline.guards_always_search_hideouts);
        assert!(!baseline.always_show_vision_cones);
    }

    #[test]
    fn the_choice_source_resolves_to_exactly_its_chosen_set() {
        let chosen = LevelModifiers {
            guards_always_search_hideouts: true,
            ..LevelModifiers::default()
        };
        // No alert, no flavour: the resolved set is the choice, untouched.
        assert_eq!(ModifierSources::chosen(chosen).resolve(), chosen);
    }

    #[test]
    fn the_alert_hook_composes_on_top_of_the_chosen_set() {
        // #210 drives this path: the player chose the easier overlay, and a loud
        // raid switches on the harder search for the next facility. Both survive
        // the merge — sources add, they do not cancel.
        let chosen = LevelModifiers {
            always_show_vision_cones: true,
            ..LevelModifiers::default()
        };
        let alert = LevelModifiers {
            guards_always_search_hideouts: true,
            ..LevelModifiers::default()
        };
        let resolved = ModifierSources {
            chosen,
            alert: Some(alert),
        }
        .resolve();
        assert!(resolved.always_show_vision_cones);
        assert!(resolved.guards_always_search_hideouts);
    }

    #[test]
    fn union_is_a_field_wise_or() {
        let a = LevelModifiers {
            guards_always_search_hideouts: true,
            always_show_vision_cones: false,
        };
        let b = LevelModifiers {
            guards_always_search_hideouts: false,
            always_show_vision_cones: true,
        };
        let both = a.union(b);
        assert!(both.guards_always_search_hideouts);
        assert!(both.always_show_vision_cones);
        // Union with the baseline changes nothing.
        assert_eq!(a.union(LevelModifiers::default()), a);
    }
}
