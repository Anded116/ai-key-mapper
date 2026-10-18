//! `%LOCALAPPDATA%\aikeymapper\hidhide-journal.json`, written atomically
//! (temp file + `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)`).

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use windows::core::HSTRING;
use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};

use crate::procs::{self, Owner};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    /// 0: no owner, the hide stays until an explicit `unhide`/`restore`.
    pub owner_pid: u32,
    pub owner_start: u64,
    pub dirty: bool,
    /// `Active` before our first change; `None` once it has been given back.
    pub prev_active: Option<bool>,
    /// Only entries that were absent from the driver's list when we added them.
    pub added_blacklist: Vec<String>,
    pub added_whitelist: Vec<String>,
}

impl Journal {
    pub fn path() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        base.join("aikeymapper").join("hidhide-journal.json")
    }

    pub fn load() -> io::Result<Self> {
        match fs::read(Self::path()) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::path();
        fs::create_dir_all(path.parent().unwrap())?;
        let tmp = path.with_extension("json.tmp");
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&serde_json::to_vec_pretty(self).map_err(io::Error::other)?)?;
        f.sync_all()?;
        drop(f);
        unsafe {
            MoveFileExW(
                &HSTRING::from(tmp.as_os_str()),
                &HSTRING::from(path.as_os_str()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(io::Error::other)
    }

    pub fn owner(&self) -> Option<Owner> {
        (self.owner_pid != 0).then_some(Owner { pid: self.owner_pid, start: self.owner_start })
    }

    pub fn owned_by(&self, who: Owner) -> bool {
        self.dirty && self.owner() == Some(who)
    }

    /// Dirty, and the process that owned it is gone (crash, kill, reboot).
    pub fn orphaned(&self) -> bool {
        self.dirty && self.owner().is_some_and(|o| !procs::is_alive(o))
    }

    pub fn has_changes(&self) -> bool {
        self.prev_active.is_some() || !self.added_blacklist.is_empty() || !self.added_whitelist.is_empty()
    }
}
