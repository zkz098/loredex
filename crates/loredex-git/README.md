# loredex-git

Read-only [libgit2](https://github.com/libgit2/libgit2) (git2) evidence layer for [Loredex](https://github.com/zkz098/loredex), an evidence-backed repository wiki for AI coding agents.

Single type, `GitRepo`, exposing exactly the git questions the wiki needs to decide whether a claim about the code is still true:

- **HEAD content** — `head_blob_oid(path)` (content hash of a file at HEAD), `head_file_content`, `head_lines(path, lo, hi)` for re-reading cited line ranges, `head_commit` / `head_short` / `commit_exists`.
- **Change detection** — `changed_paths_since(commit, paths)` diffs a pathspec range against HEAD; cross-checking this *with* blob comparison is what makes Loredex's staleness verdict independent of trusting commit ids.
- **Rename recovery** — `path_of_blob_in_head(oid)` finds a moved file by its identical blob oid (a plain `git mv` preserves the blob, so content addressing is more precise than rename heuristics); `renames_since(from)` adds git similarity detection for rename + small-edit cases.
- **Direct object access** — `blob_content(oid)` reads any object from the odb, used to fetch the old snippet content when projecting line drift.
- **Housekeeping** — `discover`, `workdir`, `wiki_dir` (`<workdir>/.wiki`), `tracked_files`, `is_dirty`.

```rust
let repo = loredex_git::GitRepo::discover(".")?;
let oid = repo.head_blob_oid("src/main.rs")?;      // pins the evidence
let lines = repo.head_lines("src/main.rs", 3, 6)?; // the cited snippet at HEAD
```

The wrapper never writes to git. Errors are `loredex_core::WikiError::Git`. License: GPL-3.0-or-later. See the [workspace README](https://github.com/zkz098/loredex) for the full design.
