use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use foldhash::fast::RandomState as FastState;
use loredex_core::WikiError;
use similar::{ChangeTag, TextDiff};

/// Local git metadata only: foldhash is fast but not HashDoS-resistant.
type FastMap<K, V> = HashMap<K, V, FastState>;
type LineMapCache = RefCell<FastMap<(String, String), Option<Rc<LineMap>>>>;

pub type Result<T> = std::result::Result<T, WikiError>;

pub struct GitRepo {
    repo: gix::Repository,
    workdir: PathBuf,
}

impl GitRepo {
    /// Open the repository containing `path` (walks up like git does).
    pub fn discover(path: &Path) -> Result<Self> {
        let repo = gix::discover(path).map_err(|e| WikiError::Git(e.to_string()))?;
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
        let id = head
            .id()
            .ok_or_else(|| WikiError::Git("HEAD does not point to a commit".into()))?;
        Ok(id.to_string())
    }

    pub fn head_short(&self) -> Result<String> {
        Ok(self.head_commit()?.chars().take(9).collect::<String>())
    }

    pub fn commit_exists(&self, sha: &str) -> bool {
        self.view().commit_exists(sha)
    }

    /// Blob oid of `rel_path` at HEAD, or None if the path is not tracked there.
    /// Single-path lookup via the HEAD tree (no full tree walk).
    pub fn head_blob_oid(&self, rel_path: &str) -> Result<Option<String>> {
        self.view().head_blob_oid(rel_path)
    }

    /// Content of `rel_path` as stored at HEAD.
    pub fn head_file_content(&self, rel_path: &str) -> Result<Option<String>> {
        let Some(content) = self.view().head_file_content(rel_path)? else {
            return Ok(None);
        };
        Ok(Some((*content).clone()))
    }

    /// Inclusive line range [lo, hi] (1-based) of the file at HEAD.
    pub fn head_lines(&self, rel_path: &str, lo: u32, hi: u32) -> Result<Option<String>> {
        self.view().head_lines(rel_path, lo, hi)
    }

    /// Paths from `paths` that changed between `from_commit` and HEAD.
    pub fn changed_paths_since(&self, from_commit: &str, paths: &[&str]) -> Result<Vec<String>> {
        self.view().changed_paths_since(from_commit, paths)
    }

    /// Raw content of any object by oid (used to read the recorded evidence blob).
    pub fn blob_content(&self, oid: &str) -> Result<Option<String>> {
        let Some(content) = self.view().blob_content(oid)? else {
            return Ok(None);
        };
        Ok(Some((*content).clone()))
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
        self.repo
            .is_dirty()
            .map_err(|e| WikiError::Git(e.to_string()))
    }

    /// Structured change set between any resolvable ref and HEAD (see `GitView::path_deltas`).
    pub fn path_deltas(&self, since_ref: &str) -> Result<Vec<PathDelta>> {
        self.view().path_deltas(since_ref)
    }
}

/// A whole-repo path change between a ref and HEAD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathDelta {
    pub old: Option<String>,
    pub new: Option<String>,
    pub change: PathChange,
}

impl PathDelta {
    pub fn new_path(&self) -> Option<&str> {
        self.new.as_deref()
    }

    pub fn old_path(&self) -> Option<&str> {
        self.old.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathChange {
    Modified,
    Added,
    Deleted,
    Renamed,
}

impl PathChange {
    pub fn as_str(&self) -> &'static str {
        match self {
            PathChange::Modified => "modified",
            PathChange::Added => "added",
            PathChange::Deleted => "deleted",
            PathChange::Renamed => "renamed",
        }
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
    fn build(repo: &gix::Repository, tree: &gix::Tree) -> Result<Self> {
        let mut index = TreeIndex::default();
        Self::walk(repo, tree, "", &mut index)?;
        index.files.sort();
        Ok(index)
    }

    fn walk(
        repo: &gix::Repository,
        tree: &gix::Tree,
        prefix: &str,
        index: &mut TreeIndex,
    ) -> Result<()> {
        for entry in tree.iter() {
            let entry = entry.map_err(|e| WikiError::Git(e.to_string()))?;
            let name = entry.filename().to_string();
            let full = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.mode().is_tree() {
                if let Ok(obj) = repo.find_object(entry.oid()) {
                    if let Ok(subtree) = obj.try_into_tree() {
                        Self::walk(repo, &subtree, &full, index)?;
                    }
                }
            } else {
                index.files.push(full.clone());
                if entry.mode().is_blob() {
                    let oid = entry.oid().to_string();
                    index
                        .blobs
                        .entry(oid.clone())
                        .or_insert_with(|| full.clone());
                    index.paths.insert(full, PathKind::Blob(oid));
                } else {
                    index.paths.insert(full, PathKind::Other);
                }
            }
        }
        Ok(())
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
    /// (old blob, new path) -> line map, shared by every citation that
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

    fn head_tree(&self) -> Result<gix::Tree<'_>> {
        let mut head = self
            .repo
            .repo
            .head()
            .map_err(|e| WikiError::Git(e.to_string()))?;
        let commit = head
            .peel_to_commit()
            .map_err(|e| WikiError::Git(e.to_string()))?;
        commit.tree().map_err(|e| WikiError::Git(e.to_string()))
    }

    fn tree_index(&self) -> Result<&TreeIndex> {
        if self.tree.get().is_none() {
            let tree = self.head_tree()?;
            let index = TreeIndex::build(&self.repo.repo, &tree)?;
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
        let tree = match self.head_tree() {
            Ok(tree) => tree,
            Err(_) => return Ok(None),
        };
        match tree.lookup_entry_by_path(Path::new(rel_path)) {
            Ok(Some(entry)) if entry.mode().is_blob() => Ok(Some(entry.object_id().to_string())),
            Ok(Some(_)) => Err(WikiError::Git(format!("{rel_path} is not a blob"))),
            Ok(None) => Ok(None),
            Err(e) => Err(WikiError::Git(e.to_string())),
        }
    }

    fn cached_content(&self, rel_path: &str) -> Result<Option<Rc<String>>> {
        if let Some(cached) = self.contents.borrow().get(rel_path).cloned() {
            return Ok(cached);
        }
        let content = match self.head_blob_oid(rel_path)? {
            Some(oid) => self.blob_content(&oid)?,
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
        let content = match gix::ObjectId::from_hex(oid_str.as_bytes()) {
            Ok(oid) => match self.repo.repo.find_object(oid) {
                Ok(obj) if obj.kind == gix::object::Kind::Blob => {
                    Some(Rc::new(String::from_utf8_lossy(&obj.data).into_owned()))
                }
                _ => None,
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

    /// Structured change set between any resolvable ref (commit sha, branch,
    /// tag, `HEAD~n`) and HEAD, with similarity-based rename pairing.
    pub fn path_deltas(&self, since_ref: &str) -> Result<Vec<PathDelta>> {
        let Some(old_tree) = self.tree_of(since_ref) else {
            return Err(WikiError::Git(format!(
                "{since_ref} does not resolve to a commit with a tree"
            )));
        };
        let head_tree = self.head_tree()?;
        let diff = self
            .repo
            .repo
            .diff_tree_to_tree(Some(&old_tree), Some(&head_tree), None)
            .map_err(|e| WikiError::Git(e.to_string()))?;

        let mut out = Vec::new();
        let is_wiki = |p: &Option<String>| {
            p.as_deref()
                .map(|x| x == ".wiki" || x.starts_with(".wiki/"))
                .unwrap_or(false)
        };

        for change in diff {
            let (old, new, path_change, mode) = match change {
                gix::object::tree::diff::ChangeDetached::Addition {
                    location,
                    entry_mode,
                    ..
                } => (
                    None,
                    Some(location.to_string()),
                    PathChange::Added,
                    entry_mode,
                ),
                gix::object::tree::diff::ChangeDetached::Deletion {
                    location,
                    entry_mode,
                    ..
                } => (
                    Some(location.to_string()),
                    None,
                    PathChange::Deleted,
                    entry_mode,
                ),
                gix::object::tree::diff::ChangeDetached::Modification {
                    location,
                    entry_mode,
                    ..
                } => (
                    Some(location.to_string()),
                    Some(location.to_string()),
                    PathChange::Modified,
                    entry_mode,
                ),
                gix::object::tree::diff::ChangeDetached::Rewrite {
                    source_location,
                    location,
                    entry_mode,
                    ..
                } => (
                    Some(source_location.to_string()),
                    Some(location.to_string()),
                    PathChange::Renamed,
                    entry_mode,
                ),
            };

            if mode.is_tree() {
                continue;
            }

            if is_wiki(&old) || is_wiki(&new) {
                continue;
            }
            if old.is_some() || new.is_some() {
                out.push(PathDelta {
                    old,
                    new,
                    change: path_change,
                });
            }
        }
        out.sort_by(|a, b| {
            a.new_path()
                .unwrap_or_default()
                .cmp(b.new_path().unwrap_or_default())
        });
        Ok(out)
    }

    /// Batch pre-load: one pathspec-limited diff per recorded commit, covering
    /// the union of all evidence paths for that commit.
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
        let head_tree = self.head_tree()?;
        let diff = self
            .repo
            .repo
            .diff_tree_to_tree(Some(&old_tree), Some(&head_tree), None)
            .map_err(|e| WikiError::Git(e.to_string()))?;

        let path_set: BTreeSet<&str> = paths.iter().copied().collect();
        for change in diff {
            let mode = change.entry_mode();
            if mode.is_tree() {
                continue;
            }
            let loc = change.location().to_string();
            if path_set.contains(loc.as_str()) {
                changed.insert(loc.clone());
            }
            if let gix::object::tree::diff::ChangeDetached::Rewrite {
                source_location, ..
            } = change
            {
                let src = source_location.to_string();
                if path_set.contains(src.as_str()) {
                    changed.insert(src);
                }
            }
        }
        Ok(changed)
    }

    pub fn is_dirty(&self) -> Result<bool> {
        if let Some(d) = self.dirty.get() {
            return Ok(*d);
        }
        let d = self.repo.is_dirty()?;
        let _ = self.dirty.set(d);
        Ok(d)
    }

    fn tree_of(&self, commit_or_ref: &str) -> Option<gix::Tree<'_>> {
        let sha = self.resolve_ref_commit(commit_or_ref).ok()?;
        let id = gix::ObjectId::from_hex(sha.as_bytes()).ok()?;
        let commit = self
            .repo
            .repo
            .find_object(id)
            .ok()?
            .try_into_commit()
            .ok()?;
        commit.tree().ok()
    }

    /// Full commit sha a ref (sha, branch, tag, `HEAD~n`) resolves to.
    pub fn resolve_ref_commit(&self, rev: &str) -> Result<String> {
        let id = self
            .repo
            .repo
            .rev_parse_single(rev.as_bytes())
            .map_err(|e| WikiError::Git(format!("cannot resolve ref `{rev}`: {e}")))?;
        let commit = id
            .object()
            .map_err(|e| WikiError::Git(e.to_string()))?
            .try_into_commit()
            .map_err(|e| WikiError::Git(format!("{rev} is not a commit-able object: {e}")))?;
        Ok(commit.id().to_string())
    }

    pub fn is_ancestor(&self, maybe_ancestor: &str, descendant: &str) -> Result<bool> {
        let a_sha = self.resolve_ref_commit(maybe_ancestor)?;
        let b_sha = self.resolve_ref_commit(descendant)?;
        if a_sha == b_sha {
            return Ok(true);
        }
        let a_id =
            gix::ObjectId::from_hex(a_sha.as_bytes()).map_err(|e| WikiError::Git(e.to_string()))?;
        let b_id =
            gix::ObjectId::from_hex(b_sha.as_bytes()).map_err(|e| WikiError::Git(e.to_string()))?;
        let walk = self
            .repo
            .repo
            .rev_walk([b_id])
            .all()
            .map_err(|e| WikiError::Git(e.to_string()))?;
        for info in walk {
            let info = info.map_err(|e| WikiError::Git(e.to_string()))?;
            if info.id == a_id {
                return Ok(true);
            }
        }
        Ok(false)
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
            if let Ok(head_tree) = self.head_tree() {
                if let Ok(diff) =
                    self.repo
                        .repo
                        .diff_tree_to_tree(Some(&old_tree), Some(&head_tree), None)
                {
                    for change in diff {
                        if let gix::object::tree::diff::ChangeDetached::Rewrite {
                            source_location,
                            location,
                            ..
                        } = change
                        {
                            out.insert(source_location.to_string(), location.to_string());
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

    /// Line map from the recorded blob to `new_path` at HEAD.
    /// Drift projection uses this instead of a full-file diff per citation.
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
        if old_content.as_bytes().contains(&0) || new_content.as_bytes().contains(&0) {
            return Ok(None);
        }
        Ok(Some(LineMap::compute(&old_content, &new_content)))
    }

    pub fn commit_exists(&self, sha: &str) -> bool {
        if let Some(cached) = self.commit_exists.borrow().get(sha) {
            return *cached;
        }
        let exists = match gix::ObjectId::from_hex(sha.as_bytes()) {
            Ok(oid) => match self.repo.repo.find_object(oid) {
                Ok(obj) => obj.kind == gix::object::Kind::Commit,
                Err(_) => false,
            },
            Err(_) => false,
        };
        self.commit_exists
            .borrow_mut()
            .insert(sha.to_string(), exists);
        exists
    }
}

/// Precomputed line-to-line mapping between two revisions of a file.
pub struct LineMap {
    /// map[old_line_0based] = new_line_0based
    map: Vec<u32>,
    new_len: usize,
}

impl LineMap {
    pub fn compute(old_content: &str, new_content: &str) -> Self {
        let old_lines: Vec<&str> = old_content.split('\n').collect();
        let new_lines: Vec<&str> = new_content.split('\n').collect();
        let old_len = old_lines.len();
        let new_len = new_lines.len();
        let mut map = vec![0u32; old_len];
        let (mut oi, mut ni) = (0usize, 0usize);
        let diff = TextDiff::from_lines(old_content, new_content);
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Equal => {
                    if oi < old_len {
                        map[oi] = ni as u32;
                    }
                    oi += 1;
                    ni += 1;
                }
                ChangeTag::Delete => {
                    if oi < old_len {
                        map[oi] = ni.saturating_sub(1) as u32;
                    }
                    oi += 1;
                }
                ChangeTag::Insert => {
                    ni += 1;
                }
            }
        }
        while oi < old_len {
            map[oi] = ni as u32;
            oi += 1;
            ni += 1;
        }
        let max_new = (new_len.saturating_sub(1)) as u32;
        for slot in &mut map {
            *slot = (*slot).min(max_new);
        }
        LineMap { map, new_len }
    }

    /// Map a 0-based range from old lines onto the new lines.
    pub fn project(&self, lo0: usize, hi0: usize) -> (usize, usize) {
        if self.map.is_empty() {
            return (lo0, hi0);
        }
        let o_lo = lo0.min(self.map.len() - 1);
        let o_hi = hi0.min(self.map.len() - 1);
        let mut n_lo = self.map[o_lo] as usize;
        let mut n_hi = self.map[o_hi] as usize;
        if n_hi < n_lo {
            std::mem::swap(&mut n_lo, &mut n_hi);
        }
        if n_lo == n_hi && hi0 > lo0 {
            n_hi = (n_lo + (hi0 - lo0)).min(self.new_len.saturating_sub(1));
        }
        (n_lo, n_hi)
    }
}

fn lines_of(content: &str, lo: u32, hi: u32) -> Option<String> {
    if lo == 0 || hi < lo {
        return None;
    }
    let mut lines = Vec::new();
    for (idx, line) in content.split('\n').enumerate() {
        let line_no = (idx + 1) as u32;
        if line_no >= lo && line_no <= hi {
            lines.push(line);
        }
        if line_no > hi {
            break;
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_requested_window() {
        let text = "line 1\nline 2\nline 3\nline 4\nline 5";
        assert_eq!(lines_of(text, 1, 1), Some("line 1".into()));
        assert_eq!(lines_of(text, 2, 4), Some("line 2\nline 3\nline 4".into()));
        assert_eq!(lines_of(text, 5, 5), Some("line 5".into()));
        assert_eq!(lines_of(text, 0, 1), None);
        assert_eq!(lines_of(text, 4, 2), None);
        assert_eq!(lines_of(text, 6, 8), None);
    }

    #[test]
    fn keeps_split_newline_semantics_at_edges() {
        let text = "a\nb\n";
        assert_eq!(lines_of(text, 1, 2), Some("a\nb".into()));
        assert_eq!(lines_of(text, 3, 3), Some("".into()));
    }

    #[test]
    fn path_deltas_classifies_modify_rename_delete() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path();

        let sig = git2::Signature::now("t", "t@l").unwrap();
        let g2 = git2::Repository::init(path).unwrap();

        std::fs::write(path.join("keep.txt"), "v1\n").unwrap();
        std::fs::write(path.join("modify.txt"), "v1\n").unwrap();
        std::fs::write(path.join("delete.txt"), "v1\n").unwrap();
        std::fs::write(path.join("mv_clean.txt"), "identical body\n").unwrap();
        std::fs::write(
            path.join("mv_edit.txt"),
            "long body line 1\nline 2\nline 3\nline 4\n",
        )
        .unwrap();
        let mut idx = g2.index().unwrap();
        idx.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        let t1 = idx.write_tree().unwrap();
        let c1 = g2
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                "c1",
                &g2.find_tree(t1).unwrap(),
                &[],
            )
            .unwrap();

        std::fs::write(path.join("modify.txt"), "v2 modified\n").unwrap();
        std::fs::remove_file(path.join("delete.txt")).unwrap();
        std::fs::rename(path.join("mv_clean.txt"), path.join("renamed_clean.txt")).unwrap();
        std::fs::write(
            path.join("renamed_edit.txt"),
            "long body line 1\nline 2 edited\nline 3\nline 4\n",
        )
        .unwrap();
        std::fs::remove_file(path.join("mv_edit.txt")).unwrap();
        std::fs::write(path.join("added.txt"), "brand new\n").unwrap();
        let mut idx = g2.index().unwrap();
        idx.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        let t2 = idx.write_tree().unwrap();
        let c1_obj = g2.find_commit(c1).unwrap();
        let _c2 = g2
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                "c2",
                &g2.find_tree(t2).unwrap(),
                &[&c1_obj],
            )
            .unwrap();

        let lrepo = GitRepo::discover(path).unwrap();
        let deltas = lrepo.path_deltas(&c1.to_string()).unwrap();

        let by_new: HashMap<String, &PathDelta> = deltas
            .iter()
            .filter_map(|d| d.new.as_ref().map(|n| (n.clone(), d)))
            .collect();

        assert_eq!(
            by_new.get("modify.txt").unwrap().change,
            PathChange::Modified
        );
        assert_eq!(by_new.get("added.txt").unwrap().change, PathChange::Added);
        assert!(deltas
            .iter()
            .any(|d| d.old.as_deref() == Some("delete.txt") && d.change == PathChange::Deleted));
        assert_eq!(
            by_new.get("renamed_clean.txt").unwrap().change,
            PathChange::Renamed
        );
        assert_eq!(
            by_new.get("renamed_clean.txt").unwrap().old.as_deref(),
            Some("mv_clean.txt")
        );
    }
}
