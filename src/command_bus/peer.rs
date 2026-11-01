//! Stable reciprocal trust intent; public certificates are not vault credentials.
use super::{CommandMetadata, Risk};
use crate::{
    host,
    settings::{Error, Result},
    thread::HostId,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(super) static REGISTRY: &[CommandMetadata] = &[CommandMetadata::new(
    "peer.connect",
    Risk::Configuration,
    host::ADMIN,
)];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerConnection {
    pub host: HostId,
    pub name: String,
    pub server_name: String,
    pub certificate: Vec<u8>,
    pub fingerprint: Vec<u8>,
    pub addresses: Vec<String>,
    pub port: u16,
    pub return_tunnel: bool,
    pub device_certificate: Vec<u8>,
    pub certificate_chain: Vec<Vec<u8>>,
}
impl PeerConnection {
    pub fn metadata(&self) -> &'static CommandMetadata {
        &REGISTRY[0]
    }
    pub(super) fn normalize(&mut self) -> Result<()> {
        if self.name.trim().is_empty()
            || self.name.len() > 256
            || self.name.chars().any(char::is_control)
            || self.server_name != format!("canopy-{}.local", self.host)
            || self.port == 0
            || self.addresses.len() > 8
            || self.certificate.len() > 16384
            || self.device_certificate.len() > 16384
            || self.certificate_chain.is_empty()
            || self.certificate_chain.len() > 8
            || self.certificate_chain.iter().map(Vec::len).sum::<usize>() > 16384
            || self.fingerprint != Sha256::digest(&self.certificate).as_slice()
        {
            return Err(Error::InvalidRuntimeState);
        }
        for address in &mut self.addresses {
            let ip: std::net::IpAddr = address.parse().map_err(|_| Error::InvalidRuntimeState)?;
            if (ip.is_loopback() && !self.return_tunnel) || ip.is_unspecified() || ip.is_multicast()
            {
                return Err(Error::InvalidRuntimeState);
            }
            *address = ip.to_string();
        }
        Ok(())
    }
}
