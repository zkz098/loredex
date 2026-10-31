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

use crate::config::Locale;

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
    pub fn instructions_for(&self, subject: &str, locale: Locale) -> String {
        match locale {
            Locale::En => format!(
                "Write page `{subject}` of type `{}`. Sections: {}. Answer concretely, each claim cited: {}. Close with an inline [[cite:path:Lx-Ly]] for every statement.",
                self.name,
                self.sections.join(" -> "),
                self.checklist.join(" ")
            ),
            Locale::Zh => format!(
                "为 `{subject}` 撰写类型为 `{}` 的页面。章节:{}。逐条具体回答并配引用:{}。每个论断后面都要有行内 [[cite:path:Lx-Ly]]。",
                self.name,
                self.sections.join(" -> "),
                self.checklist.join(" ")
            ),
        }
    }
}

fn builtin(locale: Locale) -> Vec<TypePlaybook> {
    match locale {
        Locale::En => builtin_en(),
        Locale::Zh => builtin_zh(),
    }
}

fn builtin_en() -> Vec<TypePlaybook> {
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

fn builtin_zh() -> Vec<TypePlaybook> {
    [
        TypePlaybook {
            name: "architecture".into(),
            description: "系统如何划分以及为什么这样划分".into(),
            sections: vec![
                "系统目标".into(),
                "模块地图与分层".into(),
                "数据流".into(),
                "故障域".into(),
                "关键不变量".into(),
            ],
            checklist: vec![
                "顶层单元有哪些?各自负责什么?".into(),
                "依赖允许朝哪个方向?".into(),
                "X 失败时最先崩的是什么?".into(),
            ],
        },
        TypePlaybook {
            name: "workflow".into(),
            description: "一项用户可见能力的端到端路径".into(),
            sections: vec![
                "触发点".into(),
                "步骤序列".into(),
                "状态迁移".into(),
                "错误处理".into(),
                "可观测性".into(),
            ],
            checklist: vec![
                "追踪一个具体请求/任务从入口到完成.".into(),
                "可能在哪失败?用户当时看到什么?".into(),
            ],
        },
        TypePlaybook {
            name: "system".into(),
            description: "单个组件/crate 的运行时机制".into(),
            sections: vec![
                "职责".into(),
                "公开接口".into(),
                "内部机制".into(),
                "配置".into(),
                "资源与生命周期".into(),
            ],
            checklist: vec![
                "这个组件向谁暴露了什么?".into(),
                "它持有什么状态?持久化在哪里?".into(),
                "有哪些开关?默认值是什么?".into(),
            ],
        },
        TypePlaybook {
            name: "integration".into(),
            description: "与外部系统或协议的边界".into(),
            sections: vec![
                "对接方与契约".into(),
                "适配层布局".into(),
                "兼容性与版本".into(),
                "错误映射".into(),
            ],
            checklist: vec![
                "实现的是哪个外部契约?在哪里有偏差?".into(),
                "外部错误如何映射为本地错误?".into(),
            ],
        },
        TypePlaybook {
            name: "testing".into(),
            description: "如何断言正确性".into(),
            sections: vec![
                "测试分层".into(),
                "如何运行".into(),
                "夹具与测试架".into(),
                "值得注意的回归".into(),
            ],
            checklist: vec![
                "哪条命令验证哪种风险?".into(),
                "测试套件编码了哪些回归?".into(),
            ],
        },
    ]
    .into()
}

/// Built-ins (locale follows `[wiki] language` in `.wiki/wiki.toml`) merged
/// with repo-local `[page_types.<name>]` overrides from the same file
/// (same name: toml wins field-wise).
pub fn registry(wiki_dir: &Path) -> BTreeMap<String, TypePlaybook> {
    let locale = crate::config::locale(wiki_dir);
    let mut map: BTreeMap<String, TypePlaybook> = builtin(locale)
        .into_iter()
        .map(|p| (p.name.clone(), p))
        .collect();
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
