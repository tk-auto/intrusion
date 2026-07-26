//! Turn-loop tests, split by the concern they pin (§4.2).
//!
//! One module per subsystem, mirroring the split of the code they exercise:
//! [`turn`] for the loop itself, [`guards`] for phase 3, [`doors`] for
//! [`super::doors`], [`view`] for [`super::view`]'s read surface, [`abilities`] for
//! [`super::abilities`], [`traversal`] for [`super::traversal`], and [`ducts`] for
//! the §10.7 crawlspace. They share [`crate::test_support`]'s builders rather than a
//! common harness here, so each file stands alone.

mod abilities;
mod doors;
mod ducts;
mod guards;
mod traversal;
mod turn;
mod view;
