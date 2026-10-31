# Loredex

给 AI coding agent 提供**可信的仓库 wiki**：纯 Markdown 存储、git 驱动的新鲜度追踪、每条声明钉住可验证的代码出处。Rust 实现，双入口（CLI + MCP server）。

## 为什么

Agent 读仓库的痛点：上下文贵、过期文档误导。Loredex 的答案是：

- **Evidence 钉定** — 页面正文里的每个声明都携带 `[[cite:path:Lx-Ly]]` 标记，落库时解析为 `(路径, 行区间, git blob sha, 片段内容 sha1)` 四元组。没有引用的内容拒绝入库。
- **Git 自动追踪过期** — 不信任 commit 号本身：对比 HEAD blob、用 tree diff 交叉验证、校验时直接重算行区间内容哈希。代码一改，相关页面即判 stale。
- **行号漂移投影** — 源文件增删行后，旧引用通过行级 diff 自动映射到新位置（相似度达标就采纳并在 note 留痕），页面保持 verified；确实丢失的降级 draft 强制重写
- **重命名自动恢复** — `git mv` 不改内容时按 blob oid 在 HEAD 反查新路径（内容寻址，比重命名启发式更精确）；mv+小改走 git 相似度改名检测 + 行投影；两步都失败的才要求人工重写。
- **零数据库** — frontmatter 就是数据库。`.wiki/pages/*.md` 随 git 提交、可 review、可手改，不存在可损坏的权威索引；搜索索引每次从文件毫秒级重建。

## 安装

**从源码（cargo）**，需要 Rust 1.85+（git2 会 vendored 编译 libgit2，几分钟）：

```bash
cargo install loredex-cli loredex-mcp   # 发布到 crates.io 后
# 或本地：
cargo build --release
# 二进制：target/release/loredex（CLI）、target/release/loredex-mcp（MCP server）
```

**预编译二进制**：见 GitHub Releases（`loredex-<os>-<arch>` 归档 + 安装脚本；发布工程由 cargo-dist 承担，配置随仓库提供）。发布后可用 `cargo binstall loredex-cli` 秒装。

## 快速上手（CLI）

```bash
cd /path/to/your-repo          # 需至少有一个 commit
loredex init                  # 创建 .wiki/，gitignore .wiki/.cache/

loredex modules               # 模块发现（cargo targets / 目录分组）
loredex put core --title "核心层" --body notes.md
loredex list / show core / search "关键词"
loredex check --mark          # 过期扫描，exit code 非 0 = 有 stale（可挂 CI）
loredex plan [--json]         # 生成计划：未覆盖模块(New) + stale页(Refresh)，写入 .wiki/runs.json 账本
loredex index                 # 重新生成 pages/index.md + pages/quickstart.md（按类型读序 + agent 协议）
loredex playbooks             # 页面类型写作指南（内置5类 + wiki.toml 扩展）
loredex lint                  # 断链 / stale-but-verified / 正文-frontmatter 漂移 / 未知 type，有问题 exit 1
loredex auto [--json]         # 一条命令维护：投影自愈 -> lint -> 剩余工作开 plan（exit 1 = 有撰写工作）
loredex task <id> skipped     # 手工销账 plan 任务（skip 会被记住，不再复发；plan --reset 重新暴露）
loredex templates --out contrib   # 导出 GitHub Action 模板 + post-merge hook 模板
loredex verify core           # 内容哈希级校验（不依赖 commit 号）
loredex refresh core          # 漂移投影刷新
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
    "loredex": { "command": "loredex-mcp" }
  }
}
```

解析全部失败时工具返回带指引的错误，agent 会调用 `wiki_attach_repo` 显式挂载；`wiki_current_repo` 可随时自省当前仓库及其解析来源。需要钉死单一仓库时：

```json
{
  "mcpServers": {
    "loredex": {
      "command": "loredex-mcp",
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
| `wiki_plan` / `wiki_mark_task` | 打开/续传生成计划 run、销账（put/refresh 自动销账） |
| `wiki_playbooks` | 页面类型写作指南（plan 任务的 `instructions` 即由此推导） |
| `wiki_auto` | 维护编排：零 token 投影自愈 + lint + 剩余工作的 plan |

## 工作流

```
模块发现 → 读源码 → put_page(带引用) → [代码演进] → check/刷新投影
                                                ├─ blob 未变    → verified 保持
                                                ├─ 行号漂移     → 自动投影，verified + note
                                                ├─ git mv       → 内容寻址反查新路径，verified
                                                ├─ mv + 小改    → 相似度改名 + 投影，verified
                                                └─ 内容丢失     → draft，agent 重写
```

- 页面与 `.wiki/` 一起提交进 git，团队共享、PR 可 review；`runs.json` 账本同样入库，agent 中断后 `wiki_plan` 自动续传 pending 任务
- plan 任务自带 `suggested_type` 与 playbook `instructions`（该类型页面应有哪些章节、回答哪些问题），生产 agent 无需隐藏 prompt 也能写出结构一致的页面；类型可在 `.wiki/wiki.toml [page_types.<name>]` 扩展，未注册的 type 会被 `lint` 标出
- `loredex lint`（断链/stale/漂移）适合挂 CI；页面 id 支持 `architecture/overview` 式分层，`--type` 标注页面种类

## 架构

```
crates/
├── loredex-core     数据模型（Status/Evidence/PageMeta）、Markdown 解析、
│                 引用语法解析器、分词 + 倒排索引（无 IO）
├── loredex-git      git2 只读封装：HEAD blob、行区间、tree diff、对象库直读
├── loredex-engine   业务层：check_page（双重过期判定）、verify_page（sha1 校验）、
│                 drift 投影（similar 行级 diff）、put_page、modules 发现
├── loredex-cli   clap 入口（loredex）
└── loredex-mcp   rmcp 3.8 stdio server（loredex-mcp）
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

## 许可

GNU GPL v3.0 或更新版本（见 [LICENSE](LICENSE)）。`.wiki/` 中的 wiki 数据不受影响——它只是你仓库里的普通 Markdown 文件。

## 开发

```bash
cargo nextest run --workspace          # 54 个测试（单测 + git-fixture 集成 + MCP 协议内测试）
cargo llvm-cov nextest --workspace --summary-only   # 覆盖率基线
```

- 测试栈：cargo-nextest（每测试独立进程，env 操纵安全；`.config/nextest.toml` 配 15s slow-timeout，死锁会失败而非挂起）+ cargo-llvm-cov
- 当前覆盖：总计 ~75% 区域 / 79% 行；核心层（core/engine/git/modules）80-98%，`wiki-mcp` 77%（进程内 duplex client↔server 走真实协议栈验证四层解析链、roots 能力门控、全生命周期），CLI 入口为薄参数映射未单测
- wiki-mcp 拆为 lib+bin：集成测试直接构造 `WikiServer::new_open()/pinned()`
