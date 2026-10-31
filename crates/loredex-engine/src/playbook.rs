//! Page-type playbooks: the generation guidance openwiki bakes into its
//! closed-source prompts, exposed here as data — built-in types plus
//! repo-local extensions in `.wiki/wiki.toml [page_types.<name>]`.
//!
//! Plans carry `suggested_type` + `instructions` derived from these, so any
//! producer agent writes structurally consistent pages without a hidden
//! prompt pipeline.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypePlaybook {
    pub name: String,
    pub description: String,
    /// sections the page should carry, in order
    pub sections: Vec<String>,
    /// questions the page must answer concretely (each with citations)
    pub checklist: Vec<String>,
}

impl TypePlaybook {
    /// Authoring instructions embedded into plan tasks.
    pub fn instructions_for(&self, subject: &str) -> String {
        format!(
            "Write page `{subject}` of type `{}`. Sections: {}. Answer concretely, each claim cited: {}. Close with an inline [[cite:path:Lx-Ly]] for every statement.",
            self.name,
            self.sections.join(" -> "),
            self.checklist.join(" ")
        )
    }
}

fn builtin() -> Vec<TypePlaybook> {
    [
        TypePlaybook {
            name: "architecture".into(),
            description: "How the system is partitioned and why".into(),
            sections: vec![
                "system purpose".into(),
                "module map & layering".into(),
                "data flow".into(),
                "failure domains".into(),
                "key invariants".into(),
            ],
            checklist: vec![
                "What are the top-level units and what does each own?".into(),
                "Which dependencies are allowed in which direction?".into(),
                "What breaks first when X fails?".into(),
            ],
        },
        TypePlaybook {
            name: "workflow".into(),
            description: "End-to-end path of one user-visible capability".into(),
            sections: vec![
                "trigger".into(),
                "step sequence".into(),
                "state transitions".into(),
                "error handling".into(),
                "observability".into(),
            ],
            checklist: vec![
                "Trace one concrete request/task from entry to completion.".into(),
                "Where can it fail and what does the user see then?".into(),
            ],
        },
        TypePlaybook {
            name: "system".into(),
            description: "Runtime mechanics of one component/crate".into(),
            sections: vec![
                "responsibility".into(),
                "public surface".into(),
                "internal mechanics".into(),
                "configuration".into(),
                "resources & lifecycle".into(),
            ],
            checklist: vec![
                "What does this component expose and to whom?".into(),
                "What state does it own and where is it persisted?".into(),
                "What knobs exist and what are their defaults?".into(),
            ],
        },
        TypePlaybook {
            name: "integration".into(),
            description: "Boundary with an external system or protocol".into(),
            sections: vec![
                "counterpart & contract".into(),
                "adapter layout".into(),
                "compatibility & versioning".into(),
                "failure mapping".into(),
            ],
            checklist: vec![
                "Which external contract is implemented, and where does it deviate?".into(),
                "How are foreign errors mapped to local ones?".into(),
            ],
        },
        TypePlaybook {
            name: "testing".into(),
            description: "How correctness is asserted".into(),
            sections: vec![
                "test layers".into(),
                "how to run".into(),
                "fixtures & harness".into(),
                "notable regressions".into(),
            ],
            checklist: vec![
                "Which command validates which risk?".into(),
                "What regressions does the suite encode?".into(),
            ],
        },
    ]
    .into()
}

/// Built-ins merged with repo-local `[page_types.<name>]` overrides from
/// `.wiki/wiki.toml` (same name: toml wins field-wise).
pub fn registry(wiki_dir: &Path) -> BTreeMap<String, TypePlaybook> {
    let mut map: BTreeMap<String, TypePlaybook> =
        builtin().into_iter().map(|p| (p.name.clone(), p)).collect();
    let text = match std::fs::read_to_string(wiki_dir.join("wiki.toml")) {
        Ok(t) => t,
        Err(_) => return map,
    };
    #[derive(Deserialize)]
    struct Cfg {
        #[serde(default)]
        page_types: BTreeMap<String, Partial>,
    }
    #[derive(Deserialize, Default)]
    struct Partial {
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        sections: Option<Vec<String>>,
        #[serde(default)]
        checklist: Option<Vec<String>>,
    }
    let cfg: Cfg = match toml::from_str(&text) {
        Ok(c) => c,
        Err(_) => return map, // tolerate: config is optional decoration
    };
    for (name, part) in cfg.page_types {
        let entry = map.entry(name.clone()).or_insert_with(|| TypePlaybook {
            name: name.clone(),
            description: String::new(),
            sections: vec![],
            checklist: vec![],
        });
        if let Some(d) = part.description {
            entry.description = d;
        }
        if let Some(s) = part.sections {
            entry.sections = s;
        }
        if let Some(c) = part.checklist {
            entry.checklist = c;
        }
    }
    map
}

/// Type suggestion for a not-yet-covered module: the repo-root grouping is a
/// structural overview candidate; everything else documents one component.
pub fn suggest_for_module(module_name: &str) -> &'static str {
    if module_name == "." || module_name == "docs" {
        "architecture"
    } else {
        "system"
    }
}
