# Loredex

中文文档（[English README](README.md)）

[![CI](https://github.com/zkz098/loredex/actions/workflows/ci.yml/badge.svg)](https://github.com/zkz098/loredex/actions/workflows/ci.yml)
[![GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSE)

给 AI coding agent 提供**可信的仓库 wiki**：纯 Markdown 存储、git 驱动的新鲜度追踪、每条声明钉住可验证的代码出处。Rust 实现，双入口（CLI + MCP server）。

## 为什么

Agent 读仓库的痛点：上下文贵、过期文档误导。Loredex 的答案是：

- **Evidence 钉定** — 页面正文里的每个声明都携带 `[[cite:path:Lx-Ly]]` 标记，落库时解析为 `(路径, 行区间, git blob sha, 片段内容 sha1)` 四元组。没有引用的内容拒绝入库。
- **密钥门禁** — 引用行区间命中密钥形态（云厂商/平台 API key 与 token、URL 内嵌凭据、私钥块）时写入即拒绝，`lint` 会回扫存量页面；扫描带整文件上下文，引用区间落在 PEM 私钥正文里、即使不含 `-----BEGIN` 行也会被拦下。片段明文从不落库、从不回显，预览只出脱敏文本。
- **Git 自动追踪过期** — 不信任 commit 号本身：对比 HEAD blob、用 tree diff 交叉验证、校验时直接重算行区间内容哈希。代码一改，相关页面即判 stale。
- **行号漂移投影** — 源文件增删行后，旧引用通过行级 diff 自动映射到新位置（相似度达标就采纳并在 note 留痕），页面保持 verified；确实丢失的降级 draft 强制重写
- **重命名自动恢复** — `git mv` 不改内容时按 blob oid 在 HEAD 反查新路径（内容寻址，比重命名启发式更精确）；mv+小改走 git 相似度改名检测 + 行投影；两步都失败的才要求人工重写。
- **零数据库** — frontmatter 就是数据库。`.wiki/pages/*.md` 随 git 提交、可 review、可手改，不存在可损坏的权威索引；搜索索引每次从文件毫秒级重建。

## 安装

**从 crates.io（cargo）**，需要 Rust 1.89+（git2 会 vendored 编译 libgit2，几分钟）：

```bash
cargo install loredex-cli loredex-mcp
# 或在仓库检出中：
cargo build --release
# 二进制：target/release/loredex（CLI）、target/release/loredex-mcp（MCP server）
```

**预编译二进制**：GitHub Releases 提供 `loredex-cli-<target-triple>.tar.xz` / `loredex-mcp-<target-triple>.tar.xz` 归档 + shell/PowerShell 安装脚本（cargo-dist 构建 5 个目标）；`cargo binstall loredex-cli` 秒装。

**CPU 基线**：预编译产物与本地检出构建的 x86-64 统一按 `x86-64-v3` 基线编译（`.cargo/config.toml` 中 `-C target-cpu=x86-64-v3`），要求 2013+ 的 AVX2 时代 CPU；老机器删掉该配置即可重编。aarch64 目标与从 crates.io `cargo install`（配置不随包发布）不受影响。

## 快速上手（CLI）

```bash
cd /path/to/your-repo          # 需至少有一个 commit
loredex init                  # 创建 .wiki/pages/ + wiki.toml（语言设置），gitignore .wiki/.cache/

loredex modules               # 模块发现（cargo targets / 目录分组）
loredex evidence crates/foo --kw serve --kw auth  # 引用草稿：可直接粘贴的 [[cite:...]] 窗口，预览已脱敏
loredex put core --title "核心层" --body notes.md   # 引用命中密钥形态行会被拒绝（--no-redact 放行）
loredex put core --title "核心层" --body notes.md --if-match <page_sha1>   # 乐观并发：页面被他人改过则拒绝写入（`show core --sha1` 可取令牌）
loredex list / show core / search "关键词"
loredex check --mark          # 过期扫描，exit code 非 0 = 有 stale（可挂 CI）
loredex diff --since v0.1.4   # ref→HEAD 变更集触及哪些页面的证据（--json 供 PR 机器人，--fail-on-affected 做门禁）
loredex links                 # 基于共引/模块邻接/文本相似度的 [[wiki:id]] 互链推荐（仅供参考，不算 lint 错误）
loredex plan [--json]         # 生成计划：未覆盖模块(New) + stale页(Refresh)，写入 .wiki/runs.json 账本
loredex claim --agent codex-1 --limit 1   # 多 agent：租约认领 plan 任务（租期过期后可被回收）
loredex pack crates/foo --budget-tokens 8000   # Context Pack：已验证页面 + 引用处 HEAD 源码片段，去重并按 token 预算裁剪
loredex index                 # 重新生成 pages/index.md + pages/quickstart.md（按类型读序 + agent 协议）
loredex note retry-loop --title "重试卡死" --body notes.md --type troubleshooting   # 无引用的随手记录：存为 draft，由 plan 排队补引用
loredex organize              # 列出证据高度重合的页面对，提示合并或互链（参考性，不是错误）
loredex playbooks             # 页面类型写作指南（内置 8 类：architecture/workflow/system/integration/testing + rule/troubleshooting/decision，另支持 wiki.toml 扩展）
loredex lint                  # 断链 / stale-but-verified / 正文-frontmatter 漂移 / 未知 type / 密钥引用回扫，有问题 exit 1（overlapping-evidence 属 advisory，不会让 CI 失败）
loredex auto [--json]         # 一条命令维护：投影自愈 -> lint -> 剩余工作开 plan（exit 1 = 有撰写工作）
loredex task <id> skipped     # 手工销账 plan 任务（skip 会被记住，不再复发；plan --reset 重新暴露）
loredex templates --out contrib   # 导出 GitHub Action 模板 + post-merge hook 模板
loredex verify core           # 内容哈希级校验（不依赖 commit 号）
loredex refresh core          # 漂移投影刷新
loredex config --language zh  # 工具生成骨架文本的语言（index/quickstart、playbook 指南、AGENTS.md 块）；agent 撰写的正文不会被翻译
loredex agents-md             # 在 AGENTS.md 写入托管发现块（--check 供 CI、--remove、--file CLAUDE.md）
loredex import ./openwiki     # 导入 openwiki / Google OKF 包；无可解析引用的正文落为 draft 交由 plan 补引用（--dry-run / --force / --prefix）
```

`--body -` 从 stdin 读取。页面正文示例：

```markdown
serve() 在循环中处理请求 [[cite:src/api.rs:L1-L5]]，main 先初始化 db [[cite:src/main.rs:L3-L6]]。
```

## Agent 接入（MCP）

**零配置接入（推荐）**：一份全局配置即可服务任意仓库，server 按四层解析链自动定位目标仓库——`--repo` 参数（强制钉定）→ `LOREDEX_REPO` 环境变量 → 客户端 MCP roots → 进程工作目录（主流客户端都在 workspace 目录 spawn server）：

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
| `wiki_diff_since` | ref→HEAD 变更集与各页证据连接（PR 范围过期分析） |
| `wiki_link_suggestions` | 带理由的跨页 `[[wiki:id]]` 互链推荐（排序输出） |
| `wiki_note` | 无引用也可落页：正文无引用则存为 `draft`（明确未验证），由 plan 排队补证据 |
| `wiki_organize` | 列出证据高度重合的页面对，给出笔记/规则重复项的整理清单 |
| `wiki_list` / `wiki_read` | 页面清单 / 读页面（stale 带醒目 warning） |
| `wiki_search` | 分词搜索（驼峰、下划线、CJK 二元组、前缀） |
| `wiki_context_pack` | 面向单个模块的 token 预算上下文包：已验证且新鲜页面 + 去重后的 HEAD 片段；stale/draft 默认跳过并列出 |
| `wiki_claim_tasks` | 把待办 plan 任务租给某个 agent，避免多个 agent 重复撰写；租期到期可被其他 agent 回收 |
| `wiki_get_evidence` | 把单条引用展开为 HEAD 当前源码内容 |
| `wiki_list_modules` | 发现待编写模块，含已有引用覆盖统计 |
| `wiki_evidence_candidates` | 引用草稿：按关键字排序的 `[[cite:...]]` 窗口，预览已脱敏 |
| `wiki_put_page` | 提交 agent 撰写的内容（零引用拒绝；命中密钥形态引用拒绝，除非 `allow_secret_citations`）；传入 `wiki_read.page_sha1` 得到的 `if_match` 可拒绝并发覆盖，`if_absent` 用于只允许创建 |
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
                                                └─ 内容丢失     → draft（保留最后已知 pin），agent 重写
```

- agent 随手发现但暂时配不上引用的知识，用 `loredex note` / `wiki_note` 落为 `draft`（显式「未验证」标记，不会被当成事实）；等 `plan` 排队补上证据后才晋升 verified。`loredex organize` 与 `overlapping-evidence` lint 提示负责把长期积累的 notes/rules 重复项列成待办，避免不成体系地散落
- 多 agent 并行无需额外服务：`wiki_read` / `loredex show` 返回页面内容 sha1，写入时传 `if_match` 会把并发覆盖变成显式冲突而不是丢失更新；页面写入与 `runs.json` 账本由操作系统文件锁（`flock` / `LockFileEx`）跨进程串行化，持锁进程崩溃时内核自动释放；`wiki_claim_tasks` / `loredex claim` 在 `runs.json` 中租约认领 plan 任务（租期过期即可回收，崩溃的 agent 不会永久占坑）
- 页面与 `.wiki/` 一起提交进 git，团队共享、PR 可 review；`runs.json` 账本同样入库，agent 中断后 `wiki_plan` 自动续传 pending 任务
- plan 任务自带 `suggested_type` 与 playbook `instructions`（该类型页面应有哪些章节、回答哪些问题），生产 agent 无需隐藏 prompt 也能写出结构一致的页面；类型可在 `.wiki/wiki.toml [page_types.<name>]` 扩展，未注册的 type 会被 `lint` 标出
- `loredex lint`（断链/stale/漂移）适合挂 CI；页面 id 支持 `architecture/overview` 式分层，`--type` 标注页面种类
- `loredex agents-md` 在 AGENTS.md/CLAUDE.md 维护标记块（`<!-- BEGIN loredex -->`），让进入仓库的 agent 自动发现 wiki 与新鲜度协议——只重写标记区域，用户内容不碰；`loredex config --language zh` 切换所有工具生成骨架文本的语言
- `loredex import` 迁移既有 openwiki / OKF 包，信任模型不破——引用不可解析的页面一律落为 `draft`，由 plan 补引用而不是洗白成 verified

## 架构

```
crates/
├── loredex-core     数据模型（Status/Evidence/PageMeta）、Markdown 解析、
│                 引用语法解析器、分词 + 倒排索引（无 IO）
├── loredex-git      git2 只读封装：HEAD blob、行区间、tree diff、对象库直读
├── loredex-engine   业务层：check_page（双重过期判定）、verify_page（sha1 校验）、
│                 drift 投影（similar 行级 diff）、put_page、modules 发现、
│                 plan/run 账本、index、lint、auto、wiki.toml 配置（[wiki] language）、
│                 AGENTS.md 托管块、openwiki/OKF 导入器
├── loredex-cli   clap 入口（loredex）
└── loredex-mcp   rmcp 3.8 stdio server（loredex-mcp）
```

关键实现细节：

- 过期判定 = blob 对比 **和** `生成commit→HEAD` 的 pathspec diff 交叉验证；生成 commit 不可解析也判 stale
- 漂移投影对旧行→新行建映射（删除行锚定最近存活行、区间坍缩保持宽度），投影后新旧 snippet 相似度 ≥ 0.6 才采纳
- 无法恢复的引用逐条隔离：页面其余引用照常投影，丢失项保留最后一次已知 pin，页面落盘为 `draft`（不再停留在 `verified` + stale）
- 引用校验在入库时进行：路径必须在 HEAD 存在，行区间非法直接拒绝；生成的导航页 id（`index`/`quickstart`）保留，禁止覆盖
- MCP 仓库解析四层链（cli > env > roots > cwd），roots 层带客户端能力门控与 3 秒超时，任何失配客户端都不会卡死会话
- 批量命令共享一个 `GitView`：HEAD 路径索引、按 commit 的 changed/rename 集合、blob 路径与文件内容均按命令计算一次，而非按页面；记录在 HEAD 的页面直接跳过 diff。单页工具保留直接 tree 查找，不会为一条引用遍历大型 monorepo
- `git2::Repository` 是 `!Sync`，MCP server 只保存解析出的 workdir；每次工具调用在 blocking 线程池中打开自己的 Repository。读写门控允许读工具并发，put/refresh/auto/plan/attach 串行
- 长驻 MCP server 在内存中缓存已解析页面与搜索索引，键为页面文件元数据（自身写入显式失效；外部编辑由 mtime/长度检测）

## 已知边界

- 内容质量取决于撰写者（工具保证"有出处且出处未腐化"，不保证解释正确）
- CLI 的 check/verify 仍按命令全量加载页面；MCP server 复用内存页面/索引缓存，但尚未落盘持久索引——万级页面需要它
- 正文内联引用标记在 refresh 投影后不同步行号（frontmatter 为权威）
- Context Pack 的 token 数是启发式估算（ASCII 约 4 字符 / 非 ASCII 每字符 1 token），不是模型分词器；任务租约是协作提示，不是硬锁
- 非 ASCII 路径建议仓库开启 `core.quotePath=false`

## Roadmap

- M4：按 commit range 增量重建受影响页面、`post-commit` hook 集成
- 正文内联标记同步重写
- `.wiki/.cache/` 持久页面/搜索索引（超大 wiki 场景；当前 MCP 缓存仅在内存）

## 许可

GNU GPL v3.0 或更新版本（见 [LICENSE](LICENSE)）。`.wiki/` 中的 wiki 数据不受影响——它只是你仓库里的普通 Markdown 文件。

## 开发

```bash
cargo nextest run --workspace          # 132 个测试（单测 + git-fixture 集成 + MCP 协议内测试）
cargo llvm-cov nextest --workspace --summary-only   # 覆盖率基线
cargo bench -p loredex-engine --bench scan -- --budget   # 扫描/索引回归下限（CI 门禁）
```

- 测试栈：cargo-nextest（每测试独立进程，env 操纵安全；`.config/nextest.toml` 配 15s slow-timeout，死锁会失败而非挂起）+ cargo-llvm-cov + criterion
- 基准：2000 页合成仓库覆盖全新扫描、单文件修改、300 页引用被删除文件、索引构建与页面加载；`--budget` 用数量级余量的上限做硬门禁，CI 同时记录 criterion 统计基准
- 当前覆盖：总计 ~75% 区域 / 79% 行；核心层（core/engine/git/modules）80-98%，`loredex-mcp` 77%（进程内 duplex client↔server 走真实协议栈验证四层解析链、roots 能力门控、全生命周期），CLI 入口为薄参数映射未单测
- loredex-mcp 拆为 lib+bin：集成测试直接构造 `WikiServer::new_open()/pinned()`
- 发布：cargo-dist（`dist-workspace.toml`）在 `v*` tag 上构建 5 平台产物 + shell/PowerShell 安装脚本；随后 `.github/workflows/release-notes.yml` 用 [git-cliff](https://git-cliff.org)（配置在 `cliff.toml`）渲染的更新日志替换 GitHub Release 正文——仓库里刻意不保留 CHANGELOG.md
