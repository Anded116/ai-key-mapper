//! Direct IOCTL access to the HidHide filter driver (`\\.\HidHide`), without HidHideCLI.
//!
//! The control device is exclusive: while someone else holds it open (HidHide Configuration
//! Client, DS4Windows, another mapper) `CreateFile` fails with `ERROR_ACCESS_DENIED`. So every
//! call opens the device for one short transaction, closes it right away and retries with
//! backoff on `ACCESS_DENIED`. No administrator rights are needed.

mod ioctl;
mod ntpath;

use std::fmt;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows::core::{w, HRESULT};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_INVALID_FUNCTION,
    ERROR_INVALID_PARAMETER, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, GENERIC_READ, HANDLE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows::Win32::System::IO::DeviceIoControl;

pub use ioctl::{decode_multi_sz, encode_multi_sz};
pub use ntpath::dos_to_nt_path;

pub const DEVICE: &str = r"\\.\HidHide";

/// How long a transaction waits for the exclusive handle before giving up with [`Error::Busy`].
const BUSY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub enum Error {
    /// No `\\.\HidHide` device: the driver is not installed or not loaded.
    NotInstalled,
    /// The control device stayed open in another process for the whole retry window.
    Busy,
    /// The driver does not know this IOCTL (session blacklist on 1.5.x).
    Unsupported,
    Path(PathBuf, &'static str),
    Os(windows::core::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotInstalled => write!(f, "HidHide is not installed ({DEVICE} not found)"),
            Error::Busy => write!(f, "{DEVICE} is held open by another process (HidHide Client?)"),
            Error::Unsupported => write!(f, "this HidHide driver does not support the request"),
            Error::Path(p, why) => write!(f, "{}: {why}", p.display()),
            Error::Os(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    NotInstalled,
    Installed { version: Option<String>, session_api: bool },
    Busy,
}

/// Full persistent configuration of the driver.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Config {
    /// Global "cloak" flag: the blacklist is enforced only while it is on.
    pub active: bool,
    pub inverse: bool,
    /// Device instance paths, as `SetupDiGetDeviceInstanceId` returns them.
    pub blacklist: Vec<String>,
    /// NT image paths, see [`dos_to_nt_path`].
    pub whitelist: Vec<String>,
}

/// No handle is kept between calls: the device is exclusive.
#[derive(Debug, Clone, Copy, Default)]
pub struct HidHide;

impl HidHide {
    pub fn probe() -> Result<Status> {
        match Self::tx(|h| h.session_api()) {
            Ok(session_api) => Ok(Status::Installed { version: installed_version(), session_api }),
            Err(Error::NotInstalled) => Ok(Status::NotInstalled),
            Err(Error::Busy) => Ok(Status::Busy),
            Err(e) => Err(e),
        }
    }

    pub fn active(&self) -> Result<bool> {
        Self::tx(|h| h.get_bool(ioctl::GET_ACTIVE))
    }

    pub fn set_active(&self, on: bool) -> Result<()> {
        Self::tx(|h| h.set_bool(ioctl::SET_ACTIVE, on))
    }

    pub fn blacklist(&self) -> Result<Vec<String>> {
        Self::tx(|h| h.get_list(ioctl::GET_BLACKLIST))
    }

    pub fn set_blacklist(&self, paths: &[String]) -> Result<()> {
        Self::tx(|h| h.set_list(ioctl::SET_BLACKLIST, paths))
    }

    pub fn whitelist(&self) -> Result<Vec<String>> {
        Self::tx(|h| h.get_list(ioctl::GET_WHITELIST))
    }

    pub fn set_whitelist(&self, images: &[String]) -> Result<()> {
        Self::tx(|h| h.set_list(ioctl::SET_WHITELIST, images))
    }

    pub fn config(&self) -> Result<Config> {
        Self::tx(|h| h.config())
    }

    /// Read-modify-write under one exclusive handle, so no other client can interleave.
    /// Only the fields `f` changed are written back. Returns the configuration before the edit.
    pub fn edit<E: From<Error>>(&self, f: impl FnOnce(&mut Config) -> Result<(), E>) -> Result<Config, E> {
        Self::tx(|h| {
            let before = h.config()?;
            let mut after = before.clone();
            f(&mut after)?;
            if after.whitelist != before.whitelist {
                h.set_list(ioctl::SET_WHITELIST, &after.whitelist)?;
            }
            if after.blacklist != before.blacklist {
                h.set_list(ioctl::SET_BLACKLIST, &after.blacklist)?;
            }
            if after.inverse != before.inverse {
                h.set_bool(ioctl::SET_WLINVERSE, after.inverse)?;
            }
            if after.active != before.active {
                h.set_bool(ioctl::SET_ACTIVE, after.active)?;
            }
            Ok(before)
        })
    }

    /// Driver 1.7+: the entries live until the calling process exits (even on a crash).
    pub fn session_hide(&self, paths: &[String]) -> Result<()> {
        Self::tx(|h| h.set_list(ioctl::ADD_SESSION_BLACKLIST, paths)).map_err(unsupported)
    }

    /// Driver 1.7+: drops the session entries of the calling process only.
    pub fn session_clear(&self) -> Result<()> {
        Self::tx(|h| h.call(ioctl::CLR_SESSION_BLACKLIST, &[], None).map(drop)).map_err(unsupported)
    }

    fn tx<T, E: From<Error>>(f: impl FnOnce(&Handle) -> Result<T, E>) -> Result<T, E> {
        let h = Handle::open()?;
        f(&h)
    }
}

fn is(e: &windows::core::Error, code: windows::Win32::Foundation::WIN32_ERROR) -> bool {
    e.code() == HRESULT::from_win32(code.0)
}

fn unsupported(e: Error) -> Error {
    match e {
        Error::Os(ref os) if is(os, ERROR_INVALID_PARAMETER) || is(os, ERROR_INVALID_FUNCTION) => {
            Error::Unsupported
        }
        e => e,
    }
}

/// `HKLM\SOFTWARE\Nefarius Software Solutions e.U.\HidHide\Version`, written by the installer.
fn installed_version() -> Option<String> {
    let mut buf = [0u16; 64];
    let mut len = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Nefarius Software Solutions e.U.\HidHide"),
            w!("Version"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        )
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let chars = (len as usize / 2).min(buf.len());
    let end = buf[..chars].iter().position(|&c| c == 0).unwrap_or(chars);
    Some(String::from_utf16_lossy(&buf[..end]))
}

struct Handle(HANDLE);

impl Handle {
    fn open() -> Result<Self> {
        let start = Instant::now();
        let mut delay = Duration::from_millis(10);
        loop {
            let res = unsafe {
                CreateFileW(
                    w!(r"\\.\HidHide"),
                    GENERIC_READ.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(0),
                    None,
                )
            };
            match res {
                Ok(h) => return Ok(Self(h)),
                Err(e) if is(&e, ERROR_FILE_NOT_FOUND) || is(&e, ERROR_PATH_NOT_FOUND) => {
                    return Err(Error::NotInstalled)
                }
                Err(e) if is(&e, ERROR_ACCESS_DENIED) => {
                    if start.elapsed() >= BUSY_TIMEOUT {
                        return Err(Error::Busy);
                    }
                    sleep(delay);
                    delay = (delay * 2).min(Duration::from_millis(250));
                }
                Err(e) => return Err(Error::Os(e)),
            }
        }
    }

    /// Returns the byte count the driver reported.
    fn call(&self, code: u32, input: &[u8], output: Option<&mut [u8]>) -> Result<u32> {
        let mut returned = 0u32;
        let (out_ptr, out_len) = match output {
            Some(buf) => (Some(buf.as_mut_ptr().cast()), buf.len() as u32),
            None => (None, 0),
        };
        let in_ptr = (!input.is_empty()).then(|| input.as_ptr().cast());
        unsafe {
            DeviceIoControl(self.0, code, in_ptr, input.len() as u32, out_ptr, out_len, Some(&mut returned), None)
        }
        .map_err(Error::Os)?;
        Ok(returned)
    }

    fn get_bool(&self, code: u32) -> Result<bool> {
        let mut b = [0u8; 1];
        self.call(code, &[], Some(&mut b))?;
        Ok(b[0] != 0)
    }

    fn set_bool(&self, code: u32, on: bool) -> Result<()> {
        self.call(code, &[on as u8], None).map(drop)
    }

    /// Two calls: the first, without a buffer, reports the size in bytes.
    fn get_list(&self, code: u32) -> Result<Vec<String>> {
        let needed = self.call(code, &[], None)? as usize;
        if needed == 0 {
            return Ok(Vec::new());
        }
        let mut buf = vec![0u16; needed.div_ceil(2)];
        let bytes = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr().cast::<u8>(), buf.len() * 2) };
        let got = self.call(code, &[], Some(bytes))? as usize;
        buf.truncate(got / 2);
        Ok(decode_multi_sz(&buf))
    }

    fn set_list(&self, code: u32, items: &[String]) -> Result<()> {
        let wide = encode_multi_sz(items);
        let bytes: Vec<u8> = wide.iter().flat_map(|c| c.to_le_bytes()).collect();
        self.call(code, &bytes, None).map(drop)
    }

    fn config(&self) -> Result<Config> {
        Ok(Config {
            active: self.get_bool(ioctl::GET_ACTIVE)?,
            inverse: self.get_bool(ioctl::GET_WLINVERSE)?,
            blacklist: self.get_list(ioctl::GET_BLACKLIST)?,
            whitelist: self.get_list(ioctl::GET_WHITELIST)?,
        })
    }

    /// `CLR_SESSION_BLACKLIST` without buffers: 1.7+ succeeds (no side effects beyond this
    /// process's own session entries), 1.5.x rejects the unknown code.
    fn session_api(&self) -> Result<bool> {
        match self.call(ioctl::CLR_SESSION_BLACKLIST, &[], None).map_err(unsupported) {
            Ok(_) => Ok(true),
            Err(Error::Unsupported) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}
