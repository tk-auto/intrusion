//! The player-policy seam (§13.2): `state → Input`, behind a trait.
//!
//! The harness never decides what the player does — a policy does, one
//! decision per issued input, reading the same pure [`State`] the renderer
//! draws from. The first policy is [`Scripted`] — replay a fixed input list —
//! which is all determinism testing needs; the baseline stealth bot is its own
//! ticket (§13.2's companion).

use intrusion_core::{Input, State};

/// A player for the headless harness: asked once per issued input what to do
/// with the state as it stands. Policies may keep state of their own (a script
/// cursor, a plan), so `decide` takes `&mut self`; a fresh policy per run keeps
/// runs independent.
pub trait PlayerPolicy {
    /// The next input to feed [`State::step`].
    fn decide(&mut self, state: &State) -> Input;
}

/// The scripted policy (§13.2): replay a fixed input list, then hold with
/// [`Input::Wait`].
///
/// Holding — rather than ending the run — keeps the world honest after the
/// script runs dry: patrols keep sweeping and can still capture an idle
/// player, and the harness's input cap rules the run a timeout if nothing
/// ends it first. A replay is `(seed, [inputs])` (§12.4), so this policy plus
/// a seed *is* the bug-report format.
pub struct Scripted {
    script: Vec<Input>,
    cursor: usize,
}

impl Scripted {
    /// A policy that replays `script` from its start, then waits forever.
    pub fn new(script: Vec<Input>) -> Self {
        Self { script, cursor: 0 }
    }
}

impl PlayerPolicy for Scripted {
    fn decide(&mut self, _state: &State) -> Input {
        let input = self.script.get(self.cursor).copied();
        self.cursor += 1;
        input.unwrap_or(Input::Wait)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intrusion_core::{generate_level, Direction, LevelConfig, Rng, State};

    /// Any placed level serves as a state to poll the policy against — the
    /// scripted policy never reads it, which is exactly what makes it a replay.
    fn any_state() -> State {
        let (layout, placement) =
            generate_level(&LevelConfig::V1, &mut Rng::new(0)).expect("the V1 config generates");
        State::new(
            layout,
            placement.player(),
            Direction::North,
            placement.guards(),
            placement.intel().iter().copied(),
            placement.exit(),
        )
    }

    /// The script replays in order, then the policy holds with Wait — it never
    /// runs out, so a stuck run is the harness's cap to end, not a hang.
    #[test]
    fn a_script_replays_in_order_then_holds_with_wait() {
        let state = any_state();
        let script = vec![
            Input::Step(Direction::North),
            Input::Step(Direction::East),
            Input::Wait,
        ];
        let mut policy = Scripted::new(script.clone());
        for &expected in &script {
            assert_eq!(policy.decide(&state), expected);
        }
        for _ in 0..3 {
            assert_eq!(policy.decide(&state), Input::Wait, "exhausted: holds");
        }
    }
}
