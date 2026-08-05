//! The **intel wallet** (§2.2/§14 v3): the run's currency, and the one way to spend it.
//!
//! §2.2's table says intel *"accumulates and is spent"* within a run and carries out of
//! none. The accumulating half has been here since the campaign layer existed — every
//! completed raid banks its haul ([`Campaign::bank`](super::Campaign)) — and this is the
//! half that makes it a **currency** rather than a score: a balance something can be
//! taken out of, and an answer when there is not enough in it.
//!
//! # Why intel is not the exit key any more
//!
//! In quick play intel is what the exit asks for (§4.5): gather the set, then leave. The
//! campaign takes that gate off ([`IntelGate::None`](crate::IntelGate)) and the two rules
//! cannot both hold, because a currency you must spend on the way out is not a currency —
//! it is a toll. So in a campaign **extraction is voluntary**: intel, caches and every
//! unlockable in a facility are *surplus*, the exit never refuses, and what a raid was
//! worth is settled at the hub afterwards rather than at the mouth of the tunnel.
//!
//! That is a deliberate relaxation and not an oversight — see appendix 47, which also
//! records why walking out empty-handed carries **no** explicit penalty: the campaign
//! escalates on its own (#210) and caches are one-shot, so a wasted raid already leaves
//! the run weaker for a harder facility. A punishment on top would be charging twice for
//! the same mistake.
//!
//! # One spend context
//!
//! There is **no in-level spending**. A wallet you could dip into mid-raid would make
//! every tight corner a shop, and the §4.4 turn-cost rule the one thing standing between
//! the player and buying their way out of a mistake. Spending happens at the map between
//! facilities, which is what [`Outlay::Closed`] refuses everywhere else — the check is
//! [`Campaign::spend`](super::Campaign::spend)'s, so a sink cannot forget to make it.

#[cfg(test)]
mod tests;

/// The run's **intel balance** (§2.2) — what it has harvested and not yet spent.
///
/// A newtype over a counter rather than a bare `u32` field, because the two operations it
/// permits are not the two a counter permits: intel goes **in** whole (a raid's haul) and
/// comes **out** only through a [`spend`](Self::spend) that can refuse. Nothing outside
/// this type may set the balance, so "the wallet went negative" and "a sink debited
/// without checking" are not states the rest of the campaign can reach.
///
/// Nothing persists it, which is §2.2's across-runs half needing no code: a wallet is a
/// plain value inside a [`Campaign`](super::Campaign), and a finished run drops both.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Wallet {
    balance: u32,
}

impl Wallet {
    /// An empty wallet — where every run starts (§2.2: nothing carries in).
    pub const fn empty() -> Self {
        Self { balance: 0 }
    }

    /// What the run is carrying.
    pub const fn balance(self) -> u32 {
        self.balance
    }

    /// **Bank a raid's haul.** Saturating, because a balance that wrapped would hand a
    /// run an empty wallet as a reward for a very good raid; the ceiling is unreachable
    /// in a 2–3 hour run and the honest failure is to stop counting rather than to lie.
    pub fn bank(&mut self, taken: u32) {
        self.balance = self.balance.saturating_add(taken);
    }

    /// Whether `cost` is affordable — what a sink asks before it offers, so a price the
    /// run cannot meet can be *shown* as unaffordable rather than only discovered by
    /// pressing the key.
    pub const fn affords(self, cost: u32) -> bool {
        self.balance >= cost
    }

    /// **Spend `cost`**, or refuse and change nothing.
    ///
    /// The refusal is the point of the return type: a sink calls this and is *told* what
    /// happened, in words it can put on a screen ([`Outlay::message`]), rather than
    /// getting a `bool` and inventing its own wording. A refused spend leaves the balance
    /// exactly where it was — there is no partial payment.
    #[must_use]
    pub fn spend(&mut self, cost: u32) -> Outlay {
        if !self.affords(cost) {
            return Outlay::Short {
                cost,
                balance: self.balance,
            };
        }
        self.balance -= cost;
        Outlay::Paid {
            cost,
            balance: self.balance,
        }
    }
}

/// What a spend did (§14 v3) — the answer every sink gets back, and the only way the
/// wallet talks.
///
/// Three answers rather than two, because *"you cannot afford that"* and *"you cannot do
/// that here"* are different facts and a player told the wrong one is being lied to. Both
/// refusals leave the run untouched.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outlay {
    /// Bought: `cost` came out and `balance` is what is left.
    Paid { cost: u32, balance: u32 },
    /// **Refused — not enough intel.** `cost` is what was asked and `balance` is what the
    /// run has; both are carried so the message can name the shortfall rather than say
    /// "no".
    Short { cost: u32, balance: u32 },
    /// **Refused — not at the hub.** The run is inside a facility or it is over, and
    /// there is no spending in either (see the module docs).
    Closed,
}

impl Outlay {
    /// Whether the thing was actually bought — the one question a sink branches on before
    /// applying its effect.
    pub const fn paid(self) -> bool {
        matches!(self, Outlay::Paid { .. })
    }

    /// What the intel came to, after. `None` for a refusal, which changed nothing.
    pub const fn balance(self) -> Option<u32> {
        match self {
            Outlay::Paid { balance, .. } => Some(balance),
            _ => None,
        }
    }

    /// **What to tell the player**, in the world's own word for the currency (§11.8:
    /// *intel* needs no translation) and in the message register §11.7 uses — a statement
    /// of the fact, not an apology for it.
    ///
    /// The wording lives here rather than in the renderer so that every sink refuses in
    /// the same voice, and so a test can pin what a refusal says without drawing a screen.
    /// A purchase says what it cost and what is left, because the balance is the next
    /// decision's input and the hub should not have to be re-read to find it.
    pub fn message(self) -> String {
        match self {
            Outlay::Paid { cost, balance } => format!("spent {cost} intel — {balance} left"),
            Outlay::Short { cost, balance } => format!("needs {cost} intel — you have {balance}"),
            Outlay::Closed => "nothing to spend intel on in here".to_string(),
        }
    }
}
