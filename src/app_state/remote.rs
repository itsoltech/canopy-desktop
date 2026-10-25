//! Application ownership of the native host and its concrete feature bridges.
mod control;
pub use control::ControlBridge;
mod catalog;
mod chat;
mod pages;
mod state;
pub use state::RemoteState;
