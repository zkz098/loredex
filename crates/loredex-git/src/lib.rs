use std::path::{Path, PathBuf};

use git2::{DiffOptions, Repository};
use loredex_core::WikiError;

pub type Result<T> = std::result::Result<T, WikiError>;

pub struct GitRepo {
    repo: Repository,
    workdir: PathBuf,
}

impl GitRepo {
    /// Open the repository containing `path` (walks up like git does).
    pub fn discover(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path).map_err(|e| WikiError::Git(e.to_string()))?;
        let workdir = repo
            .workdir()
            .ok_or_else(|| WikiError::Git("bare repository has no workdir".into()))?
            .to_path_buf();
        Ok(Self { repo, workdir })
    }

    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    pub fn wiki_dir(&self) -> PathBuf {
        self.workdir.join(".wiki")
    }

    pub fn head_commit(&self) -> Result<String> {
        let head = self.repo.head().map_err(|e| WikiError::Git(e.to_string()))?;
        let oid = head
            .target()
            .ok_or_else(|| WikiError::Git("HEAD does not point to a commit".into()))?;
        Ok(oid.to_string())
    }

    pub fn head_short(&self) -> Result<String> {
        Ok(self.head_commit()?
            .chars()
            .take(9)
            .collect::<String>())
    }

    pub fn commit_exists(&self, sha: &str) -> bool {
        match self.repo.revparse_single(sha) {
            Ok(obj) => obj.kind() == Some(git2::ObjectType::Commit) || obj.peel_to_commit().is_ok(),
            Err(_) => false,
        }
    }

    /// Blob oid of `rel_path` at HEAD, or None if the path is not tracked there.
    pub fn head_blob_oid(&self, rel_path: &str) -> Result<Option<String>> {
        let spec = format!("HEAD:{rel_path}");
        match self.repo.revparse_single(&spec) {
            Ok(obj) => {
                let blob = obj
                    .into_blob()
                    .map_err(|_| WikiError::Git(format!("{rel_path} is not a blob")))?;
                Ok(Some(blob.id().to_string()))
            }
            Err(_) => Ok(None),
        }
    }

    /// Content of `rel_path` as stored at HEAD.
    pub fn head_file_content(&self, rel_path: &str) -> Result<Option<String>> {
        let spec = format!("HEAD:{rel_path}");
        match self.repo.revparse_single(&spec) {
            Ok(obj) => {
                let blob = obj
                    .into_blob()
                    .map_err(|_| WikiError::Git(format!("{rel_path} is not a blob")))?;
                Ok(Some(String::from_utf8_lossy(&blob.content()).into_owned()))
            }
            Err(_) => Ok(None),
        }
    }

    /// Inclusive line range [lo, hi] (1-based) of the file at HEAD.
    pub fn head_lines(&self, rel_path: &str, lo: u32, hi: u32) -> Result<Option<String>> {
        let Some(content) = self.head_file_content(rel_path)? else {
            return Ok(None);
        };
        let lines: Vec<&str> = content.split('\n').collect();
        let lo = lo.max(1) as usize;
        let hi = (hi as usize).min(lines.len());
        if lo > lines.len() {
            return Ok(Some(String::new()));
        }
        Ok(Some(lines[lo - 1..hi].join("\n")))
    }

    /// Paths from `paths` that changed between `from_commit` and HEAD.
    pub fn changed_paths_since(&self, from_commit: &str, paths: &[&str]) -> Result<Vec<String>> {
        let old = self
            .repo
            .revparse_single(from_commit)
            .and_then(|obj| obj.peel_to_commit())
            .map_err(|e| WikiError::Git(format!("cannot resolve {from_commit}: {e}")));
        let old = match old {
            Ok(c) => c,
            Err(_) => return Ok(Vec::new()),
        };
        let old_tree = old
            .tree()
            .map_err(|e| WikiError::Git(e.to_string()))?;
        let head_commit = self
            .repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .map_err(|e| WikiError::Git(e.to_string()))?;
        let head_tree = head_commit
            .tree()
            .map_err(|e| WikiError::Git(e.to_string()))?;

        let mut opts = DiffOptions::new();
        let mut changed = Vec::new();
        for p in paths {
            opts.pathspec(*p);
        }
        let diff = self
            .repo
            .diff_tree_to_tree(Some(&old_tree), Some(&head_tree), Some(&mut opts))
            .map_err(|e| WikiError::Git(e.to_string()))?;
        for delta in diff.deltas() {
            if let Some(new) = delta.new_file().path() {
                changed.push(new.display().to_string());
            }
            if let Some(old) = delta.old_file().path() {
                let s = old.display().to_string();
                if !changed.contains(&s) {
                    changed.push(s);
                }
            }
        }
        Ok(changed)
    }

    /// Raw content of any object by oid (used to read the recorded evidence blob).
    pub fn blob_content(&self, oid: &str) -> Result<Option<String>> {
        let oid = git2::Oid::from_str(oid).map_err(|e| WikiError::Git(e.to_string()))?;
        match self.repo.find_blob(oid) {
            Ok(blob) => Ok(Some(String::from_utf8_lossy(&blob.content()).into_owned())),
            Err(_) => Ok(None),
        }
    }

    /// All file paths tracked at HEAD (recursive tree walk).
    pub fn tracked_files(&self) -> Result<Vec<String>> {
        let commit = self
            .repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .map_err(|e| WikiError::Git(e.to_string()))?;
        let tree = commit.tree().map_err(|e| WikiError::Git(e.to_string()))?;
        let mut out = Vec::new();
        Self::walk_tree(&self.repo, &tree, "", &mut out);
        out.sort();
        Ok(out)
    }

    fn walk_tree(repo: &Repository, tree: &git2::Tree, prefix: &str, out: &mut Vec<String>) {
        for entry in tree {
            let Some(name) = entry.name() else { continue };
            let full = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            if entry.kind() == Some(git2::ObjectType::Tree) {
                if let Ok(obj) = entry.to_object(repo) {
                    if let Ok(subtree) = obj.into_tree() {
                        Self::walk_tree(repo, &subtree, &full, out);
                    }
                }
            } else {
                out.push(full);
            }
        }
    }

    /// Find a path at HEAD whose blob has the given oid (content-addressed
    /// rename lookup: an unchanged `git mv` keeps the same oid at the new path).
    pub fn path_of_blob_in_head(&self, oid_str: &str) -> Result<Option<String>> {
        let oid = git2::Oid::from_str(oid_str).map_err(|e| WikiError::Git(e.to_string()))?;
        let commit = self
            .repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .map_err(|e| WikiError::Git(e.to_string()))?;
        let tree = commit.tree().map_err(|e| WikiError::Git(e.to_string()))?;
        let mut found: Option<String> = None;
        Self::find_blob_walk(&self.repo, &tree, "", &oid, &mut found);
        Ok(found)
    }

    fn find_blob_walk(
        repo: &Repository,
        tree: &git2::Tree,
        prefix: &str,
        want: &git2::Oid,
        found: &mut Option<String>,
    ) {
        if found.is_some() {
            return;
        }
        let mut entries: Vec<_> = tree.iter().collect();
        entries.sort_by_key(|e| e.name().unwrap_or("").to_string());
        for entry in entries {
            if found.is_some() {
                return;
            }
            let Some(name) = entry.name() else { continue };
            let full = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            if entry.kind() == Some(git2::ObjectType::Tree) {
                if let Ok(obj) = entry.to_object(repo) {
                    if let Ok(subtree) = obj.into_tree() {
                        Self::find_blob_walk(repo, &subtree, &full, want, found);
                    }
                }
            } else if entry.id() == *want {
                *found = Some(full);
                return;
            }
        }
    }

    /// old -> new path pairs renamed between `from_commit` and HEAD
    /// (git similarity rename detection).
    pub fn renames_since(&self, from_commit: &str) -> Vec<(String, String)> {
        let Ok(old) = self
            .repo
            .revparse_single(from_commit)
            .and_then(|o| o.peel_to_commit())
        else {
            return Vec::new();
        };
        let Ok(old_tree) = old.tree() else { return Vec::new() };
        let Ok(head_commit) = self.repo.head().and_then(|h| h.peel_to_commit()) else {
            return Vec::new();
        };
        let Ok(head_tree) = head_commit.tree() else {
            return Vec::new();
        };
        let Ok(mut diff) = self
            .repo
            .diff_tree_to_tree(Some(&old_tree), Some(&head_tree), None)
        else {
            return Vec::new();
        };
        let mut find = git2::DiffFindOptions::new();
        find.renames(true);
        if diff.find_similar(Some(&mut find)).is_err() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for delta in diff.deltas() {
            if delta.status() == git2::Delta::Renamed {
                if let (Some(o), Some(n)) = (delta.old_file().path(), delta.new_file().path()) {
                    out.push((o.display().to_string(), n.display().to_string()));
                }
            }
        }
        out
    }

    /// True if tracked files have uncommitted changes (ignores untracked files).
    pub fn is_dirty(&self) -> Result<bool> {
        let statuses = self
            .repo
            .statuses(None)
            .map_err(|e| WikiError::Git(e.to_string()))?;
        Ok(statuses.iter().any(|s| {
            let f = s.status();
            !(f.is_wt_new() || f.is_index_new() || f.is_ignored())
        }))
    }
}
