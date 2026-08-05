//! What the wallet owes a run (§2.2/§14 v3): that it only grows by banking and only
//! shrinks by a spend that could refuse, that a refusal costs nothing, and that both
//! refusals say which one they are.

use super::*;

/// **A wallet starts empty** (§2.2: nothing carries into a run), and banking is the only
/// way that changes.
#[test]
fn a_fresh_wallet_is_empty_and_fills_only_by_banking() {
    let mut wallet = Wallet::empty();
    assert_eq!(wallet.balance(), 0);
    assert_eq!(Wallet::default(), Wallet::empty());

    wallet.bank(3);
    wallet.bank(4);
    assert_eq!(wallet.balance(), 7, "hauls accumulate within the run");
}

/// **Banking saturates rather than wraps.** Unreachable in a real run, and the point is
/// which way it fails: a run that robbed the country blind must not be handed an empty
/// wallet for it.
#[test]
fn banking_saturates() {
    let mut wallet = Wallet::empty();
    wallet.bank(u32::MAX);
    wallet.bank(9);
    assert_eq!(wallet.balance(), u32::MAX);
}

/// **A spend debits exactly what it asked for**, and says what is left — the balance the
/// next decision is made against.
#[test]
fn an_affordable_spend_debits_and_reports_the_rest() {
    let mut wallet = Wallet::empty();
    wallet.bank(10);

    let outlay = wallet.spend(4);
    assert!(outlay.paid());
    assert_eq!(
        outlay,
        Outlay::Paid {
            cost: 4,
            balance: 6
        }
    );
    assert_eq!(outlay.balance(), Some(6));
    assert_eq!(wallet.balance(), 6);

    // Exactly affordable is affordable: the boundary is `>=`, so a price you can just
    // meet is a price you can pay.
    assert!(wallet.spend(6).paid());
    assert_eq!(wallet.balance(), 0);
}

/// **A refused spend changes nothing** — no partial payment, so a sink that branches on
/// [`Outlay::paid`] can never leave the money gone and the effect unapplied.
#[test]
fn an_unaffordable_spend_costs_nothing_and_names_the_shortfall() {
    let mut wallet = Wallet::empty();
    wallet.bank(3);

    let outlay = wallet.spend(5);
    assert!(!outlay.paid());
    assert_eq!(
        outlay,
        Outlay::Short {
            cost: 5,
            balance: 3
        }
    );
    assert_eq!(outlay.balance(), None, "a refusal left the balance alone");
    assert_eq!(wallet.balance(), 3, "the wallet is untouched");
}

/// **`affords` and `spend` agree**, at the boundary and either side of it — the property
/// that lets a sink *show* a price as unaffordable without pressing the key.
#[test]
fn affords_answers_what_spend_would_do() {
    let mut wallet = Wallet::empty();
    wallet.bank(5);
    for cost in 0..=8 {
        let mut probe = wallet;
        assert_eq!(
            wallet.affords(cost),
            probe.spend(cost).paid(),
            "affords({cost}) must answer what spend({cost}) does",
        );
    }
    // Free is always affordable, empty wallet included — a sink priced at zero is a
    // design decision, not something the wallet should refuse.
    assert!(Wallet::empty().affords(0));
}

/// **The three answers are three different sentences.** A player told "not enough intel"
/// when the truth is "not here" has been told the wrong fact, so the wordings must not
/// collapse into one another.
#[test]
fn every_outlay_says_which_one_it_is() {
    let paid = Outlay::Paid {
        cost: 5,
        balance: 2,
    }
    .message();
    let short = Outlay::Short {
        cost: 5,
        balance: 2,
    }
    .message();
    let closed = Outlay::Closed.message();

    assert!(paid.contains('5') && paid.contains('2'), "{paid:?}");
    assert!(short.contains('5') && short.contains('2'), "{short:?}");
    assert_ne!(paid, short);
    assert_ne!(short, closed);
    assert_ne!(paid, closed);
    // Every one of them names the currency in the world's own word (§11.8).
    for message in [&paid, &short, &closed] {
        assert!(message.contains("intel"), "{message:?}");
    }
}
