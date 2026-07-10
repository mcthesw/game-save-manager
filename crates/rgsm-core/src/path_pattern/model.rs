use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "camelCase")]
pub enum PlatformKind {
    Windows,
    Linux,
    MacOs,
}

impl PlatformKind {
    pub const fn host() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "camelCase")]
pub enum StoreKind {
    Steam,
    Gog,
    Microsoft,
    Uplay,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PathPlaceholder {
    Root,
    Game,
    Base,
    Home,
    StoreGameId,
    StoreUserId,
    OsUserName,
    WinAppData,
    WinLocalAppData,
    WinLocalAppDataLow,
    WinDocuments,
    WinPublic,
    WinProgramData,
    WinDir,
    XdgData,
    XdgConfig,
}

impl PathPlaceholder {
    pub const ALL: [Self; 16] = [
        Self::Root,
        Self::Game,
        Self::Base,
        Self::Home,
        Self::StoreGameId,
        Self::StoreUserId,
        Self::OsUserName,
        Self::WinAppData,
        Self::WinLocalAppData,
        Self::WinLocalAppDataLow,
        Self::WinDocuments,
        Self::WinPublic,
        Self::WinProgramData,
        Self::WinDir,
        Self::XdgData,
        Self::XdgConfig,
    ];

    pub const fn token(self) -> &'static str {
        match self {
            Self::Root => "<root>",
            Self::Game => "<game>",
            Self::Base => "<base>",
            Self::Home => "<home>",
            Self::StoreGameId => "<storeGameId>",
            Self::StoreUserId => "<storeUserId>",
            Self::OsUserName => "<osUserName>",
            Self::WinAppData => "<winAppData>",
            Self::WinLocalAppData => "<winLocalAppData>",
            Self::WinLocalAppDataLow => "<winLocalAppDataLow>",
            Self::WinDocuments => "<winDocuments>",
            Self::WinPublic => "<winPublic>",
            Self::WinProgramData => "<winProgramData>",
            Self::WinDir => "<winDir>",
            Self::XdgData => "<xdgData>",
            Self::XdgConfig => "<xdgConfig>",
        }
    }

    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|placeholder| placeholder.token() == token)
            .or_else(|| (token == "<storeuserid>").then_some(Self::StoreUserId))
    }

    pub const fn windows_applicable(self) -> bool {
        !matches!(self, Self::XdgData | Self::XdgConfig)
    }

    pub fn catalog() -> Vec<PathPlaceholderDescriptor> {
        Self::ALL
            .into_iter()
            .map(|placeholder| PathPlaceholderDescriptor {
                placeholder,
                token: placeholder.token().to_string(),
                windows_applicable: placeholder.windows_applicable(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PathPlaceholderDescriptor {
    pub placeholder: PathPlaceholder,
    pub token: String,
    pub windows_applicable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct ManifestPathPattern(String);

impl ManifestPathPattern {
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn raw(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct ManifestPathConstraints {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub os: Vec<PlatformKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stores: Vec<StoreKind>,
}

impl ManifestPathConstraints {
    pub fn allows_platform(&self, platform: PlatformKind) -> bool {
        self.os.is_empty() || self.os.contains(&platform)
    }

    pub fn allows_store(&self, store: StoreKind) -> bool {
        self.stores.is_empty() || self.stores.contains(&store)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedManifestPathPattern {
    pub pattern: ManifestPathPattern,
    pub placeholders: Vec<PathPlaceholder>,
}

impl ParsedManifestPathPattern {
    pub fn contains(&self, placeholder: PathPlaceholder) -> bool {
        self.placeholders.contains(&placeholder)
    }
}
