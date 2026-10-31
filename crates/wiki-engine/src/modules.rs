use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use wiki_git::GitRepo;

use crate::Result;

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub kind: String,
    pub files: Vec<String>,
}

/// Discover generation units for a coding agent: Rust targets via
/// `cargo metadata`, everything else grouped by directory.
pub fn discover(repo: &GitRepo) -> Result<Vec<ModuleInfo>> {
    let files: Vec<String> = repo.tracked_files()?;
    let mut modules = Vec::new();
    let mut covered: BTreeMap<String, ()> = BTreeMap::new();

    if let Some(meta) = cargo_metadata(repo.workdir()) {
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
                let name = target.get("name").and_then(|n| n.as_str()).unwrap_or(pkg_name);
                let src_dir = match Path::new(src)
                    .parent()
                    .and_then(|p| p.strip_prefix(repo.workdir()).ok())
                {
                    Some(rel) => format!("{}{}", rel.display(), std::path::MAIN_SEPARATOR).trim_end().to_string(),
                    None => continue,
                };
                let module_files: Vec<String> = files
                    .iter()
                    .filter(|f| f.starts_with(&src_dir))
                    .cloned()
                    .collect();
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

fn cargo_metadata(workdir: &Path) -> Option<serde_json::Value> {
    let out = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(workdir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}
