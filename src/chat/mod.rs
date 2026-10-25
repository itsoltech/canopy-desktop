//! Headless agent chat: provider protocols, a provider-neutral transcript and the
//! subprocess that carries them. No GPUI here; the application runtime owns a
//! session, feeds it process events and writes back the lines it returns.
pub mod acp;
pub mod activity;
pub mod agent;
pub mod attachments;
pub mod backend;
pub mod backend_session;
pub mod claude;
pub mod closed;
pub mod codex;
pub mod compose;
pub mod config;
pub mod fences;
pub mod fork;
pub mod goal;
pub mod history;
pub mod launch;
pub mod mcp;
pub mod mentions;
pub mod models;
pub mod page;
pub mod pages;
pub mod presentation;
pub mod process;
pub mod provider;
pub mod quotes;
pub mod scratch;
pub mod session;
pub mod shell;
pub mod tools;
pub mod transcript;
