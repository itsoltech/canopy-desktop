//! Native remote transport support. Feature handlers remain the authoritative
//! owners of application state; streams carry their committed changes.
pub mod artifact_rpc;
pub mod auth;
pub mod client;
pub mod enrollment;
pub mod host;
pub mod metrics;
pub mod network;
pub mod resources;
pub mod server;
pub mod settings_rpc;
pub mod sync;
pub mod tasks;
pub mod terminal_rpc;
pub mod tls;
pub mod transcript;

/// Generated native RPC DTOs. Domain owners validate before accepting commands.
#[allow(
    clippy::large_enum_variant,
    reason = "generated protocol oneofs follow the wire schema; domain state uses its own types"
)]
pub mod proto {
    tonic::include_proto!("canopy.remote.v1");
    pub const DESCRIPTOR: &[u8] = tonic::include_file_descriptor_set!("canopy-remote-v1");
}
pub mod composer;
pub mod discovery;
