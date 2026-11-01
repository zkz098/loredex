# Loredex 官方网站 (Website)

基于 **Astro**、**UnoCSS**、**Remix Icon** 打造的 Loredex (repowiki) 官方项目主页。已全面切换至 **pnpm** 并针对 **Cloudflare Pages / Workers** 完成适配。

---

## 🛠️ 技术栈

- 🚀 **Framework**: [Astro 7](https://astro.build/) (Static Site Generation, 零运行时 JS 开销)
- 🎨 **Styling**: [UnoCSS](https://unocss.dev/) (原子化 CSS、自定义主题色系与 Shortcuts)
- 🔣 **Icons**: [Remix Icon 4](https://remixicon.com/) (高质量开源系统图标集)
- 📦 **Package Manager**: [pnpm 10](https://pnpm.io/) (快速、节省磁盘空间的包管理器)
- ☁️ **Deployment**: [Cloudflare Pages / Workers Static Assets](https://pages.cloudflare.com/)

---

## 💻 本地开发与构建 (pnpm)

在仓库根目录直接运行快捷命令：

```bash
# 安装依赖
pnpm install

# 启动本地开发服务 (http://localhost:4321)
pnpm site:dev

# 生产环境打包 (输出至 website/dist/)
pnpm site:build

# 本地预览打包产物
pnpm site:preview
```

或直接进入 `website` 目录独立操作：

```bash
cd website
pnpm dev
pnpm build
pnpm preview
```

---

## ☁️ Cloudflare Pages 部署指南与近期变更说明

### 1. Cloudflare Pages 近期核心变更要点

根据 Cloudflare 官方最新文档及构建系统升级：
1. **构建镜像与 Node.js 默认版本升级 (Build System V3)**：
   - Cloudflare 已将默认构建系统升级为 **V3**，并将默认 Node.js 从 Node 18 (已 EOL) 提升为 **Node.js 22**。
   - 原生支持 **pnpm 10**。
   - 本项目已通过根目录及 `website/` 下的 `.node-version`（设为 `22`）和 `package.json` 中的 `"packageManager": "pnpm@10.33.0"` 锁定环境。
2. **切勿使用旧版全局安装指令**：
   - 过去旧教程中常用的 `npm install -g pnpm && pnpm install && pnpm build` 在 V3 构建镜像中会导致 `EEXIST` 文件冲突报错。
   - **正确做法**：直接使用原生构建命令 `pnpm run build`，Cloudflare 会根据 `pnpm-lock.yaml` 和 `packageManager` 自动激活原生 pnpm。
3. **Pages 与 Workers Static Assets 的架构统一**：
   - Cloudflare 已将底层静态资产能力升级为 **Workers Static Assets**。
   - 项目中已提供标准 [`wrangler.jsonc`](./wrangler.jsonc)，既支持传统的 Pages Git 托管，也支持直接运行 `pnpm deploy:pages` 或 `pnpm deploy:worker` 进行发布。
4. **边缘缓存策略 (`_headers`)**：
   - 已配置 `public/_headers`，Astro 生成的不可变静态资源（`/_astro/*`、字体文件等）会自动应用 `Cache-Control: public, max-age=31536000, immutable`，享有 Cloudflare 全球 CDN 最优性能。

---

### 2. 在 Cloudflare Dashboard 中创建与配置

当你在 Cloudflare 控制台连接 Git 仓库部署时，请根据以下参数配置：

#### 方案 A：以 `website` 作为根目录（推荐）
- **Framework preset**: `Astro`
- **Root directory**: `website` *(关键：因为本项目为 Rust + Web 单体仓，站点源码在 website 目录下)*
- **Build command**: `pnpm run build`
- **Build output directory**: `dist`

#### 方案 B：以项目根目录 `/` 作为构建起点
- **Framework preset**: `None` 或 `Astro`
- **Root directory**: `/`
- **Build command**: `pnpm run site:build`
- **Build output directory**: `website/dist`

#### 环境变量检查（Environment Variables）：
如在旧版已存在的 Pages 项目中遇到构建异常，请在项目 **Settings > Environment variables** 中核对：
- `NODE_VERSION` = `22`
- `PNPM_VERSION` = `10`
并在 **Settings > Build & deployments** 中确认 **Build system version** 已切换为 **Version 3 (Latest)**。
