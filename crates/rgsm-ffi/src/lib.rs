//! rgsm-ffi: FFI bindings for the Game Save Manager core library.
//!
//! This crate will expose a C-compatible API for use from other languages.

use std::ffi::{CString, c_char};
use std::sync::OnceLock;

/// Returns the version of the core library.
#[unsafe(no_mangle)]
pub extern "C" fn rgsm_version() -> *const c_char {
    static VERSION: OnceLock<CString> = OnceLock::new();
    VERSION
        .get_or_init(|| CString::new(env!("CARGO_PKG_VERSION")).expect("valid package version"))
        .as_ptr()
}
