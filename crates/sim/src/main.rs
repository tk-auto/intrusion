//! Headless simulation harness (§13.2) — placeholder.
//!
//! The real thing runs *N* seeded games with a scripted or bot player and emits
//! the balance metrics in §13.2 (win rate, ability-usage histogram, strategy
//! diversity, …). That is its own ticket, and the `playtest` skill is blocked on
//! it. For now this is an empty-but-real binary target so the workspace layout
//! is complete and the gate exercises it.

use intrusion_core::Rng;

fn main() {
    // Prove the core links into the sim binary. Replace with the seeded-run loop
    // when the headless sim lands (§13.2).
    let mut rng = Rng::new(0);
    let _ = rng.next_u64();
    println!("intrusion-sim: headless harness not yet implemented (see design §13.2)");
}
