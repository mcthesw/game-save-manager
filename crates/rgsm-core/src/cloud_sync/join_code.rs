//! Portable credentials for an existing library, not a configuration backup.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{Backend, CloudSettings};

const PREFIX: &str = "RGSM1:";
const MAX_CODE_BYTES: usize = 65_536;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CloudJoinCode {
    version: u32,
    pub library_id: String,
    pub root_path: String,
    pub backend: Backend,
}

impl std::fmt::Debug for CloudJoinCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CloudJoinCode([redacted])")
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum JoinCodeError {
    #[error("{}", rust_i18n::t!("cloud_join.errors.invalid"))]
    Invalid,
    #[error("{}", rust_i18n::t!("cloud_join.errors.version"))]
    UnsupportedVersion,
    #[error("{}", rust_i18n::t!("cloud_join.errors.backend"))]
    UnsupportedBackend,
    #[error("{}", rust_i18n::t!("cloud_join.errors.size"))]
    TooLarge,
}

impl CloudJoinCode {
    pub fn new(settings: &CloudSettings, library_id: &str) -> Result<Self, JoinCodeError> {
        let code = Self {
            version: 1,
            library_id: library_id.into(),
            root_path: settings.root_path.clone(),
            backend: settings.backend.clone(),
        };
        code.validate()?;
        Ok(code)
    }

    pub fn decode(text: &str) -> Result<Self, JoinCodeError> {
        if text.len() > MAX_CODE_BYTES {
            return Err(JoinCodeError::TooLarge);
        }
        let encoded = text
            .trim()
            .strip_prefix(PREFIX)
            .ok_or(JoinCodeError::Invalid)?;
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| JoinCodeError::Invalid)?;
        // Never propagate serde/base64 diagnostics containing input or credentials.
        let code: Self = serde_json::from_slice(&bytes).map_err(|_| JoinCodeError::Invalid)?;
        code.validate()?;
        Ok(code)
    }

    pub fn encode(&self) -> Result<String, JoinCodeError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| JoinCodeError::Invalid)?;
        let text = format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes));
        if text.len() > MAX_CODE_BYTES {
            return Err(JoinCodeError::TooLarge);
        }
        Ok(text)
    }

    pub fn settings(&self, local: &CloudSettings) -> CloudSettings {
        CloudSettings {
            root_path: self.root_path.clone(),
            backend: self.backend.clone(),
            ..local.clone()
        }
    }

    pub fn endpoint(&self) -> Result<&str, JoinCodeError> {
        match &self.backend {
            Backend::WebDAV { endpoint, .. } | Backend::S3 { endpoint, .. } => Ok(endpoint),
            _ => Err(JoinCodeError::UnsupportedBackend),
        }
    }

    fn validate(&self) -> Result<(), JoinCodeError> {
        if self.version != 1 {
            return Err(JoinCodeError::UnsupportedVersion);
        }
        uuid::Uuid::parse_str(&self.library_id).map_err(|_| JoinCodeError::Invalid)?;
        let url = reqwest::Url::parse(self.endpoint()?).map_err(|_| JoinCodeError::Invalid)?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(JoinCodeError::Invalid);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_sync::S3AddressingStyle;
    const LIBRARY: &str = "11111111-1111-4111-8111-111111111111";
    #[test]
    fn retains_all_s3_addressing_styles() {
        for addressing_style in [
            S3AddressingStyle::PathStyle,
            S3AddressingStyle::VirtualHostedStyle,
            S3AddressingStyle::Auto,
        ] {
            let settings = CloudSettings {
                backend: Backend::S3 {
                    endpoint: "https://s3.example.org".into(),
                    bucket: "saves".into(),
                    region: "auto".into(),
                    access_key_id: "key".into(),
                    secret_access_key: "secret".into(),
                    addressing_style,
                },
                ..Default::default()
            };
            let text = CloudJoinCode::new(&settings, LIBRARY)
                .unwrap()
                .encode()
                .unwrap();
            assert_eq!(
                CloudJoinCode::decode(&text).unwrap().backend,
                settings.backend
            );
        }
    }
    #[test]
    fn preview_endpoints_cannot_embed_credentials_or_non_http_schemes() {
        for endpoint in [
            "https://user:secret@example.org",
            "https://example.org?password=secret",
            "https://example.org/#secret",
            "file:///secret",
            "not-a-url",
        ] {
            let settings = CloudSettings {
                backend: Backend::WebDAV {
                    endpoint: endpoint.into(),
                    username: "u".into(),
                    password: "p".into(),
                },
                ..Default::default()
            };
            assert_eq!(
                CloudJoinCode::new(&settings, LIBRARY).unwrap_err(),
                JoinCodeError::Invalid
            );
        }
    }
    #[test]
    fn round_trip_credentials_without_copying_local_transfer_preferences() {
        for backend in [
            Backend::WebDAV {
                endpoint: "https://example.org/dav".into(),
                username: "用户".into(),
                password: "secret\n\"".into(),
            },
            Backend::S3 {
                endpoint: "https://s3.example.org".into(),
                bucket: "saves".into(),
                region: "auto".into(),
                access_key_id: "key".into(),
                secret_access_key: "secret".into(),
                addressing_style: S3AddressingStyle::PathStyle,
            },
        ] {
            let source = CloudSettings {
                backend: backend.clone(),
                root_path: "/games/游戏".into(),
                auto_sync_interval: 99,
                max_concurrency: 8,
            };
            let encoded = CloudJoinCode::new(&source, LIBRARY)
                .unwrap()
                .encode()
                .unwrap();
            let decoded = CloudJoinCode::decode(&encoded).unwrap();
            let local = CloudSettings {
                auto_sync_interval: 3,
                max_concurrency: 1,
                ..Default::default()
            };
            let recovered = decoded.settings(&local);
            assert_eq!(recovered.backend, backend);
            assert_eq!(recovered.root_path, source.root_path);
            assert_eq!(recovered.auto_sync_interval, 3);
            assert_eq!(recovered.max_concurrency, 1);
            assert!(!format!("{decoded:?}").contains("secret"));
        }
    }
    #[test]
    fn rejects_damaged_large_and_future_codes_without_echoing_input() {
        for text in [
            "secret".into(),
            "RGSM1:not!base64-secret".into(),
            "x".repeat(MAX_CODE_BYTES + 1),
        ] {
            assert!(
                !CloudJoinCode::decode(&text)
                    .unwrap_err()
                    .to_string()
                    .contains("secret")
            );
        }
        let settings = CloudSettings {
            backend: Backend::WebDAV {
                endpoint: "https://example.org".into(),
                username: "u".into(),
                password: "secret".into(),
            },
            ..Default::default()
        };
        let mut code = CloudJoinCode::new(&settings, LIBRARY).unwrap();
        code.version = 2;
        let text = format!(
            "{PREFIX}{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&code).unwrap())
        );
        assert_eq!(
            CloudJoinCode::decode(&text).unwrap_err(),
            JoinCodeError::UnsupportedVersion
        );
        for backend in [Backend::Disabled, Backend::Fs] {
            assert_eq!(
                CloudJoinCode::new(
                    &CloudSettings {
                        backend,
                        ..Default::default()
                    },
                    LIBRARY
                )
                .unwrap_err(),
                JoinCodeError::UnsupportedBackend
            );
        }
    }
}
