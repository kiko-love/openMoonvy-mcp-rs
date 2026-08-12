# openMoonvy-mcp-rs

[![Crates.io](https://img.shields.io/crates/v/openmoonvy-mcp-rs)](https://crates.io/crates/openmoonvy-mcp-rs)
[![CI](https://github.com/kiko-love/openMoonvy-mcp-rs/actions/workflows/release.yml/badge.svg)](https://github.com/kiko-love/openMoonvy-mcp-rs/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Moonvy 设计稿 → 前端代码的 MCP 服务器（Rust）。纯 API 架构，单二进制分发，无需 Node.js、无浏览器运行时。

- **单二进制**：编译后一个可执行文件，无运行时依赖
- **纯 API**：Bearer token 直连 Moonvy（`global-api.moonvy.com`），无浏览器、无 daemon
- **输出降噪**：`skipEmptyGroups`（丢弃空容器）、`flatten`（画板原点绝对坐标）、`only`（类型/`image` 语义过滤）、`detectDuplicates`（重复标注）
- **定向检索**：`moonvy_find_node` 按名称/文本搜索节点（id/bbox/text/容器上下文），无需全树转储
- **状态对比**：`moonvy_diff_designs` 输出 added/removed/changed + before/after 快照；同名节点按最小总几何距离（匈牙利匹配）+ 类型约束配对，避免交叉误报
- **蒙版语义**：`isMaskGroup` 标注裁剪容器（兼容 `isPureMask`），`skipEmptyGroups` 不再误删蒙版组
- **样式代码**：`moonvy_get_style_code` 一键生成 CSS / Tailwind（绝对定位 + 圆角判圆 + 渐变/描边/字重），支持 `nodeId` 限定组件
- **工作区索引**：`.moonvy-mcp/catalog.json` 支持按名称/别名/标签检索设计（含尺寸元数据）

## 目录

- [安装](#安装)
- [快速上手](#快速上手)
- [工具](#工具)
- [配置 MCP](#配置-mcp)
- [工作区索引](#工作区索引)
- [构建与测试](#构建与测试)
- [架构](#架构)
- [发布流程](#发布流程)
- [FAQ](#faq)

## 安装

**GitHub Releases（推荐，单二进制）**：

```bash
# macOS / Linux
curl -LsSf https://github.com/kiko-love/openMoonvy-mcp-rs/releases/latest/download/openmoonvy-mcp-rs-installer.sh | sh
# Windows (PowerShell)
irm https://github.com/kiko-love/openMoonvy-mcp-rs/releases/latest/download/openmoonvy-mcp-rs-installer.ps1 | iex
```

自动下载对应平台二进制（Windows/macOS/Linux × x86_64/arm64），带 sha256 校验。

**crates.io（开发者）**：

```bash
cargo install openmoonvy-mcp-rs
```

## 快速上手

1. **登录**：调用 `moonvy_login`（自动打开浏览器登录并保存 token），或设置 `MOONVY_TOKEN` 环境变量。
2. **（可选）索引项目**：调用 `moonvy_sync_project`（workspaceDir 为前端项目根目录），之后可按设计名称直接取树。
3. **获取设计**：调用 `moonvy_get_design_context`（URL 传 `https://moonvy.com/project/...` 目录或具体设计文件 URL），一次拿到元数据 + 图层树 + Token。
4. **降噪还原**：`moonvy_get_tree` 带 `skipEmptyGroups` + `flatten` + `detectDuplicates`；找特定元素用 `moonvy_find_node`；取图用 `moonvy_get_asset_url` / `moonvy_download_asset`。
5. **对比状态**：`moonvy_diff_designs` 对比普通态与 hover 态两个设计，直接输出变更图层。
6. **生成样式代码**：`moonvy_get_style_code`（`format: "css" | "tailwind"`，可用 `nodeId` 限定单个组件）直接产出可粘贴的样式片段。

**典型工作流（分层获取）**：

```
find_node("编辑个人资料")   → 拿到 containerId（弹窗组件容器）
get_tree(nodeId=containerId) → 只拉取该组件子树（KB 级，非整页 60KB）
get_style_code(nodeId=...)  → 生成 CSS/Tailwind 样式片段
```

## 工具

### 设计浏览

| 工具 | 说明 |
|---|---|
| `moonvy_list_pages` | 项目文件列表（分页 BFS，含 preview/尺寸/时间元数据） |
| `moonvy_get_design` | 设计元数据（标题、画框尺寸） |
| `moonvy_get_design_context` | 一次返回 元数据+图层树+Token（聚合入口） |

### 图层与检索

| 工具 | 说明 |
|---|---|
| `moonvy_get_tree` | 图层树：`withStyle` / `skipEmptyGroups` / `flatten` / `only` / `detectDuplicates` / `includeAssets` / `nodeId` 子树 / `region` 区域 |
| `moonvy_get_tree_by_name` | 按名称取图层树（基于 catalog 索引，唯一精确匹配直接返回） |
| `moonvy_find_node` | 按名称/文本子串搜索节点（id/bbox/text/containerId） |
| `moonvy_list_layers` | 扁平图层列表（找节点 ID） |
| `moonvy_get_node_style` | 单节点样式（strokeWidth/strokeColor/gradient/字重映射） |
| `moonvy_extract_tokens` | 设计 Token（colors/fontSizes/radii/spacing） |

### 样式与资产

| 工具 | 说明 |
|---|---|
| `moonvy_get_style_code` | 生成 CSS / Tailwind 样式代码（支持 nodeId 限定组件） |
| `moonvy_get_asset_url` | 资产直链（slice/snapshot/image，不落盘） |
| `moonvy_download_asset` | 下载切图/快照/图片填充到本地 |

### 对比与索引

| 工具 | 说明 |
|---|---|
| `moonvy_diff_designs` | 对比两个设计（added/removed/changed + before/after） |
| `moonvy_sync_project` | 扫描项目写入 `.moonvy-mcp/catalog.json` 索引 |
| `moonvy_search_designs` | 按名称/别名/标签检索索引 |

### 认证

| 工具 | 说明 |
|---|---|
| `moonvy_login` | 浏览器引导登录（自动保存 token） |
| `moonvy_set_token` | 手动保存 Moonvy JWT（有效期约 180 天） |

## 配置 MCP

### opencode

```json
{
  "mcp": {
    "moonvy": {
      "type": "local",
      "command": ["/absolute/path/to/openmoonvy-mcp-rs"],
      "enabled": true,
      "environment": {
        "MOONVY_WORKSPACE_DIR": "/path/to/frontend"
      }
    }
  }
}
```

### Claude Code / Cursor

```json
{
  "mcpServers": {
    "moonvy": {
      "command": "/absolute/path/to/openmoonvy-mcp-rs",
      "env": {
        "MOONVY_WORKSPACE_DIR": "/path/to/frontend"
      }
    }
  }
}
```

`MOONVY_WORKSPACE_DIR`（可选）启用 catalog/aliases 资源与 workspace 自动补全。

## 工作区索引

`moonvy_sync_project` 将 Moonvy 项目扫描写入 `.moonvy-mcp/catalog.json`，此后可按名称/别名/标签检索设计，无需记忆 URL：

```
moonvy-sync_project(projectUrl, workspaceDir)  → 生成 catalog.json
moonvy-search_designs(query, workspaceDir)     → 按名称/别名/标签检索
moonvy-get_tree_by_name(name, workspaceDir)    → 直接取树
```

索引也作为 MCP 资源暴露（`moonvy://catalog/{workspaceId}` / `moonvy://aliases/{workspaceId}`），支持 workspace 自动补全。

## 构建与测试

```bash
cargo build --release          # 产物：target/release/openmoonvy-mcp-rs(.exe)
cargo test                     # 46 个单元测试（serde 解析/树选项/diff/token/样式代码）
cargo test -- --ignored --nocapture real_api_smoke   # 真实 Moonvy API 端到端实测
```

## 架构

```
src/
├── main.rs     # stdio 入口（rmcp 官方 Rust SDK）
├── server.rs   # MCP 工具注册与 schema（#[tool_router] 宏）+ resources/prompts
├── tools.rs    # 业务逻辑（分页/资产解析与下载/catalog 同步）
├── api.rs      # Moonvy API 客户端（reqwest + gzip + 缓存）
├── genome.rs   # genome 解析（树/样式/token/diff/搜索/样式代码生成，纯函数）
├── catalog.rs  # workspace 索引（catalog.json + 检索）
├── token.rs    # token 加载与 JWT 过期解析
└── login.rs    # 引导登录（CDP 捕获 token）
```

## 发布流程

cargo-dist 自动构建：`git tag vX.Y.Z && git push origin vX.Y.Z` → GitHub Actions 构建 5 平台 → 上传 Release + 安装脚本；另可用 `cargo publish --registry crates-io` 发布到 crates.io。

## FAQ

**Q: 为什么是全树拉取时输出很大？**
A: 单个 1440x900 设计页全树约 60KB（compact+flatten 后）。建议用 `moonvy_find_node` + `get_tree(nodeId)` 分层获取，只拉目标组件子树。

**Q: token 失效了怎么办？**
A: 工具会返回 `[AUTH_REQUIRED]` / `[AUTH_EXPIRED]`，调用 `moonvy_login` 重新登录即可。token 有效期约 180 天。

**Q: 支持多个前端项目吗？**
A: 支持。设置 `MOONVY_ALLOWED_WORKSPACES`（分号分隔多个目录），每个工作区独立维护 catalog 索引。

## License

MIT
