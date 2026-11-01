//! Native remote transport support. Feature handlers remain the authoritative
//! owners of application state; streams carry their committed changes.
pub mod artifact_rpc;
pub mod auth;
pub mod client;
pub mod enrollment;
pub mod host;
pub mod metrics;
pub mod network;
pub mod peer;
pub mod resources;
pub mod server;
pub mod settings_rpc;
pub mod sync;
pub mod tasks;
pub mod terminal_rpc;
pub mod tls;
pub mod transcript;

pub mod composer;
pub mod discovery;
