中文 | [English](./README.md)

# xnip

> 为 LLM agent 设计的精准文本编辑 CLI — token 消耗降低 ≥ 70%。

xnip 把 LLM agent 的「读一段 → 生成新一段 → 整段替换」压缩为「文件路径 + 位置 + 新内容」一条命令。

- 单一 Rust 静态二进制，全平台分发
- 9 个子命令：`peek` / `find` / `replace` / `insert` / `move` / `indent` / `apply` / `mcp` / `doctor`
- `apply` 接受三种格式（原生紧凑 / JSON / YAML）的批量原子编辑
- 原子提交、可预览（`--dry-run`）、参数对称可逆（`--revert`）
- **`xnip mcp`**：内置 [Model Context Protocol](https://modelcontextprotocol.io/) stdio server，与 Claude Desktop / Cursor / Cline / Continue / Zed 等插拔即用

## 安装

### 从源码安装

```sh
git clone https://github.com/SeaLoong/xnip
cd xnip
cargo install --path .
```

### 预构建二进制

```sh
# macOS / Linux
curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh

# Windows
iwr -useb https://github.com/SeaLoong/xnip/releases/latest/download/install.ps1 | iex

# 任何装 Rust 的平台
cargo install xnip
```

## 快速上手

```sh
# 看第 30-45 行
xnip peek src/Foo.vue --lines 30-45

# 找匹配位置
xnip find --pattern '^const PORT' src/Foo.vue

# 替换某行（先预览）
xnip replace src/Foo.vue --lines 30 --text "const X = 1;" --dry-run

# 跨文件改常量名
xnip replace --files-from list.txt --pattern OLD_NAME --repl NEW_NAME

# 批量原子编辑
xnip apply edits.txt
```

详见 [`docs/SKILL.md`](./docs/SKILL.md) 与 [`docs/examples.md`](./docs/examples.md)。

## 集成到 LLM Agent

xnip 提供两条集成路径，可同时启用。

### 路径 A — MCP（结构化工具协议）

适用于原生支持 MCP 的 agent。agent 直接调用 8 个结构化工具，无需处理 shell 引号转义或退出码。

首先确认 xnip 在 PATH 中：`xnip doctor`

然后将 xnip 加入 agent 的 MCP 配置：

**Claude Desktop**

编辑 `~/Library/Application Support/Claude/claude_desktop_config.json`（macOS）或 `%APPDATA%\Claude\claude_desktop_config.json`（Windows），然后重启：

```json
{
  "mcpServers": {
    "xnip": { "command": "xnip", "args": ["mcp"] }
  }
}
```

**Claude Code**

在项目根目录创建 `.mcp.json`：

```json
{
  "mcpServers": {
    "xnip": { "command": "xnip", "args": ["mcp"] }
  }
}
```

**Cursor**

在项目目录创建 `.cursor/mcp.json`，或全局使用 `~/.cursor/mcp.json`。可直接复制 [`integrations/cursor/mcp.json`](./integrations/cursor/mcp.json)：

```json
{
  "mcpServers": {
    "xnip": { "command": "xnip", "args": ["mcp"] }
  }
}
```

**Cline / Continue**

界面 → 设置 → MCP → 添加 server → command: `xnip`，args: `["mcp"]`

**Zed**

在 `~/.config/zed/settings.json` 中添加：

```json
{
  "context_servers": {
    "xnip": { "command": { "path": "xnip", "args": ["mcp"] } }
  }
}
```

重启客户端后，8 个工具出现在 agent 工具面板：`xnip_peek`、`xnip_find`、`xnip_replace`、`xnip_insert`、`xnip_move`、`xnip_indent`、`xnip_apply`、`xnip_doctor`。完整 MCP 手册：[`docs/mcp.md`](./docs/mcp.md)。

### 路径 B — Skill / prompt 注入

适用于通过 system prompt 或指令文件驱动的 agent。把对应 snippet 放到指定位置后，agent 会自主调用 `xnip <子命令>` 完成文件编辑。

| Agent | 仓库中的源文件 | 放置位置 |
|-------|--------------|---------|
| **GitHub Copilot** | [`integrations/copilot/xnip.md`](./integrations/copilot/xnip.md) | 追加到项目的 `.github/copilot-instructions.md` |
| **Aider** | [`integrations/aider/CONVENTIONS.md`](./integrations/aider/CONVENTIONS.md) | 合并到项目的 `CONVENTIONS.md` |
| **Claude Code** | [`integrations/claude-code/SKILL.md`](./integrations/claude-code/SKILL.md) | 复制到项目的 `.claude/skills/xnip.md` |
| **AGENTS.md**（Codex 等） | [`integrations/agents-md/AGENTS.md`](./integrations/agents-md/AGENTS.md) | 追加到项目的 `AGENTS.md` |
| **其他 agent** | [`integrations/generic/SKILL.md`](./integrations/generic/SKILL.md) | 粘贴到 agent 的 system prompt 或自定义指令中 |

## 设计理念

- **项目无关**：不感知项目结构、不读项目配置、不假设语言
- **跨平台一致**：所有平台跑同一份编译产物
- **机器友好**：stderr 给人，stdout 给机器；可选 `--json` NDJSON
- **原子写入**：tmpfile 同目录 → 校验 → atomic rename；默认不写 `.bak`（`--backup` 启用）
- **参数对称可逆**：`--revert` 用同样参数反向执行；不可逆即报错

完整设计规范：[`PLAN.md`](./PLAN.md) · 版本历史：[`CHANGELOG.md`](./CHANGELOG.md)

## License

MIT — 见 [`LICENSE`](./LICENSE)。
