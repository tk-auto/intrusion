//! Turn-loop tests, split by the concern they pin (§4.2).
//!
//! One module per subsystem, mirroring the split of the code they exercise:
//! [`turn`] for the loop itself, [`guards`] for phase 3, [`doors`] for
//! [`super::doors`], [`view`] for [`super::view`]'s read surface, [`abilities`] for
//! [`super::abilities`], [`activation`] for [`super::activation`]'s precondition
//! ladder, [`effects`] for [`super::effects`]'s marks and areas, [`alert`] for the
//! §7.3 facility alert ladder and [`reinforcements`] for the
//! guards its top two rungs send in, [`bore`] for [`super::bore`], [`lockdown`] for
//! [`super::lockdown`], [`traversal`] for
//! [`super::traversal`], [`ducts`] for
//! the §10.7 crawlspace, [`tunnel`] for the player's own way in and out (§4.5/#466),
//! [`comms`] for the §7.7 comms console that kills the radio net,
//! [`exchange`] for the §8.3 crate trade a full run is offered (#266), and
//! [`watched_consoles`] for the §12.6 modifier that patrols the objectives. They share [`crate::test_support`]'s builders rather than a common
//! harness here, so each file stands alone.

mod abilities;
mod activation;
mod alert;
mod bore;
mod cache;
mod comms;
mod doors;
mod ducts;
mod effects;
mod exchange;
mod guards;
mod lockdown;
mod reinforcements;
mod traversal;
mod tunnel;
mod turn;
mod view;
mod watched_consoles;
