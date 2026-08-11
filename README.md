# openMoonvy-mcp-rs

**Moonvy 设计稿 → 前端代码的 MCP 服务器（Rust 版）**。纯 API 架构，单二进制分发，无需 Node.js。

当前为 **PoC 阶段**：3 个核心工具已可用（真实 Moonvy API 实测通过）。

## 特性

- 🚀 单二进制：编译后一个可执行文件，无运行时依赖
- ⚡ 纯 API：Bearer token 直连 Moonvy，无浏览器、无 daemon
- 🌳 输出降噪：`skipEmptyGroups`（丢弃空容器）、`flatten`（画板原点绝对坐标）、`only`（类型过滤）、`detectDuplicates`（重复标注）
- 📐 强类型：serde 解析 genome，字段错误编译期发现

## 工具（PoC）

| 工具 | 说明 |
|---|---|
| `moonvy_get_design` | 设计元数据（标题、画框尺寸） |
| `moonvy_get_tree` | 图层树，支持 `withStyle` / `skipEmptyGroups` / `flatten` / `only` / `detectDuplicates` |
| `moonvy_extract_tokens` | 设计 Token（colors/fontSizes/radii/spacing） |

## 构建

```bash
cargo build --release
# 产物：target/release/openmoonvy-mcp-rs(.exe)
```

## 配置 token

优先级：`MOONVY_TOKEN` 环境变量 > `~/.moonvy-ai/token.json`

```bash
export MOONVY_TOKEN="<JWT>"          # 或
mkdir -p ~/.moonvy-ai && # 写入 token.json: {"token":"<JWT>", ...}
```

## 配置 MCP（opencode / Claude Code / Cursor）

```json
{
  "mcp": {
    "moonvy": {
      "type": "local",
      "command": ["/absolute/path/to/openmoonvy-mcp-rs"],
      "enabled": true
    }
  }
}
```

## 架构

```
src/
├── main.rs     # stdio 入口（rmcp 官方 Rust SDK）
├── server.rs   # MCP 工具（tool_router 宏）
├── api.rs      # Moonvy API 客户端（reqwest + gzip）
├── genome.rs   # genome 解析（树/样式/token，纯函数）
└── token.rs    # token 加载
```

行为契约与 TypeScript 版（moonvy-ai）对齐：相同的节点结构、样式归一化、树选项语义。

## 路线图

- [x] PoC：3 核心工具 + stdio + 真实 API 实测
- [ ] 全量工具（list_pages/layers/style/sync/search/asset/diff）
- [ ] 自动登录（浏览器 CDP）
- [ ] cargo-dist 发布（GitHub Releases + Homebrew + crates.io）
- [ ] 单元测试（Rust）

## License

MIT
