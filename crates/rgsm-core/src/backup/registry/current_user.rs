#[cfg(any(windows, test))]
use super::normalize_registry_path;

/// Offer a portable root only when the explicit SID belongs to this process.
/// Never change another user's path or silently rewrite the saved value.
#[cfg(windows)]
pub fn suggest_current_user_path(path: &str) -> Option<String> {
    let normalized = normalize_registry_path(path);
    if !normalized.to_ascii_uppercase().starts_with("HKEY_USERS\\") {
        return None;
    }
    portable_path(&normalized, &current_sid().ok()?)
}

#[cfg(any(windows, test))]
fn portable_path(path: &str, current_sid: &str) -> Option<String> {
    let normalized = normalize_registry_path(path);
    let mut parts = normalized.splitn(3, '\\');
    if !parts.next()?.eq_ignore_ascii_case("HKEY_USERS")
        || !parts.next()?.eq_ignore_ascii_case(current_sid)
    {
        return None;
    }
    let subkey = parts.next()?.trim_start_matches('\\');
    (!subkey.is_empty()).then(|| format!("HKEY_CURRENT_USER\\{subkey}"))
}

#[cfg(windows)]
fn current_sid() -> std::io::Result<String> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER,
            TokenUser,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    // SAFETY: query only this process's token. OwnedHandle closes it on every
    // return path; the aligned buffer remains alive while its SID is read.
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let token = OwnedHandle::from_raw_handle(token);
        let mut length = 0;
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut length,
        );
        if length == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        if GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        ) == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut text = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut count = 0;
        while *text.add(count) != 0 {
            count += 1;
        }
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, count));
        LocalFree(text.cast());
        Ok(sid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn suggestion_matches_the_entire_sid_and_preserves_subkeys() {
        assert_eq!(
            portable_path(
                "REGISTRY:HKEY_USERS/S-1-5-21-123/SOFTWARE/Game",
                "S-1-5-21-123"
            ),
            Some("HKEY_CURRENT_USER\\SOFTWARE\\Game".into())
        );
        for path in [
            "HKEY_USERS/S-1-5-21-1234/SOFTWARE/Game",
            "HKEY_USERS/S-1-5-21-other/SOFTWARE/Game",
            "HKEY_USERS/S-1-5-21-123_Classes/Game",
            "HKEY_CURRENT_USER/SOFTWARE/Game",
        ] {
            assert_eq!(portable_path(path, "S-1-5-21-123"), None);
        }
    }
    #[cfg(windows)]
    #[test]
    fn process_sid_can_offer_a_current_user_path_without_registry_writes() {
        let sid = current_sid().unwrap();
        assert_eq!(
            suggest_current_user_path(&format!("HKEY_USERS\\{sid}\\Software\\Example")),
            Some("HKEY_CURRENT_USER\\Software\\Example".into())
        );
    }
}
