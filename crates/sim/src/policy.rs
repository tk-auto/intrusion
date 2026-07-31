//! The player-policy seam (§13.2): `state → Input`, behind a trait.
//!
//! The harness never decides what the player does — a policy does, one
//! decision per issued input, reading the same pure [`State`] the renderer
//! draws from. [`Scripted`] replays a fixed input list — all determinism testing
//! needs — while [`StealthBot`](crate::StealthBot) is the baseline bot that
//! actually plays (§13.2's companion), reading the same [`State`] through the
//! player's own channels.

use intrusion_core::{Input, State};

/// A player for the headless harness: asked once per issued input what to do
/// with the state as it stands. Policies may keep state of their own (a script
/// cursor, a plan), so `decide` takes `&mut self`; a fresh policy per run keeps
/// runs independent.
pub trait PlayerPolicy {
    /// The next input to feed [`State::step`].
    fn decide(&mut self, state: &State) -> Input;

    /// The name of the playstyle profile driving this policy, when there is one
    /// (§13.2) — the string every emitted row carries so a batch's output is
    /// attributable to the temperament that produced it.
    ///
    /// A scripted policy plays no temperament, so it answers `None` and its rows
    /// emit `null`: "no profile", never a fake `"balanced"` that would claim a
    /// bot ran the batch. Defaulted, so a new policy is honest by omission.
    fn profile_name(&self) -> Option<&'static str> {
        None
    }
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

/// A policy decorator that **records** every input its inner policy issues while
/// delegating each decision unchanged (§12.4). Wrapping the bot in one captures
/// the exact `[inputs]` half of a replay without the bot knowing it is watched;
/// the run itself is byte-identical to an unwrapped run, so the capture is
/// faithful. See [`capture_one`](crate::capture_one).
pub struct Recording<P> {
    inner: P,
    inputs: Vec<Input>,
}

impl<P> Recording<P> {
    /// Wrap `inner`, recording each input it issues.
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            inputs: Vec::new(),
        }
    }

    /// The inputs recorded so far, in issue order.
    pub fn inputs(&self) -> &[Input] {
        &self.inputs
    }

    /// Consume the decorator and take the recorded inputs.
    pub fn into_inputs(self) -> Vec<Input> {
        self.inputs
    }
}

impl<P: PlayerPolicy> PlayerPolicy for Recording<P> {
    fn decide(&mut self, state: &State) -> Input {
        let input = self.inner.decide(state);
        self.inputs.push(input);
        input
    }

    /// Transparent: a captured run reports the profile that actually played it,
    /// so a recorded row is attributable exactly like an unwrapped one.
    fn profile_name(&self) -> Option<&'static str> {
        self.inner.profile_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intrusion_core::{generate_level, Direction, LevelConfig, Rng, State};

    /// Any placed level serves as a state to poll the policy against — the
    /// scripted policy never reads it, which is exactly what makes it a replay.
    fn any_state() -> State {
        let (layout, placement) = generate_level(
            &LevelConfig::V1,
            &intrusion_core::LevelModifiers::default(),
            &mut Rng::new(0),
        )
        .expect("the V1 config generates");
        let guards = placement.guards(&layout);
        State::new(
            layout,
            placement.player(),
            Direction::North,
            guards,
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
