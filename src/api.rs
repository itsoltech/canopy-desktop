//! Typed command adapters. Admission and durable results belong to CommandBus.
mod catalog;
pub mod catalog_projection;
pub mod compat;
pub use catalog::CatalogPublisher;
pub mod decode;
pub mod projection;
pub(crate) mod rpc;
pub use decode::project as checked_project_ref;
pub use rpc::{ThreadPublisher, ThreadRpc};

pub mod proto {
    tonic::include_proto!("canopy.api.v1");
    pub const DESCRIPTOR: &[u8] = tonic::include_file_descriptor_set!("canopy-api-v1");
}

#[cfg(test)]
mod contract_tests;
