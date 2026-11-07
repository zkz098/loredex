use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

use loredex_git::{GitRepo, GitView};
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub name: String,
    pub kind: String,
    pub files: Vec<String>,
}

/// Discover generation units for a coding agent: Rust targets via
/// `cargo metadata`, everything else grouped by directory.
pub fn discover(repo: &GitRepo) -> Result<Vec<ModuleInfo>> {
    discover_with(&repo.view())
}

pub fn discover_with(view: &GitView) -> Result<Vec<ModuleInfo>> {
    let files: Vec<String> = view.tracked_files()?;
    let mut modules = Vec::new();
    let mut covered: BTreeMap<String, ()> = BTreeMap::new();

    if let Some(meta) = cargo_metadata(view.workdir(), &view.wiki_dir()) {
        // (package, target, kind, source dir, own source file)
        let mut targets: Vec<(String, String, String, String, Option<String>)> = Vec::new();
        for pkg in meta
            .get("packages")
            .and_then(|p| p.as_array())
            .into_iter()
            .flatten()
        {
            let pkg_name = pkg.get("name").and_then(|n| n.as_str()).unwrap_or("?");
            for target in pkg
                .get("targets")
                .and_then(|t| t.as_array())
                .into_iter()
                .flatten()
            {
                let Some(src) = target.get("src_path").and_then(|s| s.as_str()) else {
                    continue;
                };
                let Some(kinds) = target.get("kind").and_then(|k| k.as_array()) else {
                    continue;
                };
                let kind = kinds
                    .iter()
                    .filter_map(|k| k.as_str())
                    .find(|k| matches!(*k, "lib" | "bin" | "test" | "bench"))
                    .unwrap_or("lib")
                    .to_string();
                let name = target
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or(pkg_name);
                let rel = match Path::new(src).strip_prefix(view.workdir()) {
                    Ok(p) => p.to_string_lossy().replace('\\', "/"),
                    Err(_) => continue,
                };
                let src_dir = match Path::new(&rel).parent() {
                    Some(p) => format!("{}{}", p.display(), std::path::MAIN_SEPARATOR)
                        .trim_end()
                        .to_string(),
                    None => continue,
                };
                targets.push((
                    pkg_name.to_string(),
                    name.to_string(),
                    kind,
                    src_dir,
                    Some(rel).filter(|r| files.iter().any(|f| f == r)),
                ));
            }
        }

        // A lib target *is* its source directory: cargo gives one entry point and
        // the module tree hangs off it. A test, bench or bin target is one
        // self-contained file whose siblings are separate targets — claiming the
        // whole directory made every test file a duplicate module over the same
        // files, and kept a group emitting plan tasks no page could satisfy.
        let owned: BTreeSet<String> = targets
            .iter()
            .filter(|(_, _, kind, _, _)| matches!(kind.as_str(), "test" | "bench" | "bin"))
            .filter_map(|(_, _, _, _, own)| own.clone())
            .collect();

        for (pkg_name, name, kind, src_dir, own) in targets {
            let module_files: Vec<String> = match (kind.as_str(), own) {
                ("test" | "bench" | "bin", Some(file)) => vec![file],
                ("test" | "bench" | "bin", None) => continue,
                _ => files
                    .iter()
                    .filter(|f| f.starts_with(&src_dir) && !owned.contains(*f))
                    .cloned()
                    .collect(),
            };
            if module_files.is_empty() {
                continue;
            }
            for f in &module_files {
                covered.insert(f.clone(), ());
            }
            modules.push(ModuleInfo {
                name: format!("{pkg_name}/{name}"),
                kind: format!("rust-{kind}"),
                files: module_files,
            });
        }
    }

    // leftover (or full fallback for non-Rust repos): group by two directory levels
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in &files {
        if covered.contains_key(f) {
            continue;
        }
        let parts: Vec<&str> = f.split('/').collect();
        let group = if parts.len() > 2 {
            format!("{}/{}", parts[0], parts[1])
        } else if parts.len() > 1 {
            parts[0].to_string()
        } else {
            ".".to_string()
        };
        groups.entry(group).or_default().push(f.clone());
    }
    for (group, mut gf) in groups {
        gf.sort();
        modules.push(ModuleInfo {
            name: group,
            kind: "dir".into(),
            files: gf,
        });
    }
    Ok(modules)
}

const CACHE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct CargoMetaCache {
    key: String,
    metadata: serde_json::Value,
}

#[derive(Clone)]
struct ManifestStat {
    rel: String,
    len: u64,
    mtime_ns: u128,
}

/// `cargo metadata` is a subprocess and a fixed cost on every plan/auto.
/// Cache it under `.wiki/.cache` keyed by the on-disk manifest set (paths,
/// lengths, mtimes) and the CARGO binary. Any edited/new/deleted manifest
/// changes the key; unrelated source commits do not. A failed cargo run is
/// never cached.
fn cargo_metadata(workdir: &Path, wiki_dir: &Path) -> Option<serde_json::Value> {
    let manifests = collect_manifests(workdir);
    let has_root_manifest = manifests
        .as_ref()
        .map(|m| m.iter().any(|entry| entry.rel == "Cargo.toml"))
        .unwrap_or_else(|| workdir.join("Cargo.toml").is_file());
    if !has_root_manifest {
        return None; // cargo metadata from this workdir would not resolve a workspace
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let key = manifests.as_ref().map(|manifests| {
        let mut key_material = format!("v{CACHE_VERSION}\0{cargo}\0");
        for m in manifests {
            key_material.push_str(&format!("{}\0{}\0{}\0", m.rel, m.len, m.mtime_ns));
        }
        crate::sha1_hex(key_material.as_bytes())
    });

    // `.wiki` may not exist yet (plain `loredex modules` before init); only
    // then do we create cache files, never a whole wiki layout as a side effect.
    let cache_path = wiki_dir.join(".cache").join("cargo-metadata.json");
    if wiki_dir.is_dir() {
        if let Some(key) = key.as_deref() {
            if let Some(meta) = read_cache(&cache_path, key) {
                return Some(meta);
            }
        }
    }

    let out = Command::new(&cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(workdir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let metadata: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    if wiki_dir.is_dir() {
        if let Some(key) = key.as_deref() {
            write_cache(&cache_path, key, &metadata);
        }
    }
    Some(metadata)
}

fn collect_manifests(workdir: &Path) -> Option<Vec<ManifestStat>> {
    let mut out = Vec::new();
    let mut stack = vec![workdir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let file_type = entry.file_type().ok()?;
            let name = entry.file_name();
            if file_type.is_dir() {
                // Skip VCS/dependency trees; their manifests do not define
                // workspace members and can be enormous.
                if name != ".git" && name != "target" && name != "node_modules" {
                    stack.push(entry.path());
                }
            } else if name == "Cargo.toml" || name == "Cargo.lock" {
                let meta = entry.metadata().ok()?;
                let rel = entry
                    .path()
                    .strip_prefix(workdir)
                    .ok()?
                    .to_string_lossy()
                    .replace('\\', "/");
                let mtime_ns = meta
                    .modified()
                    .ok()?
                    .duration_since(UNIX_EPOCH)
                    .ok()?
                    .as_nanos();
                out.push(ManifestStat {
                    rel,
                    len: meta.len(),
                    mtime_ns,
                });
            }
        }
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Some(out)
}

fn read_cache(path: &Path, key: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    let cache: CargoMetaCache = serde_json::from_str(&text).ok()?;
    (cache.key == key).then_some(cache.metadata)
}

fn write_cache(path: &Path, key: &str, metadata: &serde_json::Value) {
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let cache = CargoMetaCache {
        key: key.to_string(),
        metadata: metadata.clone(),
    };
    let Ok(text) = serde_json::to_vec(&cache) else {
        return;
    };
    let tmp = PathBuf::from(format!("{}.{}.tmp", path.display(), std::process::id()));
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::collect_manifests;

    #[test]
    fn manifest_walk_skips_vcs_and_target_dirs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("target/debug")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("member")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(root.join("member/Cargo.toml"), "[package]\n").unwrap();
        std::fs::write(root.join("target/debug/Cargo.toml"), "ignored\n").unwrap();
        std::fs::write(root.join(".git/Cargo.toml"), "ignored\n").unwrap();

        let manifests = collect_manifests(root).unwrap();
        let paths: Vec<&str> = manifests.iter().map(|m| m.rel.as_str()).collect();
        assert_eq!(paths, vec!["Cargo.toml", "member/Cargo.toml"]);
    }
}
