//! DOS path → NT path (`\Device\HarddiskVolumeN\…`), the form HidHide keeps in its whitelist.

use std::path::{Path, PathBuf};

use windows::core::HSTRING;
use windows::Win32::Storage::FileSystem::{
    GetVolumeNameForVolumeMountPointW, GetVolumePathNameW, QueryDosDeviceW,
};

use crate::{Error, Result};

/// Same steps as HidHide's `FileNameToFullImageName`: volume mount point →
/// `\\?\Volume{GUID}\` → `QueryDosDevice`. Junctions and symlinks are resolved first,
/// because the driver matches the image path the kernel reports.
pub fn dos_to_nt_path(p: &Path) -> Result<String> {
    let full = full_path(p)?;
    let bad = |why: &'static str| Error::Path(full.clone(), why);
    let text = full.to_str().ok_or_else(|| bad("not valid unicode"))?;

    let mut mount = [0u16; 1024];
    unsafe { GetVolumePathNameW(&HSTRING::from(text), &mut mount) }.map_err(Error::Os)?;
    let mount = from_wide(&mount);
    let rest = text.get(mount.len()..).ok_or_else(|| bad("unexpected volume mount point"))?;

    let mut volume = [0u16; 64];
    unsafe { GetVolumeNameForVolumeMountPointW(&HSTRING::from(mount.as_str()), &mut volume) }
        .map_err(Error::Os)?;
    let volume = from_wide(&volume);
    let dos_name = volume
        .strip_prefix(r"\\?\")
        .and_then(|v| v.strip_suffix('\\'))
        .ok_or_else(|| bad("unexpected volume name"))?;

    let mut device = [0u16; 1024];
    let n = unsafe { QueryDosDeviceW(&HSTRING::from(dos_name), Some(&mut device)) };
    if n == 0 {
        return Err(Error::Os(windows::core::Error::from_thread()));
    }
    let device = from_wide(&device);

    Ok(format!(r"{device}\{rest}"))
}

fn full_path(p: &Path) -> Result<PathBuf> {
    let full = match p.canonicalize() {
        Ok(c) => c,
        Err(_) => std::path::absolute(p).map_err(|_| Error::Path(p.to_owned(), "cannot resolve"))?,
    };
    let text = full.to_string_lossy();
    if text.starts_with(r"\\?\UNC\") || (text.starts_with(r"\\") && !text.starts_with(r"\\?\")) {
        return Err(Error::Path(full, "network paths are not supported"));
    }
    Ok(match text.strip_prefix(r"\\?\") {
        Some(s) => PathBuf::from(s),
        None => full,
    })
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_exe_maps_to_device_path() {
        let exe = std::env::current_exe().unwrap();
        let nt = dos_to_nt_path(&exe).unwrap();
        assert!(nt.starts_with(r"\Device\"), "{nt}");
        let name = exe.file_name().unwrap().to_str().unwrap();
        assert!(nt.ends_with(name), "{nt}");
        assert!(!nt.contains(r"\\"), "{nt}");
    }
}
