//! aikm-hidhide-guard <pid> <creation time>: waits for the mapper to exit by any means
//! (crash, kill) and rolls back the journal if the mapper left it dirty.
//! Writes one line per run to `%LOCALAPPDATA%\aikeymapper\guard.log`.

use std::fs::OpenOptions;
use std::io::Write;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hidhide_cli::journal::Journal;
use hidhide_cli::ops;
use hidhide_cli::procs::{self, Owner};

fn main() {
    let args: Vec<u64> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let [pid, start] = args[..] else {
        eprintln!("usage: aikm-hidhide-guard <pid> <creation time>");
        std::process::exit(2);
    };
    let owner = Owner { pid: pid as u32, start };
    let waited = procs::wait_exit(owner);

    // HidHide Client may hold the exclusive handle for a while: keep trying for ~30 s.
    let mut res = ops::restore_if_owner(owner);
    for _ in 0..15 {
        match &res {
            Err(e) if e.downcast_ref::<hidhide::Error>().is_some_and(|e| matches!(e, hidhide::Error::Busy)) => {
                sleep(Duration::from_secs(2));
                res = ops::restore_if_owner(owner);
            }
            _ => break,
        }
    }
    let what = match res {
        Ok(Some(j)) => format!(
            "откатил: blacklist {:?}, whitelist {:?}, Active → {:?}",
            j.added_blacklist, j.added_whitelist, j.prev_active
        ),
        Ok(None) => "журнал чистый или не наш, ничего не делал".to_string(),
        Err(e) => format!("ошибка: {e}"),
    };
    let when = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let line = format!(
        "{when} pid {pid} {}; {what}\n",
        if waited { "завершился" } else { "уже не жил" }
    );
    let log = Journal::path().with_file_name("guard.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log) {
        let _ = f.write_all(line.as_bytes());
    }
}
