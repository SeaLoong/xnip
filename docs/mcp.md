# MCP Server (`xnip mcp`)

`xnip mcp` 启动一个遵循 [Model Context Protocol](https://modelcontextprotocol.io/) 的
**stdio server**，把 xnip 的 8 个能力（peek / find / replace / insert / move /
indent / apply / doctor）作为结构化工具暴露给 LLM agent，避免 agent 通过 shell
拼参数调用 cli 时出现的引号转义、退出码解读、stdout/stderr 混淆等问题。

## 何时用 MCP，何时直接用 CLI

| 场景 | 推荐 |
|---|---|
| Agent / LLM 集成（Claude Desktop、Cursor、Cline、Continue、Zed 等） | **MCP** |
| 终端手动操作、shell 脚本、CI 流水线 | CLI |
| 想用 `--dry-run` / `--check` / `--revert` / `--json` | CLI（MCP 不暴露这些） |
| 需要 `--text-stdin` / `apply --from-stdin` / op 内 `@-` | CLI（MCP 进程的 stdin 已被协议占用） |

二者**共享同一份 core 实现**，行为完全一致。

## 启动

```sh
xnip mcp
# 该进程不打印任何提示，等待 client 通过 stdin 喂入 JSON-RPC 帧。
# Ctrl-D（关闭 stdin）或 client 主动关闭即退出。
```

无任何子参数。所有"调用"都来自 MCP client 通过 stdio 发送的 `initialize` /
`tools/list` / `tools/call` 等请求。

## 工具清单

8 个工具，名称与 cli 子命令 1:1 对应：

| MCP Tool | CLI 对应 | 类型 |
|---|---|---|
| `xnip_peek` | `xnip peek` | 只读 |
| `xnip_find` | `xnip find` | 只读 |
| `xnip_replace` | `xnip replace` | 写（原子） |
| `xnip_insert` | `xnip insert` | 写（原子） |
| `xnip_move` | `xnip move` | 写（原子） |
| `xnip_indent` | `xnip indent` | 写（原子） |
| `xnip_apply` | `xnip apply` | 写（两阶段批量） |
| `xnip_doctor` | `xnip doctor` | 辅助 |

每个工具的输入 schema 字段名与 cli flag **同名同义**（去掉 `--` 与连字符转下划线）。
例如 cli 的 `--match-line` 在 MCP 中是 `match_line`。

### 与 CLI 的差异

MCP **不暴露**以下 cli flag（理由见下文）：

- `--dry-run`：MCP 直接返回结果文本，LLM 拿到结构化 reply 比拿 unified diff 更易消费
- `--check`：MCP 用 `Err(McpError)` 表达校验失败，比退出码更直接
- `--revert`：cli 便利特性；LLM 可以直接构造反向编辑（成本极低）
- `--json`：MCP 本身就是结构化 JSON 协议，重复
- `--text-stdin` / `apply --from-stdin` / op 内 `@-`：MCP 进程的 stdin 被协议占用

MCP **保留**：

- `was` / `was_file`（写命令）：长会话中文件可能被外部改动，并发保护必备
- `backup`（写命令）：用户安全旁路，按需开启 `.bak` 副本
- `manifest_text`（`xnip_apply` 专属）：行内清单文本，适合 LLM 直接生成短清单
  而无需先写到文件

## 错误语义

tool 调用失败时返回 JSON-RPC `error` 对象（而不是 `result.isError=true`）：

| 场景 | error.code | 含义 |
|---|---|---|
| 参数缺失 / 互斥 / 类型不合法 | `-32602` (`invalid_params`) | 用户输入错误，应当修正参数重试 |
| 定位失败 / `was` 不匹配 / pattern 未命中 | `-32600` (`invalid_request`) | 状态前提不满足，可能需要先 peek 看现状 |
| 文件 IO 故障 / `apply` 阶段二部分提交 | `-32603` (`internal_error`) | 系统级错误，需人介入 |

## 调用示例

### Claude Desktop / Claude Code

`~/Library/Application Support/Claude/claude_desktop_config.json`（macOS）或项目根
`.mcp.json`（Claude Code）：

```json
{
  "mcpServers": {
    "xnip": {
      "command": "xnip",
      "args": ["mcp"]
    }
  }
}
```

### Cursor

`.cursor/mcp.json`（项目）或 `~/.cursor/mcp.json`（全局）：

```json
{
  "mcpServers": {
    "xnip": { "command": "xnip", "args": ["mcp"] }
  }
}
```

### Cline / Continue

界面 → 设置 → MCP → 添加 server：command=`xnip`, args=`["mcp"]`。

### Zed

`~/.config/zed/settings.json`：

```json
{
  "context_servers": {
    "xnip": { "command": { "path": "xnip", "args": ["mcp"] } }
  }
}
```

### 手动调试（无 client，纯 shell）

```sh
printf '%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | xnip mcp
```

应看到 `serverInfo: {name: "xnip", version: "..."}` 与 8 个工具的列表。

## 工具示例（典型 LLM 调用形态）

### `xnip_peek`：先看再改

```json
{
  "method": "tools/call",
  "params": {
    "name": "xnip_peek",
    "arguments": { "file": "src/main.rs", "match_line": "^fn main", "context": 2 }
  }
}
```

### `xnip_replace`：带并发保护的精确替换

```json
{
  "name": "xnip_replace",
  "arguments": {
    "file": "src/lib.rs",
    "lines": "12-14",
    "text": "// new implementation\nfn foo() { todo!() }\n",
    "was": "// old impl\nfn foo() { 1 + 1 }\n// trailing\n"
  }
}
```

`was` 当前 12-14 行字面比对失败 → 返回 `invalid_request` 错误，文件不会被修改。

### `xnip_apply`：批量原子提交

```json
{
  "name": "xnip_apply",
  "arguments": {
    "manifest_text": "replace src/a.rs lines 30-32 text=\"...\"\ninsert src/b.rs lines 1 position=before text=\"// header\\n\"\n",
    "format": "native",
    "backup": true
  }
}
```

阶段一全部 op 校验 + 暂存；任一失败 → 不写任何文件返回错误。
全部 OK → 阶段二依次原子 rename 提交；中途失败回滚已提交的部分（`backup=true` 时
通过 `.bak` 还原）。

## 故障排查

| 现象 | 排查 |
|---|---|
| client 报 "Failed to connect" | `which xnip`；client 配置里 `command` 必须是绝对路径或在 client 的 PATH 中 |
| `tools/list` 返回 0 个工具 | 升级到 v0.1.0+；旧版本无 MCP 子命令 |
| `xnip_apply` 报 "manifest contains op content `@-`" | MCP 不读进程 stdin；改用 `text` 字面或 `text_file` |
| 替换写入但 LLM 看不到变化 | 让 LLM 调用 `xnip_peek` 重新读取；不要让 LLM 把工具回执文本当作"最终文件状态" |

## 内部架构

`xnip mcp` 与 `xnip <其它子命令>` **共享同一二进制、同一 core**。MCP 工具 handler
是 cli 层的"平行前端"——直接调 `core::ops::*` / `apply::commit::*`，**不**通过
shell 调起子进程或抓取 cli stdout。这意味着：

- bug 修复在 core 层一处即可同时反馈到两端
- 行为一致性由共用代码保证（同一字节透传、同一 atomic write、同一 was 校验）
- 二进制大小约比纯 cli 大 ~9MB（rmcp + tokio 全家桶代价），但仅当 `xnip mcp`
  实际启动时才创建 tokio runtime，其它 cli 路径零运行时开销

依赖：`rmcp 1.7`（官方 Rust SDK）+ `tokio 1`（`rt, macros, io-std`）+
`schemars 1.0`（自动生成 tool input JsonSchema）。MSRV 1.95（受 rmcp / schemars
间接依赖及 Cargo.lock 解析结果约束）。
