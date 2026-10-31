# repowiki

给 AI coding agent 提供**可信的仓库 wiki**：纯 Markdown 存储、git 驱动的新鲜度追踪、每条声明钉住可验证的代码出处。Rust 实现，双入口（CLI + MCP server）。

## 为什么

Agent 读仓库的痛点：上下文贵、过期文档误导。repowiki 的答案是：

- **Evidence 钉定** — 页面正文里的每个声明都携带 `[[cite:path:Lx-Ly]]` 标记，落库时解析为 `(路径, 行区间, git blob sha, 片段内容 sha1)` 四元组。没有引用的内容拒绝入库。
- **Git 自动追踪过期** — 不信任 commit 号本身：对比 HEAD blob、用 tree diff 交叉验证、校验时直接重算行区间内容哈希。代码一改，相关页面即判 stale。
- **行号漂移投影** — 源文件增删行后，旧引用通过行级 diff 自动映射到新位置（相似度达标就采纳并在 note 留痕），页面保持 verified；确实丢失的降级 draft 强制重写。
- **零数据库** — frontmatter 就是数据库。`.wiki/pages/*.md` 随 git 提交、可 review、可手改，不存在可损坏的权威索引；搜索索引每次从文件毫秒级重建。

## 安装

```bash
cargo build --release
# 两个二进制：
#   target/release/repowiki        CLI
#   target/release/repowiki-mcp    MCP stdio server
```

## 快速上手（CLI）

```bash
cd /path/to/your-repo          # 需至少有一个 commit
repowiki init                  # 创建 .wiki/，gitignore .wiki/.cache/

repowiki modules               # 模块发现（cargo targets / 目录分组）
repowiki put core --title "核心层" --body notes.md
repowiki list / show core / search "关键词"
repowiki check --mark          # 过期扫描，exit code 非 0 = 有 stale（可挂 CI）
repowiki verify core           # 内容哈希级校验（不依赖 commit 号）
repowiki refresh core          # 漂移投影刷新
```

`--body -` 从 stdin 读取。页面正文示例：

```markdown
serve() 在循环中处理请求 [[cite:src/api.rs:L1-L5]]，main 先初始化 db [[cite:src/main.rs:L3-L6]]。
```

## Agent 接入（MCP）

**零配置接入（推荐）**：一份全局配置即可服务任意仓库，server 按四层解析链自动定位目标仓库——`--repo` 参数（强制钉定）→ `REPOWIKI_REPO` 环境变量 → 客户端 MCP roots → 进程工作目录（主流客户端都在 workspace 目录 spawn server）：

```json
{
  "mcpServers": {
    "repowiki": { "command": "repowiki-mcp" }
  }
}
```

解析全部失败时工具返回带指引的错误，agent 会调用 `wiki_attach_repo` 显式挂载；`wiki_current_repo` 可随时自省当前仓库及其解析来源。需要钉死单一仓库时：

```json
{
  "mcpServers": {
    "repowiki": {
      "command": "repowiki-mcp",
      "args": ["--repo", "/path/to/your-repo"]
    }
  }
}
```

Server 在 `initialize` 时下发协议契约（先查新鲜度、stale 视为未验证、漂移走 refresh、引用可审计）与解析链说明。所有工具返回结构化 `structuredContent`（`stale_now`、`matches_head`、`projectable` 等字段）。

| 工具 | 用途 |
|---|---|
| `wiki_check_freshness` | 全量 git 过期扫描 + 标注可投影项 |
| `wiki_list` / `wiki_read` | 页面清单 / 读页面（stale 带醒目 warning） |
| `wiki_search` | 分词搜索（驼峰、下划线、CJK 二元组、前缀） |
| `wiki_get_evidence` | 把单条引用展开为 HEAD 当前源码内容 |
| `wiki_list_modules` | 发现待编写模块，含已有引用覆盖统计 |
| `wiki_put_page` | 提交 agent 撰写的内容（零引用拒绝） |
| `wiki_refresh_page` | 漂移投影刷新，逐条返回投影结果 |
| `wiki_attach_repo` / `wiki_current_repo` | 挂载/切换仓库、自省解析状态 |

## 工作流

```
模块发现 → 读源码 → put_page(带引用) → [代码演进] → check/刷新投影
                                                ├─ blob 未变  → verified 保持
                                                ├─ 行号漂移   → 自动投影，verified + note
                                                └─ 内容丢失   → draft，agent 重写
```

- 页面与 `.wiki/` 一起提交进 git，团队共享、PR 可 review
- `repowiki check` 适合挂 CI 或 `post-merge` hook

## 架构

```
crates/
├── wiki-core     数据模型（Status/Evidence/PageMeta）、Markdown 解析、
│                 引用语法解析器、分词 + 倒排索引（无 IO）
├── wiki-git      git2 只读封装：HEAD blob、行区间、tree diff、对象库直读
├── wiki-engine   业务层：check_page（双重过期判定）、verify_page（sha1 校验）、
│                 drift 投影（similar 行级 diff）、put_page、modules 发现
├── wiki-cli      clap 入口（repowiki）
└── wiki-mcp      rmcp 3.8 stdio server（repowiki-mcp）
```

关键实现细节：

- 过期判定 = blob 对比 **和** `生成commit→HEAD` 的 pathspec diff 交叉验证；生成 commit 不可解析也判 stale
- 漂移投影对旧行→新行建映射（删除行锚定最近存活行、区间坍缩保持宽度），投影后新旧 snippet 相似度 ≥ 0.6 才采纳
- 引用校验在入库时进行：路径必须在 HEAD 存在，行区间非法直接拒绝
- MCP 仓库解析四层链（cli > env > roots > cwd），roots 层带客户端能力门控与 3 秒超时，任何失配客户端都不会卡死会话
- `git2::Repository` 是 `!Sync`，repo 实例存于 `Mutex<RepoSlot>` 内，异步解析只"确保"槽位、工具同步借用（单层的锁纪律）

## 已知边界

- 内容质量取决于撰写者（工具保证"有出处且出处未腐化"，不保证解释正确）
- check/verify/search 全量加载，千级页面良好，万级需持久索引（未做）
- 正文内联引用标记在 refresh 投影后不同步行号（frontmatter 为权威）
- 非 ASCII 路径建议仓库开启 `core.quotePath=false`

## Roadmap

- M4：按 commit range 增量重建受影响页面、`post-commit` hook 集成
- 正文内联标记同步重写
- `.wiki/.cache/` 持久搜索索引（超大 wiki 场景）

## 开发

```bash
cargo nextest run --workspace          # 30 个测试（单测 + git-fixture 集成 + MCP 协议内测试）
cargo llvm-cov nextest --workspace --summary-only   # 覆盖率基线
```

- 测试栈：cargo-nextest（每测试独立进程，env 操纵安全；`.config/nextest.toml` 配 15s slow-timeout，死锁会失败而非挂起）+ cargo-llvm-cov
- 当前覆盖：总计 ~75% 区域 / 79% 行；核心层（core/engine/git/modules）80-98%，`wiki-mcp` 77%（进程内 duplex client↔server 走真实协议栈验证四层解析链、roots 能力门控、全生命周期），CLI 入口为薄参数映射未单测
- wiki-mcp 拆为 lib+bin：集成测试直接构造 `WikiServer::new_open()/pinned()`
