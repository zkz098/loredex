use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use foldhash::fast::RandomState as FastState;
use git2::{DiffOptions, Oid, Repository};
use loredex_core::WikiError;

/// Local git metadata only: foldhash is fast but not HashDoS-resistant.
type FastMap<K, V> = HashMap<K, V, FastState>;
type LineMapCache = RefCell<FastMap<(String, String), Option<Rc<LineMap>>>>;

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

    /// A per-scan cache of git reads. Batch operations (check/verify/plan/
    /// lint/auto) should create one view and share it across pages so tree
    /// walks, diffs and blob reads happen once per command, not once per page.
    pub fn view(&self) -> GitView<'_> {
        GitView::new(self)
    }

    pub fn head_commit(&self) -> Result<String> {
        let head = self
            .repo
            .head()
            .map_err(|e| WikiError::Git(e.to_string()))?;
        let oid = head
            .target()
            .ok_or_else(|| WikiError::Git("HEAD does not point to a commit".into()))?;
        Ok(oid.to_string())
    }

    pub fn head_short(&self) -> Result<String> {
        Ok(self.head_commit()?.chars().take(9).collect::<String>())
    }

    pub fn commit_exists(&self, sha: &str) -> bool {
        match self.repo.revparse_single(sha) {
            Ok(obj) => obj.kind() == Some(git2::ObjectType::Commit) || obj.peel_to_commit().is_ok(),
            Err(_) => false,
        }
    }

    /// Blob oid of `rel_path` at HEAD, or None if the path is not tracked there.
    /// Single-path lookup via the HEAD tree (no full tree walk).
    pub fn head_blob_oid(&self, rel_path: &str) -> Result<Option<String>> {
        let tree = match self.repo.head().and_then(|head| head.peel_to_tree()) {
            Ok(tree) => tree,
            // unborn/no-tree HEAD: nothing is tracked yet
            Err(_) => return Ok(None),
        };
        match tree.get_path(Path::new(rel_path)) {
            Ok(entry) if entry.kind() == Some(git2::ObjectType::Blob) => {
                Ok(Some(entry.id().to_string()))
            }
            Ok(_) => Err(WikiError::Git(format!("{rel_path} is not a blob"))),
            Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(WikiError::Git(e.to_string())),
        }
    }

    /// Content of `rel_path` as stored at HEAD.
    pub fn head_file_content(&self, rel_path: &str) -> Result<Option<String>> {
        let Some(oid) = self.head_blob_oid(rel_path)? else {
            return Ok(None);
        };
        let oid = Oid::from_str(&oid).map_err(|e| WikiError::Git(e.to_string()))?;
        match self.repo.find_blob(oid) {
            Ok(blob) => Ok(Some(String::from_utf8_lossy(blob.content()).into_owned())),
            Err(_) => Ok(None),
        }
    }

    /// Inclusive line range [lo, hi] (1-based) of the file at HEAD.
    pub fn head_lines(&self, rel_path: &str, lo: u32, hi: u32) -> Result<Option<String>> {
        let Some(content) = self.head_file_content(rel_path)? else {
            return Ok(None);
        };
        Ok(lines_of(&content, lo, hi))
    }

    /// Paths from `paths` that changed between `from_commit` and HEAD.
    pub fn changed_paths_since(&self, from_commit: &str, paths: &[&str]) -> Result<Vec<String>> {
        self.view().changed_paths_since(from_commit, paths)
    }

    /// Raw content of any object by oid (used to read the recorded evidence blob).
    pub fn blob_content(&self, oid: &str) -> Result<Option<String>> {
        let oid = Oid::from_str(oid).map_err(|e| WikiError::Git(e.to_string()))?;
        match self.repo.find_blob(oid) {
            Ok(blob) => Ok(Some(String::from_utf8_lossy(blob.content()).into_owned())),
            Err(_) => Ok(None),
        }
    }

    /// All file paths tracked at HEAD (recursive tree walk).
    pub fn tracked_files(&self) -> Result<Vec<String>> {
        self.view().tracked_files()
    }

    /// Find a path at HEAD whose blob has the given oid (content-addressed
    /// rename lookup: an unchanged `git mv` keeps the same oid at the new path).
    pub fn path_of_blob_in_head(&self, oid_str: &str) -> Result<Option<String>> {
        self.view().path_of_blob_in_head(oid_str)
    }

    /// old -> new path pairs renamed between `from_commit` and HEAD
    /// (git similarity rename detection).
    pub fn renames_since(&self, from_commit: &str) -> Vec<(String, String)> {
        self.view().renames_since(from_commit).unwrap_or_default()
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

/// Precomputed HEAD tree: path -> blob/non-blob, sorted tracked files, and the
/// first path for each distinct blob (DFS with name-sorted entries).
#[derive(Default)]
struct TreeIndex {
    paths: FastMap<String, PathKind>,
    files: Vec<String>,
    blobs: FastMap<String, String>,
}

enum PathKind {
    Blob(String),
    Other,
}

impl TreeIndex {
    fn walk(&mut self, repo: &Repository, tree: &git2::Tree, prefix: &str) {
        let mut entries: Vec<_> = tree.iter().collect();
        entries.sort_by_key(|e| e.name().unwrap_or("").to_string());
        for entry in entries {
            let Some(name) = entry.name() else { continue };
            let full = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            if entry.kind() == Some(git2::ObjectType::Tree) {
                if let Ok(obj) = entry.to_object(repo) {
                    if let Ok(subtree) = obj.into_tree() {
                        self.walk(repo, &subtree, &full);
                    }
                }
            } else {
                self.files.push(full.clone());
                match entry.kind() {
                    Some(git2::ObjectType::Blob) => {
                        let oid = entry.id().to_string();
                        self.blobs
                            .entry(oid.clone())
                            .or_insert_with(|| full.clone());
                        self.paths.insert(full, PathKind::Blob(oid));
                    }
                    _ => {
                        self.paths.insert(full, PathKind::Other);
                    }
                }
            }
        }
    }
}

/// Per-command git read cache. All methods borrow `&self` and memoize through
/// `RefCell`; a new view must be created per command/tool call so a moved HEAD
/// is never observed through stale caches.
pub struct GitView<'a> {
    repo: &'a GitRepo,
    head: OnceCell<String>,
    tree: OnceCell<TreeIndex>,
    contents: RefCell<FastMap<String, Option<Rc<String>>>>,
    blob_contents: RefCell<FastMap<String, Option<Rc<String>>>>,
    changed: RefCell<FastMap<String, Rc<BTreeSet<String>>>>,
    /// Paths already covered by the cached diff for a commit. Batch callers
    /// prime this with the union of all evidence paths for that commit; a
    /// single-page caller re-diffs only the newly requested paths.
    changed_coverage: RefCell<FastMap<String, BTreeSet<String>>>,
    renames: RefCell<FastMap<String, Rc<BTreeMap<String, String>>>>,
    commit_exists: RefCell<FastMap<String, bool>>,
    /// (old blob, new path) -> xdiff line map, shared by every citation that
    /// points into the same file.
    line_maps: LineMapCache,
    dirty: OnceCell<bool>,
}

impl<'a> GitView<'a> {
    fn new(repo: &'a GitRepo) -> Self {
        Self {
            repo,
            head: OnceCell::new(),
            tree: OnceCell::new(),
            contents: RefCell::new(FastMap::default()),
            blob_contents: RefCell::new(FastMap::default()),
            changed: RefCell::new(FastMap::default()),
            changed_coverage: RefCell::new(FastMap::default()),
            renames: RefCell::new(FastMap::default()),
            commit_exists: RefCell::new(FastMap::default()),
            line_maps: RefCell::new(FastMap::default()),
            dirty: OnceCell::new(),
        }
    }

    pub fn workdir(&self) -> &Path {
        self.repo.workdir()
    }

    pub fn wiki_dir(&self) -> PathBuf {
        self.repo.wiki_dir()
    }

    pub fn head_commit(&self) -> Result<&str> {
        if let Some(c) = self.head.get() {
            return Ok(c);
        }
        let commit = self.repo.head_commit()?;
        let _ = self.head.set(commit);
        Ok(self.head.get().expect("head set above"))
    }

    pub fn head_short(&self) -> Result<String> {
        Ok(self.head_commit()?.chars().take(9).collect())
    }

    fn tree_index(&self) -> Result<&TreeIndex> {
        if self.tree.get().is_none() {
            let head = self.head_commit()?;
            let oid = Oid::from_str(head).map_err(|e| WikiError::Git(e.to_string()))?;
            let commit = self
                .repo
                .repo
                .find_commit(oid)
                .map_err(|e| WikiError::Git(e.to_string()))?;
            let tree = commit.tree().map_err(|e| WikiError::Git(e.to_string()))?;
            let mut index = TreeIndex::default();
            index.walk(&self.repo.repo, &tree, "");
            index.files.sort();
            let _ = self.tree.set(index);
        }
        Ok(self.tree.get().expect("tree set above"))
    }

    /// Build the full HEAD path index. Batch scans call this once; single-page
    /// tools keep using direct tree lookups so a huge monorepo is not walked
    /// just to resolve one citation.
    pub fn prime_head_index(&self) -> Result<()> {
        self.tree_index().map(|_| ())
    }

    /// Blob oid of `rel_path` at HEAD, or None if not tracked.
    pub fn head_blob_oid(&self, rel_path: &str) -> Result<Option<String>> {
        if let Some(index) = self.tree.get() {
            return match index.paths.get(rel_path) {
                Some(PathKind::Blob(oid)) => Ok(Some(oid.clone())),
                Some(PathKind::Other) => Err(WikiError::Git(format!("{rel_path} is not a blob"))),
                None => Ok(None),
            };
        }
        let tree = match self.repo.repo.head().and_then(|head| head.peel_to_tree()) {
            Ok(tree) => tree,
            Err(_) => return Ok(None),
        };
        match tree.get_path(Path::new(rel_path)) {
            Ok(entry) if entry.kind() == Some(git2::ObjectType::Blob) => {
                Ok(Some(entry.id().to_string()))
            }
            Ok(_) => Err(WikiError::Git(format!("{rel_path} is not a blob"))),
            Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(WikiError::Git(e.to_string())),
        }
    }

    fn cached_content(&self, rel_path: &str) -> Result<Option<Rc<String>>> {
        if let Some(cached) = self.contents.borrow().get(rel_path).cloned() {
            return Ok(cached);
        }
        let content = match self.head_blob_oid(rel_path)? {
            Some(oid) => {
                let oid = Oid::from_str(&oid).map_err(|e| WikiError::Git(e.to_string()))?;
                match self.repo.repo.find_blob(oid) {
                    Ok(blob) => Some(Rc::new(
                        String::from_utf8_lossy(blob.content()).into_owned(),
                    )),
                    Err(_) => None,
                }
            }
            None => None,
        };
        self.contents
            .borrow_mut()
            .insert(rel_path.to_string(), content.clone());
        Ok(content)
    }

    /// Content of `rel_path` at HEAD. The returned Rc is shared across calls.
    pub fn head_file_content(&self, rel_path: &str) -> Result<Option<Rc<String>>> {
        self.cached_content(rel_path)
    }

    /// Inclusive line range [lo, hi] (1-based) of the file at HEAD.
    pub fn head_lines(&self, rel_path: &str, lo: u32, hi: u32) -> Result<Option<String>> {
        let Some(content) = self.cached_content(rel_path)? else {
            return Ok(None);
        };
        Ok(lines_of(&content, lo, hi))
    }

    /// Raw content of any object by oid (recorded evidence blob), cached.
    pub fn blob_content(&self, oid_str: &str) -> Result<Option<Rc<String>>> {
        if let Some(cached) = self.blob_contents.borrow().get(oid_str).cloned() {
            return Ok(cached);
        }
        let content = match Oid::from_str(oid_str) {
            Ok(oid) => match self.repo.repo.find_blob(oid) {
                Ok(blob) => Some(Rc::new(
                    String::from_utf8_lossy(blob.content()).into_owned(),
                )),
                Err(_) => None,
            },
            Err(_) => None,
        };
        self.blob_contents
            .borrow_mut()
            .insert(oid_str.to_string(), content.clone());
        Ok(content)
    }

    /// All file paths tracked at HEAD, sorted.
    pub fn tracked_files(&self) -> Result<Vec<String>> {
        Ok(self.tree_index()?.files.clone())
    }

    /// Find the first path at HEAD whose blob has the given oid.
    pub fn path_of_blob_in_head(&self, oid_str: &str) -> Result<Option<String>> {
        Ok(self.tree_index()?.blobs.get(oid_str).cloned())
    }

    /// Paths from `paths` that changed between `from_commit` and HEAD.
    pub fn changed_paths_since(&self, from_commit: &str, paths: &[&str]) -> Result<Vec<String>> {
        let changed = self.changed_paths(from_commit, paths)?;
        Ok(paths
            .iter()
            .filter(|p| changed.contains(**p))
            .map(|p| p.to_string())
            .collect())
    }

    /// Batch pre-load: one pathspec-limited diff per recorded commit, covering
    /// the union of all evidence paths for that commit. Without this a batch
    /// still works, but grows the cached diff path-by-path.
    pub fn prime_changed_paths<'p, I>(&self, groups: I) -> Result<()>
    where
        I: IntoIterator<Item = (&'p str, Vec<&'p str>)>,
    {
        let head = self.head_commit().ok().map(str::to_string);
        for (commit, paths) in groups {
            if paths.is_empty() || head.as_deref() == Some(commit) || !self.commit_exists(commit) {
                continue;
            }
            self.changed_paths_since(commit, &paths)?;
        }
        Ok(())
    }

    /// Changed-path set cached per commit. Requests are unioned into the cached
    /// coverage so a batch that primes all evidence paths runs exactly one diff
    /// per commit, while a single-page caller re-diffs only new paths.
    fn changed_paths(&self, from_commit: &str, paths: &[&str]) -> Result<Rc<BTreeSet<String>>> {
        let needed: Vec<String> = {
            let coverage = self.changed_coverage.borrow();
            let covered = coverage.get(from_commit);
            paths
                .iter()
                .filter(|p| !covered.map(|c| c.contains(**p)).unwrap_or(false))
                .map(|p| p.to_string())
                .collect()
        };
        if !needed.is_empty() {
            let refs: Vec<&str> = needed.iter().map(String::as_str).collect();
            let added = self.diff_paths(from_commit, &refs)?;
            let mut changed = self.changed.borrow_mut();
            let mut merged = changed
                .get(from_commit)
                .map(|c| (**c).clone())
                .unwrap_or_default();
            merged.extend(added);
            changed.insert(from_commit.to_string(), Rc::new(merged));
            self.changed_coverage
                .borrow_mut()
                .entry(from_commit.to_string())
                .or_default()
                .extend(needed);
        }
        Ok(self
            .changed
            .borrow()
            .get(from_commit)
            .cloned()
            .unwrap_or_else(|| Rc::new(BTreeSet::new())))
    }

    fn diff_paths(&self, from_commit: &str, paths: &[&str]) -> Result<BTreeSet<String>> {
        let mut changed = BTreeSet::new();
        let Some(old_tree) = self.tree_of(from_commit) else {
            return Ok(changed);
        };
        let head_commit = self
            .repo
            .repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .map_err(|e| WikiError::Git(e.to_string()))?;
        let head_tree = head_commit
            .tree()
            .map_err(|e| WikiError::Git(e.to_string()))?;
        let mut opts = DiffOptions::new();
        for p in paths {
            opts.pathspec(*p);
        }
        let diff = self
            .repo
            .repo
            .diff_tree_to_tree(Some(&old_tree), Some(&head_tree), Some(&mut opts))
            .map_err(|e| WikiError::Git(e.to_string()))?;
        for delta in diff.deltas() {
            if let Some(new) = delta.new_file().path() {
                changed.insert(new.display().to_string());
            }
            if let Some(old) = delta.old_file().path() {
                changed.insert(old.display().to_string());
            }
        }
        Ok(changed)
    }

    fn tree_of(&self, commit: &str) -> Option<git2::Tree<'_>> {
        let commit = self.repo.repo.revparse_single(commit).ok()?;
        let commit = commit.peel_to_commit().ok()?;
        commit.tree().ok()
    }

    /// old -> new path pairs renamed between `from_commit` and HEAD.
    pub fn renames_since(&self, from_commit: &str) -> Result<Vec<(String, String)>> {
        let renames = self.rename_map(from_commit);
        Ok(renames
            .iter()
            .map(|(o, n)| (o.clone(), n.clone()))
            .collect())
    }

    /// Rename map cached per commit (best effort: a missing commit yields an
    /// empty map, matching the previous behavior).
    pub fn rename_map(&self, from_commit: &str) -> Rc<BTreeMap<String, String>> {
        if let Some(cached) = self.renames.borrow().get(from_commit).cloned() {
            return cached;
        }
        let mut out = BTreeMap::new();
        if let Some(old_tree) = self.tree_of(from_commit) {
            if let Ok(head_commit) = self.repo.repo.head().and_then(|h| h.peel_to_commit()) {
                if let Ok(head_tree) = head_commit.tree() {
                    if let Ok(mut diff) =
                        self.repo
                            .repo
                            .diff_tree_to_tree(Some(&old_tree), Some(&head_tree), None)
                    {
                        let mut find = git2::DiffFindOptions::new();
                        find.renames(true);
                        if diff.find_similar(Some(&mut find)).is_ok() {
                            for delta in diff.deltas() {
                                if delta.status() == git2::Delta::Renamed {
                                    if let (Some(o), Some(n)) =
                                        (delta.old_file().path(), delta.new_file().path())
                                    {
                                        out.insert(
                                            o.display().to_string(),
                                            n.display().to_string(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let out = Rc::new(out);
        self.renames
            .borrow_mut()
            .insert(from_commit.to_string(), out.clone());
        out
    }

    /// Line map from the recorded blob to `new_path` at HEAD, computed by
    /// libgit2's xdiff and cached per (blob, path). Drift projection uses this
    /// instead of a full-file diff per citation: xdiff is linear-ish with
    /// heuristics, while a pure Myers diff can go quadratic on big rewrites.
    /// `None` means "not text / blob unavailable"; callers fall back to the
    /// pure projection.
    pub fn line_map(&self, old_blob: &str, new_path: &str) -> Result<Option<Rc<LineMap>>> {
        let key = (old_blob.to_string(), new_path.to_string());
        if let Some(cached) = self.line_maps.borrow().get(&key).cloned() {
            return Ok(cached);
        }
        let map = self.compute_line_map(old_blob, new_path)?.map(Rc::new);
        self.line_maps.borrow_mut().insert(key, map.clone());
        Ok(map)
    }

    fn compute_line_map(&self, old_blob: &str, new_path: &str) -> Result<Option<LineMap>> {
        let Some(old_content) = self.blob_content(old_blob)? else {
            return Ok(None);
        };
        let Some(new_content) = self.cached_content(new_path)? else {
            return Ok(None);
        };
        let Ok(oid) = Oid::from_str(old_blob) else {
            return Ok(None);
        };
        let Ok(old_obj) = self.repo.repo.find_blob(oid) else {
            return Ok(None);
        };
        if old_obj.is_binary() {
            return Ok(None);
        }
        let mut opts = DiffOptions::new();
        opts.context_lines(0);
        let patch = match git2::Patch::from_blob_and_buffer(
            &old_obj,
            Some(Path::new(new_path)),
            new_content.as_bytes(),
            Some(Path::new(new_path)),
            Some(&mut opts),
        ) {
            Ok(patch) => patch,
            Err(_) => return Ok(None),
        };

        // Walk diff hunks with the same cursor rules as the line-based
        // reference projection: equal lines map 1:1, deleted lines anchor to
        // the previous surviving new line, inserted lines only advance new.
        let old_len = old_content.split('\n').count();
        let new_len = new_content.split('\n').count();
        let mut map = vec![0u32; old_len];
        let (mut oi, mut ni) = (0usize, 0usize);
        for h in 0..patch.num_hunks() {
            let (hunk, count) = patch.hunk(h).map_err(|e| WikiError::Git(e.to_string()))?;
            // For a pure insertion git reports old_start as the number of old
            // lines before the insertion (0 at the top, N after line N); only
            // hunks that consume old lines use the 1-based line number.
            let hunk_old = if hunk.old_lines() == 0 {
                hunk.old_start() as usize
            } else {
                (hunk.old_start() as usize).saturating_sub(1)
            };
            while oi < hunk_old && oi < map.len() {
                map[oi] = ni as u32;
                oi += 1;
                ni += 1;
            }
            for l in 0..count {
                let line = patch
                    .line_in_hunk(h, l)
                    .map_err(|e| WikiError::Git(e.to_string()))?;
                match line.origin() {
                    ' ' => {
                        if oi < map.len() {
                            map[oi] = ni as u32;
                        }
                        oi += 1;
                        ni += 1;
                    }
                    '-' => {
                        if oi < map.len() {
                            map[oi] = ni.saturating_sub(1) as u32;
                        }
                        oi += 1;
                    }
                    '+' => ni += 1,
                    _ => {}
                }
            }
        }
        while oi < map.len() {
            map[oi] = ni as u32;
            oi += 1;
            ni += 1;
        }
        let max_new = (new_len - 1) as u32;
        for slot in &mut map {
            *slot = (*slot).min(max_new);
        }
        Ok(Some(LineMap { map, new_len }))
    }

    pub fn commit_exists(&self, sha: &str) -> bool {
        if let Some(cached) = self.commit_exists.borrow().get(sha) {
            return *cached;
        }
        let exists = match self.repo.repo.revparse_single(sha) {
            Ok(obj) => obj.kind() == Some(git2::ObjectType::Commit) || obj.peel_to_commit().is_ok(),
            Err(_) => false,
        };
        self.commit_exists
            .borrow_mut()
            .insert(sha.to_string(), exists);
        exists
    }

    pub fn is_dirty(&self) -> Result<bool> {
        if let Some(cached) = self.dirty.get() {
            return Ok(*cached);
        }
        let dirty = self.repo.is_dirty()?;
        let _ = self.dirty.set(dirty);
        Ok(dirty)
    }
}

/// 0-based line projection from an old blob onto the current file content.
/// `map[old_line]` is the new line a deleted old line anchors to (the
/// previous surviving line); equal lines map 1:1.
pub struct LineMap {
    map: Vec<u32>,
    new_len: usize,
}

impl LineMap {
    /// Project a 0-based inclusive old range. A collapsed range keeps its
    /// width when possible, matching the reference projection rules.
    pub fn project(&self, lo: usize, hi: usize) -> (usize, usize) {
        let last = self.map.len() - 1;
        let lo = lo.min(last);
        let hi = hi.min(last);
        let mut nlo = self.map[lo] as usize;
        let mut nhi = self.map[hi] as usize;
        if nhi < nlo {
            std::mem::swap(&mut nlo, &mut nhi);
        }
        if nlo == nhi && hi > lo {
            nhi = (nlo + (hi - lo)).min(self.new_len - 1);
        }
        (nlo, nhi)
    }
}

fn lines_of(content: &str, lo: u32, hi: u32) -> Option<String> {
    let lo = lo.max(1) as usize;
    let hi = hi as usize;
    if hi < lo {
        return Some(String::new());
    }
    Some(
        content
            .split('\n')
            .skip(lo - 1)
            .take(hi - lo + 1)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

#[cfg(test)]
mod tests {
    use super::lines_of;

    #[test]
    fn extracts_requested_window() {
        let content = "a\nb\nc\nd\n";
        assert_eq!(lines_of(content, 2, 3).as_deref(), Some("b\nc"));
        assert_eq!(lines_of(content, 1, 99).as_deref(), Some("a\nb\nc\nd\n"));
    }

    #[test]
    fn keeps_split_newline_semantics_at_edges() {
        let content = "a\nb\n";
        // split('\n') yields a trailing empty segment
        assert_eq!(lines_of(content, 3, 3).as_deref(), Some(""));
        assert_eq!(lines_of(content, 4, 6).as_deref(), Some(""));
        assert_eq!(lines_of(content, 3, 1).as_deref(), Some(""));
    }
}
