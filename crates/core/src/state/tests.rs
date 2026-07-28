//! Turn-loop tests, split by the concern they pin (§4.2).
//!
//! One module per subsystem, mirroring the split of the code they exercise:
//! [`turn`] for the loop itself, [`guards`] for phase 3, [`doors`] for
//! [`super::doors`], [`view`] for [`super::view`]'s read surface, [`abilities`] for
//! [`super::abilities`], [`activation`] for [`super::activation`]'s precondition
//! ladder, [`alert`] for the §7.3 facility alert ladder, [`bore`] for [`super::bore`], [`lockdown`] for
//! [`super::lockdown`], [`traversal`] for
//! [`super::traversal`], [`ducts`] for
//! the §10.7 crawlspace, and [`comms`] for the §7.7 comms console that kills the
//! radio net. They share [`crate::test_support`]'s builders rather than a common
//! harness here, so each file stands alone.

mod abilities;
mod activation;
mod alert;
mod bore;
mod comms;
mod doors;
mod ducts;
mod guards;
mod lockdown;
mod traversal;
mod turn;
mod view;
