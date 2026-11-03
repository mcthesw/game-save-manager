use std::error::Error;

use opendal::ErrorKind;
use rust_i18n::t;
use serde::Serialize;
use specta::Type;

use super::{CloudLibraryServiceError, SnapshotSyncServiceError};
use crate::cloud_sync::v2::{
    CloudLibraryBootstrapError, CloudNamespaceError, SharedLibraryRepositoryError,
};

/// Why a cloud read failed, so one place can tell the player what to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CloudFailureKind {
    RateLimited,
    Unreachable,
    Auth,
    Path,
    /// This device was removed from the library and must be reconnected.
    ReconnectRequired,
    /// The library's own files are missing or unreadable; it must be rebuilt.
    LibraryDamaged,
    Other,
}

pub fn cloud_failure_kind(error: &(dyn Error + 'static)) -> CloudFailureKind {
    if let Some(kind) = library_state(error) {
        return kind;
    }
    let mut current = Some(error);
    while let Some(error) = current {
        let storage = error.downcast_ref::<opendal::Error>().or_else(|| {
            error
                .downcast_ref::<Box<opendal::Error>>()
                .map(|boxed| &**boxed)
        });
        if let Some(error) = storage {
            return match error.kind() {
                ErrorKind::RateLimited => CloudFailureKind::RateLimited,
                ErrorKind::Unexpected => CloudFailureKind::Unreachable,
                ErrorKind::PermissionDenied => CloudFailureKind::Auth,
                ErrorKind::ConfigInvalid | ErrorKind::NotFound => CloudFailureKind::Path,
                _ => CloudFailureKind::Other,
            };
        }
        current = error.source();
    }
    CloudFailureKind::Other
}

/// Transparent wrappers hide their inner error from `source()`, so the wrappers
/// that carry a library-state error are unwrapped explicitly.
fn library_state(error: &(dyn Error + 'static)) -> Option<CloudFailureKind> {
    if let Some(error) = error.downcast_ref::<SnapshotSyncServiceError>() {
        return match error {
            SnapshotSyncServiceError::Retention(inner) => library_state(inner),
            _ => None,
        };
    }
    if let Some(error) = error.downcast_ref::<CloudLibraryServiceError>() {
        return match error {
            CloudLibraryServiceError::DeviceReconnectRequired => {
                Some(CloudFailureKind::ReconnectRequired)
            }
            CloudLibraryServiceError::SharedLibrary(inner) => library_state(inner),
            CloudLibraryServiceError::Bootstrap(inner) => library_state(inner),
            _ => None,
        };
    }
    if let Some(CloudLibraryBootstrapError::Namespace(inner)) =
        error.downcast_ref::<CloudLibraryBootstrapError>()
    {
        return library_state(inner);
    }
    match (
        error.downcast_ref::<CloudNamespaceError>(),
        error.downcast_ref::<SharedLibraryRepositoryError>(),
    ) {
        (
            Some(
                CloudNamespaceError::MissingRequiredObject(_)
                | CloudNamespaceError::PartialV2(_)
                | CloudNamespaceError::MalformedObject { .. },
            ),
            _,
        )
        | (_, Some(SharedLibraryRepositoryError::Missing)) => {
            Some(CloudFailureKind::LibraryDamaged)
        }
        _ => None,
    }
}

/// Player-facing text for a failed cloud operation.
///
/// Storage errors embed the request URL and are English-only, so they become a
/// localized message with a next step and the raw error is only logged. Other
/// errors keep their own message.
pub fn cloud_error_message(error: &(dyn Error + 'static)) -> String {
    let Some(message) = storage_failure(error) else {
        return error.to_string();
    };
    log::warn!("Cloud storage request failed: {error:?}");
    message
}

/// Shown instead of a request when automatic reads are paused by the provider.
pub fn cloud_rate_limit_message() -> String {
    t!("error.cloud_rate_limited").to_string()
}

fn storage_failure(error: &(dyn Error + 'static)) -> Option<String> {
    let mut current = Some(error);
    while let Some(error) = current {
        let storage = error.downcast_ref::<opendal::Error>().or_else(|| {
            error
                .downcast_ref::<Box<opendal::Error>>()
                .map(|boxed| &**boxed)
        });
        if let Some(error) = storage {
            return Some(match error.kind() {
                ErrorKind::PermissionDenied => t!("error.cloud_access_denied").to_string(),
                ErrorKind::ConfigInvalid => t!("error.cloud_settings_invalid").to_string(),
                ErrorKind::RateLimited => t!("error.cloud_rate_limited").to_string(),
                ErrorKind::NotFound => t!("error.cloud_not_found").to_string(),
                ErrorKind::Unsupported => t!("error.cloud_unsupported").to_string(),
                // Network failures surface as an unspecific error; every other
                // kind is a specific refusal the player should hear about.
                ErrorKind::Unexpected => t!("error.cloud_unreachable").to_string(),
                _ => t!("error.cloud_request_failed", reason = error.message()).to_string(),
            });
        }
        current = error.source();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_sync::v2::{
        CloudLibraryBootstrapError, CloudNamespaceError, SharedLibraryRepositoryError,
    };
    use crate::preclude::BackendError;

    const URL: &str = "https://storage.example.test/bucket/private/path";

    fn storage_error(kind: ErrorKind) -> opendal::Error {
        opendal::Error::new(kind, "request failed").with_context("uri", URL)
    }

    #[test]
    fn storage_failures_become_actionable_messages_without_the_request_url() {
        let unreachable = CloudLibraryServiceError::Backend(BackendError::from(storage_error(
            ErrorKind::Unexpected,
        )));
        let message = cloud_error_message(&unreachable);
        assert!(!message.contains("https://"), "{message}");
        assert_eq!(message, t!("error.cloud_unreachable"));

        let denied =
            SharedLibraryRepositoryError::Transport(storage_error(ErrorKind::PermissionDenied));
        assert_eq!(
            cloud_error_message(&denied),
            t!("error.cloud_access_denied")
        );

        let missing = SharedLibraryRepositoryError::Transport(storage_error(ErrorKind::NotFound));
        assert_eq!(cloud_error_message(&missing), t!("error.cloud_not_found"));

        let unsupported =
            SharedLibraryRepositoryError::Transport(storage_error(ErrorKind::Unsupported));
        assert_eq!(
            cloud_error_message(&unsupported),
            t!("error.cloud_unsupported")
        );

        let refused =
            SharedLibraryRepositoryError::Transport(storage_error(ErrorKind::IsADirectory));
        let message = cloud_error_message(&refused);
        assert!(message.contains("request failed"), "{message}");
        assert!(!message.contains("https://"), "{message}");

        let limited =
            SharedLibraryRepositoryError::Transport(storage_error(ErrorKind::RateLimited));
        assert_eq!(
            cloud_error_message(&limited),
            t!("error.cloud_rate_limited")
        );
    }

    #[test]
    fn storage_errors_are_found_through_transparent_service_variants() {
        let invalid = CloudLibraryServiceError::SharedLibrary(
            SharedLibraryRepositoryError::Transport(storage_error(ErrorKind::ConfigInvalid)),
        );
        assert_eq!(
            cloud_error_message(&invalid),
            t!("error.cloud_settings_invalid")
        );

        let bootstrap = CloudLibraryServiceError::Bootstrap(CloudLibraryBootstrapError::Transport(
            storage_error(ErrorKind::Unexpected),
        ));
        assert_eq!(
            cloud_error_message(&bootstrap),
            t!("error.cloud_unreachable")
        );
    }

    #[test]
    fn library_state_errors_say_what_to_do_in_every_wrapper() {
        let stale = CloudLibraryServiceError::SharedLibrary(SharedLibraryRepositoryError::Stale);
        assert_eq!(
            cloud_error_message(&stale),
            t!("error.cloud_library_changed")
        );
        assert_eq!(
            cloud_error_message(&CloudLibraryServiceError::DeviceReconnectRequired),
            t!("error.cloud_reconnect_required")
        );
        let missing = CloudLibraryServiceError::Bootstrap(CloudLibraryBootstrapError::Namespace(
            CloudNamespaceError::MissingRequiredObject("v2/namespace.json"),
        ));
        assert_eq!(
            cloud_error_message(&missing),
            t!("error.cloud_library_damaged")
        );
    }

    #[test]
    fn failures_are_classified_through_wrappers() {
        let limited = SnapshotSyncServiceError::Backend(BackendError::from(storage_error(
            ErrorKind::RateLimited,
        )));
        assert_eq!(cloud_failure_kind(&limited), CloudFailureKind::RateLimited);
        let removed =
            SnapshotSyncServiceError::Retention(CloudLibraryServiceError::DeviceReconnectRequired);
        assert_eq!(
            cloud_failure_kind(&removed),
            CloudFailureKind::ReconnectRequired
        );
        let denied =
            SharedLibraryRepositoryError::Transport(storage_error(ErrorKind::PermissionDenied));
        assert_eq!(cloud_failure_kind(&denied), CloudFailureKind::Auth);
        let rebuilt = SnapshotSyncServiceError::Retention(CloudLibraryServiceError::SharedLibrary(
            SharedLibraryRepositoryError::Missing,
        ));
        assert_eq!(
            cloud_failure_kind(&rebuilt),
            CloudFailureKind::LibraryDamaged
        );
        assert_eq!(
            cloud_failure_kind(&CloudLibraryServiceError::ConfirmationRequired),
            CloudFailureKind::Other
        );
    }

    #[test]
    fn other_errors_keep_their_own_message() {
        let error = CloudLibraryServiceError::ConfirmationRequired;
        assert_eq!(cloud_error_message(&error), error.to_string());
    }
}
