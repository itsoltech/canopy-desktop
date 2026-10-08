//! Headless agent chat: provider protocols, a provider-neutral transcript and the
//! subprocess that carries them. No GPUI here; a view owns a `ChatSession`, feeds it
//! `process` events and writes back the lines it returns.
pub mod claude;
pub mod config;
pub mod launch;
pub mod mentions;
pub mod presentation;
pub mod process;
pub mod scratch;
pub mod session;
pub mod transcript;
