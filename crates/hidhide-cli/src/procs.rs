//! A process is identified by pid + creation time, so a recycled pid is not mistaken for the owner.

use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, STILL_ACTIVE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, GetProcessTimes, OpenProcess, WaitForSingleObject, INFINITE,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner {
    pub pid: u32,
    pub start: u64,
}

pub fn current() -> Owner {
    let start = unsafe { creation_time(GetCurrentProcess()) }.unwrap_or(0);
    Owner { pid: std::process::id(), start }
}

pub fn is_alive(owner: Owner) -> bool {
    let Ok(h) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, owner.pid) }) else {
        return false;
    };
    let alive = unsafe { running(h) && creation_time(h) == Some(owner.start) };
    let _ = unsafe { CloseHandle(h) };
    alive
}

/// Blocks until the owner exits. Returns false if it was already gone.
pub fn wait_exit(owner: Owner) -> bool {
    let access = PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION;
    let Ok(h) = (unsafe { OpenProcess(access, false, owner.pid) }) else {
        return false;
    };
    let same = unsafe { running(h) && creation_time(h) == Some(owner.start) };
    if same {
        let rc = unsafe { WaitForSingleObject(h, INFINITE) };
        debug_assert_eq!(rc, WAIT_OBJECT_0);
    }
    let _ = unsafe { CloseHandle(h) };
    same
}

unsafe fn running(h: HANDLE) -> bool {
    let mut code = 0u32;
    GetExitCodeProcess(h, &mut code).is_ok() && code == STILL_ACTIVE.0 as u32
}

unsafe fn creation_time(h: HANDLE) -> Option<u64> {
    let (mut c, mut e, mut k, mut u) = Default::default();
    GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u).ok()?;
    Some(filetime(c))
}

fn filetime(t: FILETIME) -> u64 {
    (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
}
