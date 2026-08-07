use super::*;

/// The design's notation, pinned (§11.4): ready shows only the name, active is
/// `[N]`, cooling is `/N/`, unusable is a lone dash — and the number is the
/// state's own, rendered verbatim (§8.2).
#[test]
fn each_state_formats_in_the_design_notation() {
    assert_eq!(AbilityState::Ready.suffix(), "");
    assert_eq!(AbilityState::Active { remaining: 3 }.suffix(), "[3]");
    assert_eq!(AbilityState::Cooling { remaining: 2 }.suffix(), "/2/");
    assert_eq!(AbilityState::Passive.suffix(), "(on)");
    assert_eq!(AbilityState::Unusable.suffix(), "—");
}

/// The **use-budget notation** (§8.2/#302), pinned: a count in parentheses, the
/// shape [`PASSIVE_MARKER`] uses — because neither is a timer — and never a
/// clock's brackets or slashes. Exhausted borrows the unusable dash rather than
/// inventing a `(0)`: `(0)` would read as a number you still have.
#[test]
fn a_use_budget_reads_as_a_count_not_a_countdown() {
    assert_eq!(AbilityState::Limited { uses: 3 }.suffix(), "(3)");
    assert_eq!(AbilityState::Limited { uses: 1 }.suffix(), "(1)");
    assert_eq!(
        AbilityState::Exhausted.suffix(),
        AbilityState::Unusable.suffix(),
        "spent draws as unusable, because that is what it is",
    );
    assert_ne!(AbilityState::Limited { uses: 0 }.suffix(), "—");

    // The states themselves stay distinct, so nothing can quietly treat a
    // budgeted ability as an unbounded one or a spent one as merely cooling.
    assert_ne!(AbilityState::Limited { uses: 2 }, AbilityState::Ready);
    assert_ne!(AbilityState::Exhausted, AbilityState::Unusable);
    assert_ne!(AbilityState::Exhausted, AbilityState::Ready);
    for n in 0..4 {
        assert_ne!(
            AbilityState::Exhausted,
            AbilityState::Cooling { remaining: n }
        );
        assert_ne!(
            AbilityState::Limited { uses: n },
            AbilityState::Active { remaining: n }
        );
    }
}

/// A budgeted entry draws its count against the bar name, exactly as the ticket's
/// worked example reads — `Bore(2)` — and the widest one a legal budget can
/// produce still fits the per-entry budget (§11.4). The single-digit fence is a
/// `const` assertion over the catalogue; this pins what that fence buys.
#[test]
fn a_budgeted_bar_entry_fits_the_row() {
    let entry = |id, state| AbilityStatus { id, state }.bar_entry();
    assert_eq!(
        entry(AbilityId::Decoy, AbilityState::Limited { uses: 2 }),
        "Decoy(2)"
    );
    assert_eq!(entry(AbilityId::Run, AbilityState::Exhausted), "Run—");
    for id in AbilityId::ALL {
        let widest = entry(id, AbilityState::Limited { uses: 9 });
        assert!(
            widest.chars().count() <= MAX_BAR_ENTRY,
            "{widest:?} overflows the per-entry budget",
        );
    }
}

/// A bar entry is the ability's **bar name** with the notation tucked straight
/// against it (§11.4) — a ready ability is the bare name, with no trailing
/// bracket or space to pay for. The name comes from the identity, so the entry
/// is built from an [`AbilityId`].
#[test]
fn a_bar_entry_is_the_bar_name_and_any_notation() {
    let entry = |id, state| AbilityStatus { id, state }.bar_entry();
    assert_eq!(entry(AbilityId::Run, AbilityState::Ready), "Run");
    assert_eq!(
        entry(AbilityId::Camouflage, AbilityState::Active { remaining: 7 }),
        "Camo[7]"
    );
    assert_eq!(
        entry(AbilityId::Decoy, AbilityState::Cooling { remaining: 12 }),
        "Decoy/12/"
    );
    assert_eq!(
        entry(AbilityId::Autodoors, AbilityState::Unusable),
        "Doors—"
    );
}

/// A passive reads `(on)` where an activated ability reads its clock (#264/#287)
/// — the marker #264 deferred to this rework. Undecorated it would have sat on
/// the bar looking exactly like the ready abilities beside it, which is the one
/// thing it is not: there is nothing to press.
///
/// What had to survive that decision is the *state*: `Passive` is still its own
/// case, never `Active { .. }` — so nothing can start showing a countdown for
/// an ability that never counts down.
#[test]
fn a_passive_reads_as_always_on_and_is_still_its_own_state() {
    assert_eq!(AbilityState::Passive.suffix(), PASSIVE_MARKER);
    let status = AbilityStatus {
        id: AbilityId::Vision,
        state: AbilityState::Passive,
    };
    assert_eq!(status.bar_entry(), "Sight(on)");
    // Marked, never the same *state* as Ready or a clock.
    assert_ne!(AbilityState::Passive, AbilityState::Ready);
    for n in 0..4 {
        assert_ne!(AbilityState::Passive, AbilityState::Active { remaining: n });
        assert_ne!(
            AbilityState::Passive,
            AbilityState::Cooling { remaining: n }
        );
    }
}

/// **A bar name may shorten its full name, never negate it** (§11.4/§11.8/#415).
///
/// The bar draws `Phase` and the help panel draws **Phase Out**, and the pair is
/// only legible because the short one is a plain truncation of the long one. It
/// was not always: while the §8.3 word *Dephase* was also the screen's word,
/// `Phase` read as its **opposite**, so a player who learned the bar name learned
/// the wrong verb. The rename fixed that, and this pins it — the prefix is the
/// property, so the pair cannot drift back into naming opposites.
///
/// Only where the bar name is a *shortening* at all: `Daze`, `Bore` and `Sight`
/// are deliberately different words (§11.4 — a plain word a player can say, not an
/// abbreviation to decode), and a different word cannot be read as a negation of
/// the one it stands in for.
#[test]
fn a_shortened_bar_name_is_a_prefix_of_the_full_one() {
    assert_eq!(AbilityId::Dephase.name(), "Phase Out");
    assert_eq!(AbilityId::Dephase.bar_name(), "Phase");
    for id in AbilityId::ALL {
        let (name, bar) = (id.name(), id.bar_name());
        let shares_a_start = name.chars().next() == bar.chars().next();
        assert!(
            name.starts_with(bar) || !shares_a_start,
            "{bar:?} begins like {name:?} but is not its prefix — a bar name that \
                 starts the same way and then diverges reads as a *different* word, \
                 which is the misread §11.8 records for this pair",
        );
    }
}

/// **The bar's width budget, as arithmetic** (§11.4/#287). The widest notation is
/// read off the catalogue's own numbers — the longest `[N]`/`/N/` any §8.3 ability
/// can show, against the passive marker — and the widest entry is that plus the
/// longest bar name, so a retune or a rename moves these rather than silently
/// overflowing the row. The render turns them into a `const` assertion against
/// the board width; this pins the values that assertion is made of.
#[test]
fn the_bar_budget_is_measured_from_the_catalog() {
    // The longest number in the catalogue is Confusion's 45 → `/45/`, exactly as
    // wide as the passive `(on)`.
    assert_eq!(MAX_STATE_NOTATION, 4);
    assert_eq!(PASSIVE_MARKER.len(), MAX_STATE_NOTATION);
    // The longest bar name is five (`Decoy`/`Phase`/`Doors`/`Sight`).
    assert_eq!(max_bar_name(), 5);
    assert_eq!(MAX_BAR_ENTRY, 9);
    // No ability, in any state its own mode can reach, draws wider than that.
    for id in AbilityId::ALL {
        let mut states = vec![AbilityState::Unusable];
        match id.def().mode() {
            AbilityMode::Passive => states.push(AbilityState::Passive),
            AbilityMode::Activated(economy) => states.extend([
                AbilityState::Ready,
                AbilityState::Active {
                    remaining: economy.duration(),
                },
                AbilityState::Cooling {
                    remaining: economy.cooldown(),
                },
            ]),
        }
        for state in states {
            let entry = AbilityStatus { id, state }.bar_entry();
            assert!(
                entry.chars().count() <= MAX_BAR_ENTRY,
                "{entry:?} overflows the per-entry budget",
            );
        }
    }
}

/// The held-set cap (§8.3/#244/#266), the other half of the budget: innate Run
/// plus [`AbilityId::MAX_TECH_HELD`] tech. Counted off the catalogue, so promoting
/// an ability to innate moves it rather than leaving a stale number behind.
#[test]
fn the_held_cap_is_the_innate_set_plus_the_tech_grant() {
    assert_eq!(AbilityId::MAX_TECH_HELD, 3);
    assert_eq!(AbilityId::MAX_HELD, 4);
    assert_eq!(
        innate_count(),
        AbilityId::ALL.iter().filter(|id| id.is_innate()).count(),
    );
    assert!(
        AbilityId::MAX_TECH_HELD <= AbilityId::TECH.len(),
        "the grant cannot exceed the pool it draws from",
    );
}

/// **There is no experimental tier** (§0/§8.3/#564): every shipped ability that is
/// not innate is in [`AbilityId::TECH`], which is the one list a `starting_abilities`
/// draw (#244) and an equipment cache (#209) both come out of. So a verb cannot ship
/// held back from the pool — the state §2.3 calls inert, and the one a status marker
/// implying a second tier would have tempted someone into building.
#[test]
fn every_shipped_ability_is_innate_or_in_the_draw_pool() {
    for id in AbilityId::ALL {
        assert_eq!(
            id.is_innate(),
            !AbilityId::TECH.contains(&id),
            "{id:?} must be either innate or drawable, and exactly one of the two",
        );
    }
    assert_eq!(
        AbilityId::ALL.len(),
        AbilityId::INNATE.len() + AbilityId::TECH.len(),
        "the two lists partition the catalogue — nothing shipped sits outside both",
    );
}

/// An entry's name is its [`AbilityId`]'s, taken by identity — the bar draws what
/// the ability *is*, and only the key it answers to comes from its position
/// (§11.6/#359).
#[test]
fn an_entry_takes_its_name_from_its_identity() {
    for id in AbilityId::ALL {
        let status = AbilityStatus {
            id,
            state: AbilityState::Ready,
        };
        assert_eq!(status.name(), id.name());
    }
}
