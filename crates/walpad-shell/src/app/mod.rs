//! Shell composition root: event loop, navigation, and global application state.

pub mod nav;
pub mod shell;
pub mod state;

#[cfg(test)]
pub(crate) use state::tests;
pub use state::*;
