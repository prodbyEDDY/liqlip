//! UIPI (ARCHITECTURE §6 "UIPI") — owner **W1-C**.
//!
//! `SendInput` "is subject to UIPI. Applications are permitted to inject input only into
//! applications that are at an equal or lesser integrity level", and a blocked call is not
//! reported ("neither GetLastError nor the return value will indicate the failure was caused
//! by UIPI blocking" — learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendinput).
//! So the integrity level of the target process is compared with ours up front:
//! `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` → `OpenProcessToken(TOKEN_QUERY)` →
//! `GetTokenInformation(TokenIntegrityLevel)` → the last sub-authority of the label SID
//! (low 0x1000, medium 0x2000, high 0x3000, system 0x4000 —
//! learn.microsoft.com/windows/win32/secauthz/mandatory-integrity-control,
//! learn.microsoft.com/windows/win32/api/winnt/ns-winnt-token_mandatory_label).
//! A target we cannot query counts as elevated (copy-only is the safe side).

/// `SECURITY_MANDATORY_MEDIUM_RID` — assumed for ourselves if our own token can't be read.
pub const MEDIUM_RID: u32 = 0x2000;

/// The decision: input into the target would be blocked (or we can't tell).
pub fn is_elevated(target_rid: Option<u32>, own_rid: Option<u32>) -> bool {
    match target_rid {
        None => true,
        Some(t) => t > own_rid.unwrap_or(MEDIUM_RID),
    }
}

/// Is the process `pid` at a higher integrity level than Liqlip (input would be blocked)?
#[cfg(windows)]
pub fn target_is_elevated(pid: u32) -> bool {
    is_elevated(win::process_rid(pid), win::own_rid())
}

#[cfg(not(windows))]
pub fn target_is_elevated(_pid: u32) -> bool {
    true
}

#[cfg(windows)]
mod win {
    use std::sync::OnceLock;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel, TOKEN_MANDATORY_LABEL,
        TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: a handle we opened and own.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    /// Integrity RID of an open process handle's primary token.
    fn token_rid(process: HANDLE) -> Option<u32> {
        // SAFETY: documented two-call pattern for GetTokenInformation (size query, then a
        // buffer of that size); the label SID points into our buffer, which outlives its use.
        // learn.microsoft.com/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
            let token = Handle(token);
            let mut len = 0u32;
            let _ = GetTokenInformation(token.0, TokenIntegrityLevel, None, 0, &mut len);
            if len == 0 {
                return None;
            }
            // u64 storage keeps the buffer aligned for TOKEN_MANDATORY_LABEL.
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            GetTokenInformation(token.0, TokenIntegrityLevel, Some(buf.as_mut_ptr().cast()), len, &mut len).ok()?;
            let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
            let sid = label.Label.Sid;
            if sid.is_invalid() {
                return None;
            }
            let count = *GetSidSubAuthorityCount(sid);
            if count == 0 {
                return None;
            }
            Some(*GetSidSubAuthority(sid, (count - 1) as u32))
        }
    }

    pub fn process_rid(pid: u32) -> Option<u32> {
        // SAFETY: OpenProcess with the least right that allows OpenProcessToken on a process
        // of the same user; the handle is closed by `Handle`.
        unsafe {
            let process = Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
            token_rid(process.0)
        }
    }

    pub fn own_rid() -> Option<u32> {
        static OWN: OnceLock<Option<u32>> = OnceLock::new();
        // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing.
        *OWN.get_or_init(|| token_rid(unsafe { GetCurrentProcess() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOW: u32 = 0x1000;
    const HIGH: u32 = 0x3000;
    const SYSTEM: u32 = 0x4000;

    #[test]
    fn higher_integrity_target_is_elevated() {
        assert!(is_elevated(Some(HIGH), Some(MEDIUM_RID)), "admin PowerShell from a normal Liqlip");
        assert!(is_elevated(Some(SYSTEM), Some(MEDIUM_RID)));
        assert!(is_elevated(Some(SYSTEM), Some(HIGH)));
    }

    #[test]
    fn equal_or_lower_integrity_is_pasteable() {
        assert!(!is_elevated(Some(MEDIUM_RID), Some(MEDIUM_RID)));
        assert!(!is_elevated(Some(LOW), Some(MEDIUM_RID)), "UWP / low-IL browser sandboxes");
        assert!(!is_elevated(Some(HIGH), Some(HIGH)), "Liqlip itself run as administrator");
        assert!(!is_elevated(Some(MEDIUM_RID), Some(HIGH)));
    }

    #[test]
    fn unknown_target_is_treated_as_elevated() {
        assert!(is_elevated(None, Some(MEDIUM_RID)));
        assert!(is_elevated(None, None));
        // Our own level unreadable → assume medium.
        assert!(!is_elevated(Some(MEDIUM_RID), None));
        assert!(is_elevated(Some(HIGH), None));
    }

    #[cfg(windows)]
    #[test]
    fn own_process_is_not_elevated_relative_to_itself() {
        // Reads our own token only — no other process, no system change.
        assert!(!target_is_elevated(std::process::id()));
    }
}
