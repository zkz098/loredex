//! Best-effort cross-process locks for the few read-modify-write sections
//! that cannot be made safe by content addressing alone: the page write in
//! `put` and the run ledger in `plan`/`claim`. Lock files live under the
//! gitignored `.wiki/.cache/locks/` and are removed on drop.
//!
//! This is a coordination hint, not a security boundary: a crashed process
//! leaves a lock behind, so locks older than `STALE_AFTER` are reclaimed.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use loredex_core::WikiError;

use crate::Result;

const STALE_AFTER: Duration = Duration::from_secs(60);
const WAIT: Duration = Duration::from_secs(5);

pub struct FileLock {
    path: PathBuf,
    token: String,
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
        let deadline = Instant::now() + timeout;
        loop {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    let nonce = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0);
                    let token = format!(
                        "pid={} at={} nonce={nonce}",
                        std::process::id(),
                        crate::now_iso()
                    );
                    let _ = write!(f, "{token}");
                    return Ok(Self { path, token });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if reclaim_stale(&path) {
                        continue;
                    }
                    if Instant::now() >= deadline {
                        return Err(WikiError::Conflict(format!(
                            "wiki is busy: lock `{}` is held by another process; retry shortly",
                            path.display()
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(WikiError::Io(e)),
            }
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Only remove our own lock: a lock older than STALE_AFTER may have
        // been reclaimed by another process while this one was stalled.
        if fs::read_to_string(&self.path).ok().as_deref() == Some(self.token.as_str()) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn reclaim_stale(path: &Path) -> bool {
    let stale = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|age| age > STALE_AFTER)
        .unwrap_or(false);
    if !stale {
        return false;
    }
    // Re-read the token before deleting. If another process already reclaimed
    // and re-created the lock, the contents differ and we leave it alone.
    let Ok(before) = fs::read(path) else {
        return false;
    };
    let Ok(after) = fs::read(path) else {
        return false;
    };
    before == after && fs::remove_file(path).is_ok()
}
