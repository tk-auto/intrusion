//! Replay inspection (§12.4/#411): read a run someone pasted and say what happened.
//!
//! This is the **read** half of [`--emit-replay`](crate::Replay). The sim could
//! already write a replay and the browser could already write one (the help panel's
//! copy control); what nothing could do was take one back and answer *"so what did
//! they actually do?"* — which is the question a pasted link is always really
//! asking.
//!
//! **It is not `--script`.** That flag replays a script and then keeps playing: the
//! policy waits forever past the script's end, over a whole batch of seeds, and
//! reports the balance metrics. Those are the right answers for a sweep and the
//! wrong ones here — a pasted link is one run, ends where its inputs end, and wants
//! its *trajectory* rather than its outcome. Feeding a 13-input link through
//! `--script` reports "capture at turn 61", when the true answer was "on turn 13 the
//! exit refused them".
//!
//! **It boots the way the browser does.** [`start_level`] — the same call the web
//! shell and the replay viewer make — so a link copied out of a build reproduces
//! *that* run, not the sim preset's variation on it. The sim's own `RunConfig` knobs
//! (the guard count, the alert ladder) are deliberately not in play: they are not in
//! the token, so a link cannot have been played under them.
//!
//! **It withholds nothing.** The per-turn line uses the game's own words where the
//! core has them ([`message_for`]), because inventing a second vocabulary for the
//! same events is how two descriptions of one game drift apart. But those words are
//! the *near line's*, and the near line is a player-facing filter — it deliberately
//! stays silent about a door a guard opened across the facility (§11.7). An
//! inspector must not inherit a filter designed to withhold, so an event the near
//! line has no words for is still reported, in plain factual form.

use intrusion_core::{
    message_for, start_level, Cell, Event, GenError, Input, LevelSeed, Outcome, State,
};

/// One replayed input and what it did: where the player was, where it left them, and
/// everything the world said in between.
#[derive(Clone, Debug)]
pub struct TurnRecord {
    /// 1-based position in the pasted stream — what to count to when scrubbing the
    /// same run in the browser's viewer.
    pub index: usize,
    pub input: Input,
    pub from: Cell,
    pub to: Cell,
    /// Every [`Event`] the step emitted, in order and unfiltered.
    pub events: Vec<Event>,
    /// The outcome *after* this input — so the turn a run ends on is visible as the
    /// turn it ends on, rather than only in the footer.
    pub outcome: Outcome,
}

impl TurnRecord {
    /// Whether the player ended this input on the cell they started it on. A step
    /// that did not move is the tell for a wall bump, a refused exit, or a blocked
    /// door — the free actions (§4.4) that a bare position list would hide.
    pub fn stayed_put(&self) -> bool {
        self.from == self.to
    }
}

/// A whole inspected replay: the level it was played on, every input replayed, and
/// the state it finished in.
pub struct Inspection {
    pub level: LevelSeed,
    /// Where the player started — always facing north (§12.4's fixed boot).
    pub start: Cell,
    pub turns: Vec<TurnRecord>,
    /// The finished world, kept whole so a caller can render it, read the alert off
    /// it, or ask it anything else it wants.
    pub state: State,
}

impl Inspection {
    pub fn outcome(&self) -> Outcome {
        self.state.outcome()
    }
}

/// Replay `inputs` on `level` and record what each one did (§12.4).
///
/// It stops when the inputs stop — there is no cap and no padding, because the
/// stream *is* the run. A run that ends early (a capture partway through a pasted
/// stream) keeps replaying the remainder: [`State::step`] goes inert on a finished
/// run, so the tail records as the nothing it is, which is exactly what a stream
/// captured past its own end should show.
pub fn inspect(level: &LevelSeed, inputs: &[Input]) -> Result<Inspection, GenError> {
    // The web shell's boot, not the sim's (see the module header).
    let mut state = start_level(level)?;
    let start = state.player();
    let mut turns = Vec::with_capacity(inputs.len());
    for (index, &input) in inputs.iter().enumerate() {
        let from = state.player();
        let events = state.step(input).to_vec();
        turns.push(TurnRecord {
            index: index + 1,
            input,
            from,
            to: state.player(),
            events,
            outcome: state.outcome(),
        });
    }
    Ok(Inspection {
        level: *level,
        start,
        turns,
        state,
    })
}

/// What one event says, in the game's own words where it has them (§11.7's
/// [`message_for`]) and plainly where it does not.
///
/// [`Event::Moved`] answers `None`: the line already carries `from → to`, so
/// narrating it as well would say the same thing twice — the one omission here, and
/// it omits nothing, since the movement is the columns.
fn describe(event: Event) -> Option<String> {
    if matches!(event, Event::Moved { .. }) {
        return None;
    }
    Some(match message_for(event) {
        Some(message) => message.text,
        // No near-line words for it: the player was not meant to be told, but the
        // reader of an inspection is. The debug form carries the cells, which is
        // what a forensic reader is here for.
        None => tidy_cells(&format!("{event:?}")),
    })
}

/// Rewrite `Cell { x: 29, y: 22 }` to `(29,22)` inside a debug string.
///
/// The fallback above is deliberately the *derived* debug form, so an event this
/// module has never heard of still reports itself rather than vanishing. What makes
/// that readable is one uniform tidy-up rather than a table of per-event phrasings:
/// a table would cover the events someone thought of on the day and rot around the
/// ones added after, while this improves every event there is and every event there
/// will be. Cells are the only noise worth the trouble — they appear in almost
/// every event and spell one coordinate across sixteen characters.
fn tidy_cells(debug: &str) -> String {
    const OPEN: &str = "Cell { x: ";
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug;
    while let Some(at) = rest.find(OPEN) {
        let (before, from_cell) = rest.split_at(at);
        let inner = &from_cell[OPEN.len()..];
        // `x, y` then the closing brace; anything that does not match that shape is
        // left exactly as it was rather than half-rewritten.
        let Some((x, after_x)) = inner.split_once(", y: ") else {
            break;
        };
        let Some((y, after)) = after_x.split_once(" }") else {
            break;
        };
        out.push_str(before);
        out.push_str(&format!("({x},{y})"));
        rest = after;
    }
    out.push_str(rest);
    out
}

/// A cell as `(x, y)`, columns aligned so a trajectory reads as one.
fn cell(at: Cell) -> String {
    format!("({:2},{:2})", at.x, at.y)
}

impl Inspection {
    /// The whole inspection as the text the `--inspect` mode prints: what the run was
    /// played on, every input and what it did, and the frame it ended on.
    ///
    /// Human-readable rather than JSON, unlike every other sim mode, because this one
    /// answers a question a person asked. The machine-readable pair it came from is
    /// the link itself.
    pub fn report(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.header());
        out.push('\n');
        out.push_str(&self.trajectory());
        out.push('\n');
        out.push_str(&self.footer());
        out
    }

    /// What the run was played on: the level's token and seed, a link that opens it,
    /// the modifiers bending it, the tech it held, and where the player opened.
    ///
    /// The **link** is there because this narration is prose for a person, and the
    /// obvious next thing they want after reading what somebody did is to go and try
    /// it themselves (§13.1/#572). A bare token would leave them building the URL by
    /// hand; the token stays beside it because that is what fits in a sentence.
    fn header(&self) -> String {
        let token = self.level.encode().unwrap_or_else(|| "<none>".to_string());
        let play = crate::play_link(&self.level).unwrap_or_else(|| "<none>".to_string());
        let modifiers = self.state.modifiers().active();
        let rules = if modifiers.is_empty() {
            "none active — baseline rules".to_string()
        } else {
            modifiers
                .iter()
                .map(|m| match m.detail {
                    Some(detail) => format!("{}: {detail}", m.name),
                    None => m.name.to_string(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let tech: Vec<&str> = self.state.loadout().iter().map(|id| id.name()).collect();
        format!(
            "level  {token}  (seed {})\n\
             play   {play}\n\
             rules  {rules}\n\
             tech   {}\n\
             start  {} facing north, {} input(s) to replay\n",
            self.level.seed,
            if tech.is_empty() {
                "none".to_string()
            } else {
                tech.join(", ")
            },
            cell(self.start),
            self.turns.len(),
        )
    }

    /// One line per input: its number, what was pressed, the move it made (or did
    /// not), and everything the world said.
    fn trajectory(&self) -> String {
        let mut out = String::new();
        for turn in &self.turns {
            let motion = if turn.stayed_put() {
                format!("{} —  stayed", cell(turn.from))
            } else {
                format!("{} → {}", cell(turn.from), cell(turn.to))
            };
            let said: Vec<String> = turn.events.iter().copied().filter_map(describe).collect();
            out.push_str(&format!(
                "{:3}. {:<3} {motion}",
                turn.index,
                intrusion_core::input_token(turn.input),
            ));
            if !said.is_empty() {
                out.push_str(&format!("   {}", said.join("; ")));
            }
            out.push('\n');
        }
        out
    }

    /// How the run ended, in a word.
    ///
    /// [`Outcome`] is three-way (§4.5) and both ways of losing collapse into
    /// `Lost` — but a reader wants to know *which*, and the recorded events already
    /// say: a run walked into by a guard and one sealed inside a wall are the same
    /// verdict and very different stories. Read from the stream rather than
    /// re-derived, so it cannot disagree with the trajectory above it.
    fn ending(&self) -> &'static str {
        let ended_with = |wanted: fn(&Event) -> bool| {
            self.turns.iter().any(|turn| turn.events.iter().any(wanted))
        };
        match self.outcome() {
            Outcome::Playing => "still playing",
            Outcome::Won => "won — out with the intel",
            Outcome::Lost if ended_with(|e| matches!(e, Event::Entombed { .. })) => {
                "lost — entombed"
            }
            Outcome::Lost if ended_with(|e| matches!(e, Event::Captured { .. })) => {
                "lost — caught by a guard"
            }
            Outcome::Lost => "lost",
        }
    }

    /// The frame the run ended on, and how it ended.
    fn footer(&self) -> String {
        let outcome = self.ending();
        format!(
            "{}\n\n{} turn(s) played, {outcome}\n",
            intrusion_core::render(&self.state).to_text().join("\n"),
            self.state.turn(),
        )
    }
}

#[cfg(test)]
mod tests;
