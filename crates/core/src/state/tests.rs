//! Turn-loop tests, split by the concern they pin (§4.2).
//!
//! One module per subsystem, mirroring the split of the code they exercise:
//! [`turn`] for the loop itself, [`guards`] for phase 3, [`doors`] for
//! [`super::doors`], [`view`] for [`super::view`]'s read surface, [`abilities`] for
//! [`super::abilities`], [`activation`] for [`super::activation`]'s precondition
//! ladder, [`effects`] for [`super::effects`]'s marks and areas, [`alert`] for the
//! §7.3 facility alert ladder and [`reinforcements`] for the
//! guards its top two rungs send in, [`bore`] for [`super::bore`], [`lockdown`] for
//! [`super::lockdown`]'s seal and [`keys`] for its second lock source — the locked
//! prize room and the key a takedown buys (§10.4/#236) —
//! [`traversal`] for
//! [`super::traversal`], [`ducts`] for
//! the §10.7 crawlspace, [`tunnel`] for the player's own way in and out (§4.5/#466),
//! [`comms`] for the §7.7 comms console that kills the radio net,
//! [`control`] for the §8.1 control-transfer seam and the drone that first uses it
//! (#273),
//! [`exchange`] for the §8.3 crate trade a full run is offered (#266),
//! [`saver`] for §4.5's one declared exception (#243),
//! [`watched_consoles`] for the §12.6 modifier that patrols the objectives,
//! [`narrowed_cones`] for the §12.6 modifier that shortens and thins every guard's
//! cone (#495), and [`ghost`] for §12.6's one rule-bending *debug* switch (#507).
//! They share [`crate::test_support`]'s builders rather than a common
//! harness here, so each file stands alone.

mod abilities;
mod activation;
mod alert;
mod bore;
mod cache;
mod comms;
mod control;
mod cover;
mod dart;
mod doors;
mod ducts;
mod effects;
mod exchange;
mod false_call;
mod ghost;
mod guards;
mod guide;
mod keys;
mod lockdown;
mod minimum_haul;
mod narrowed_cones;
mod reinforcements;
mod repel;
mod saver;
mod sense_suppressed;
mod traversal;
mod tunnel;
mod turn;
mod view;
mod watched_consoles;
