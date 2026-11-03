//! Cross-process locks for the few read-modify-write sections that cannot be
//! made safe by content addressing alone: the page write in `put` and the run
//! ledger in `plan`/`claim`. Lock files live under the gitignored
//! `.wiki/.cache/locks/`.
//!
//! The lock is an OS advisory lock on an open file handle (`flock` on Unix,
//! `LockFileEx` on Windows, via `std::fs::File::try_lock`), not the existence
//! of a file. The kernel releases it when the handle closes — on drop, and
//! equally when the holding process crashes or is killed — so there is no
//! stale-lock heuristic to get wrong and no window where two processes both
//! believe they reclaimed the same lock. Lock files are therefore never
//! deleted: unlinking a lock file another process has open would let a third
//! process lock a fresh inode while the second still holds the old one.
//!
//! This is a coordination mechanism between cooperating loredex processes,
//! not a security boundary: anything that ignores advisory locks can still
//! write the files it guards.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::Path;
use std::time::{Duration, Instant};

use loredex_core::WikiError;

use crate::Result;

const WAIT: Duration = Duration::from_secs(5);
const POLL_MIN: Duration = Duration::from_millis(2);
const POLL_MAX: Duration = Duration::from_millis(50);

/// Held for the duration of a critical section; dropping it closes the
/// handle, which releases the OS lock.
#[derive(Debug)]
pub struct FileLock {
    _file: File,
}

impl FileLock {
    /// Acquire the ledger lock (`runs.json` is a shared read-modify-write
    /// document, so every writer has to serialize).
    pub fn ledger(wiki_dir: &Path) -> Result<Self> {
        Self::acquire(wiki_dir, "runs.lock", WAIT)
    }

    /// Acquire the lock for one page id. Hashed so an id can never escape the
    /// lock directory, and independent pages never contend.
    pub fn page(wiki_dir: &Path, id: &str) -> Result<Self> {
        let name = format!("page-{}.lock", crate::sha1_hex(id.as_bytes()));
        Self::acquire(wiki_dir, &name, WAIT)
    }

    fn acquire(wiki_dir: &Path, name: &str, timeout: Duration) -> Result<Self> {
        let dir = wiki_dir.join(".cache").join("locks");
        fs::create_dir_all(&dir).map_err(WikiError::Io)?;
        let path = dir.join(name);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(WikiError::Io)?;
        let deadline = Instant::now() + timeout;
        let mut pause = POLL_MIN;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(TryLockError::WouldBlock) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(WikiError::Conflict(format!(
                            "wiki is busy: lock `{}` is held by another process; retry shortly",
                            path.display()
                        )));
                    }
                    std::thread::sleep(pause.min(deadline - now));
                    pause = (pause * 2).min(POLL_MAX);
                }
                Err(TryLockError::Error(e)) => return Err(WikiError::Io(e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_holder_times_out_until_first_drops() {
        let tmp = tempfile::TempDir::new().unwrap();
        let first = FileLock::acquire(tmp.path(), "x.lock", WAIT).unwrap();
        let err = FileLock::acquire(tmp.path(), "x.lock", Duration::from_millis(30)).unwrap_err();
        assert!(matches!(err, WikiError::Conflict(_)), "{err:?}");
        drop(first);
        FileLock::acquire(tmp.path(), "x.lock", Duration::from_millis(30)).unwrap();
    }

    #[test]
    fn leftover_lock_file_does_not_block() {
        // A crashed holder leaves the file behind but no OS lock on it.
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join(".cache").join("locks");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("runs.lock"), "pid=1 at=yesterday").unwrap();
        FileLock::ledger(tmp.path()).unwrap();
    }

    #[test]
    fn distinct_pages_do_not_contend() {
        let tmp = tempfile::TempDir::new().unwrap();
        let _a = FileLock::page(tmp.path(), "a").unwrap();
        let _b = FileLock::page(tmp.path(), "b").unwrap();
    }
}
