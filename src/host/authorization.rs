//! Public authorization metadata. Credentials never leave the host registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceGrant {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) scopes: u32,
    pub(crate) roots: Vec<String>,
    pub(crate) epoch: u64,
    pub(crate) authorization_revision: u64,
    pub(crate) revoked: bool,
}
impl DeviceGrant {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn scopes(&self) -> u32 {
        self.scopes
    }
    pub fn roots(&self) -> &[String] {
        &self.roots
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn authorization_revision(&self) -> u64 {
        self.authorization_revision
    }
    pub fn revoked(&self) -> bool {
        self.revoked
    }
}
