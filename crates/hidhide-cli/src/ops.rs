//! Journal-first mutations. Adding: the journal is saved before the driver changes, so a crash
//! in between never leaves an untracked entry. Removing: the driver changes first, then the
//! journal, for the same reason. The journal is read and written while the exclusive
//! `\\.\HidHide` handle is held, which serialises concurrent callers.

use hidhide::HidHide;

use crate::journal::Journal;
use crate::procs::Owner;
use crate::Result;

#[derive(Debug, Default)]
pub struct Added {
    pub blacklist: Vec<String>,
    pub whitelist: Vec<String>,
    pub was_active: bool,
}

/// Adds the missing `blacklist`/`whitelist` entries and turns `Active` on.
/// `owner: None` means the hide outlives this process until `unhide`/`restore`.
pub fn hide(blacklist: &[String], whitelist: &[String], owner: Option<Owner>) -> Result<Added> {
    let mut added = Added::default();
    HidHide.edit(|cfg| -> Result<()> {
        let mut j = Journal::load()?;
        if j.dirty && j.owner() != owner && j.owner().is_some() && !j.orphaned() {
            return Err(format!("скрытие уже держит процесс {}, сначала restore", j.owner_pid).into());
        }
        if !j.dirty {
            j = Journal::default();
        }
        added.blacklist = missing(blacklist, &cfg.blacklist);
        added.whitelist = missing(whitelist, &cfg.whitelist);
        added.was_active = cfg.active;

        j.prev_active.get_or_insert(cfg.active);
        j.added_blacklist.extend(added.blacklist.iter().cloned());
        j.added_whitelist.extend(added.whitelist.iter().cloned());
        j.owner_pid = owner.map_or(0, |o| o.pid);
        j.owner_start = owner.map_or(0, |o| o.start);
        j.dirty = true;
        j.save()?;

        cfg.whitelist.extend(added.whitelist.iter().cloned());
        cfg.blacklist.extend(added.blacklist.iter().cloned());
        cfg.active = true;
        Ok(())
    })?;
    Ok(added)
}

/// Takes our blacklist entries out (all, or just `only`) and, once none are left, gives
/// `Active` back. Whitelist additions stay until `restore`. Returns the entries removed and
/// the `Active` value given back, if it was.
pub fn unhide(only: &[String]) -> Result<(Vec<String>, Option<bool>)> {
    let mut removed = Vec::new();
    let mut active = None;
    let mut after = None;
    HidHide.edit(|cfg| -> Result<()> {
        let mut j = Journal::load()?;
        if !j.dirty {
            return Ok(());
        }
        removed = match only {
            [] => j.added_blacklist.clone(),
            ids => j.added_blacklist.iter().filter(|p| ids.contains(p)).cloned().collect(),
        };
        cfg.blacklist.retain(|p| !removed.contains(p));
        j.added_blacklist.retain(|p| !removed.contains(p));
        if j.added_blacklist.is_empty() {
            if let Some(a) = j.prev_active.take() {
                cfg.active = a;
                active = Some(a);
            }
        }
        if !j.has_changes() {
            j = Journal::default();
        }
        after = Some(j);
        Ok(())
    })?;
    if let Some(j) = after {
        j.save()?;
    }
    Ok((removed, active))
}

/// Undoes everything the journal holds. Returns what was undone, `None` if it was clean.
pub fn restore() -> Result<Option<Journal>> {
    restore_if(|_| true)
}

/// For the guard: restores only if the journal still belongs to `owner`.
pub fn restore_if_owner(owner: Owner) -> Result<Option<Journal>> {
    restore_if(|j| j.owned_by(owner))
}

/// Startup recovery: the owner died without cleaning up.
pub fn recover_orphan() -> Result<Option<Journal>> {
    restore_if(Journal::orphaned)
}

fn restore_if(pred: impl FnOnce(&Journal) -> bool) -> Result<Option<Journal>> {
    let mut undone = None;
    HidHide.edit(|cfg| -> Result<()> {
        let j = Journal::load()?;
        if !j.dirty || !pred(&j) {
            return Ok(());
        }
        cfg.blacklist.retain(|p| !j.added_blacklist.contains(p));
        cfg.whitelist.retain(|p| !j.added_whitelist.contains(p));
        if let Some(a) = j.prev_active {
            cfg.active = a;
        }
        undone = Some(j);
        Ok(())
    })?;
    if undone.is_some() {
        Journal::default().save()?;
    }
    Ok(undone)
}

/// Exact match: the driver may compare instance paths case-sensitively, and an entry of
/// another case is the user's, not ours.
fn missing(wanted: &[String], present: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in wanted {
        if !present.contains(w) && !out.contains(w) {
            out.push(w.clone());
        }
    }
    out
}
