pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Whether the context-menu integration is installed, and at what version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextMenuInstallState {
    NotInstalled,
    Installed { version: Option<String> },
}

impl ContextMenuInstallState {
    pub fn is_current_version(&self) -> bool {
        matches!(
            self,
            Self::Installed { version: Some(version) } if version == CURRENT_VERSION
        )
    }
}
