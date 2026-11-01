use std::error::Error;

use opendal::ErrorKind;
use rust_i18n::t;

/// Player-facing text for a failed cloud operation.
///
/// Storage errors embed the request URL and are English-only, so they become a
/// localized message with a next step and the raw error is only logged. Other
/// errors keep their own message.
pub fn cloud_error_message(error: &(dyn Error + 'static)) -> String {
    let Some(key) = storage_failure(error) else {
        return error.to_string();
    };
    log::warn!("Cloud storage request failed: {error:?}");
    t!(key).to_string()
}

fn storage_failure(error: &(dyn Error + 'static)) -> Option<&'static str> {
    let mut current = Some(error);
    while let Some(error) = current {
        let storage = error.downcast_ref::<opendal::Error>().or_else(|| {
            error
                .downcast_ref::<Box<opendal::Error>>()
                .map(|boxed| &**boxed)
        });
        if let Some(error) = storage {
            return Some(match error.kind() {
                ErrorKind::PermissionDenied => "error.cloud_access_denied",
                ErrorKind::ConfigInvalid => "error.cloud_settings_invalid",
                _ => "error.cloud_unreachable",
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
    use crate::services::CloudLibraryServiceError;

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
    fn other_errors_keep_their_own_message() {
        let error = CloudLibraryServiceError::ConfirmationRequired;
        assert_eq!(cloud_error_message(&error), error.to_string());
    }
}
