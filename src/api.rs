//! Typed command adapters. Admission and durable results belong to CommandBus.
mod catalog;
pub use catalog::CatalogPublisher;
pub mod decode;
pub(crate) mod git;
mod receipt;
pub(crate) mod resources;
pub use receipt::committed_receipt;
pub use receipt::project_receipt;
pub(crate) mod rpc;
pub use decode::project as checked_project_ref;
pub use rpc::{ThreadPublisher, ThreadRpc};

#[allow(
    clippy::large_enum_variant,
    reason = "generated wire oneofs; domain owners use native types"
)]
pub mod proto {
    tonic::include_proto!("canopy.api.v1");
    pub const DESCRIPTOR: &[u8] = tonic::include_file_descriptor_set!("canopy-api-v1");
}

#[cfg(test)]
mod contract_tests;
