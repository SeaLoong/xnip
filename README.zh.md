中文 | [English](./README.md)

# xnip

> 为 LLM agent 设计的精准文本编辑 CLI。

xnip 把 LLM agent 的「读一段 → 生成新一段 → 整段替换」压缩为「文件路径 + 位置 + 新内容」一条命令。

- 单一 Rust 二进制，支持 macOS、Linux 和 Windows 的发布目标
- 9 个子命令：`peek` / `find` / `replace` / `insert` / `move` / `indent` / `apply` / `mcp` / `doctor`
- `apply` 接受三种格式（原生紧凑 / JSON / YAML）的批量编辑，逐文件原子写入
- 原子提交、可预览（`--dry-run`）、参数对称可逆（`--revert`）
- **`xnip mcp`**：内置 [Model Context Protocol](https://modelcontextprotocol.io/) stdio server，与 Claude Desktop / Cursor / Cline / Continue / Zed 等插拔即用

## 安装

需要 Rust **1.89+**；目前可直接使用源码安装。

### 从源码安装

```sh
git clone https://github.com/SeaLoong/xnip
cd xnip
cargo install --path . --locked
```

### 预构建二进制（首次发布后可用）

仓库目前没有已发布的 GitHub Release。以下下载命令需要先通过发布流程发布版本标签；crates.io 安装还需维护者单独发布 crate。

```sh
# macOS / Linux
curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh

# Windows
iwr -useb https://github.com/SeaLoong/xnip/releases/latest/download/install.ps1 | iex

# 任何装 Rust 的平台
cargo install xnip --locked
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
printf '%s\n' 'replace --files-from list.txt s/OLD_NAME/NEW_NAME/g' | xnip apply --from-stdin

# 批量校验后逐文件提交
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
- **参数对称可逆**：`--revert` 支持受限的反向操作，具体限制见下文

完整设计规范：[`PLAN.md`](./PLAN.md) · 版本历史：[`CHANGELOG.md`](./CHANGELOG.md)

## License

MIT — 见 [`LICENSE`](./LICENSE)。

## 安全边界与使用限制

- `apply` 先校验全部操作，再逐文件提交。提交阶段失败会返回退出码 4 并列出已提交路径，**不会自动回滚**；请按实际路径从 Git 或备份恢复。
- 备份名追加 `.bak`：`main.rs` 对应 `main.rs.bak`，会覆盖已有备份。旧版使用 `main.bak`，升级后请注意名称变化。
- 写入会跟随已有符号链接并保留文件权限，但原子替换不保留硬链接身份、所有者、ACL 或扩展属性。请避免并发写入；`--was` 仅校验读取时的内容，并非文件锁。
- `--revert` 不是撤销日志。正则替换仅支持非空字面模式和不含捕获展开的非空替换串；已有同名文本仍可能导致反向替换歧义。范围替换需保持相同行数，缩进转换及无末尾换行的编辑不保证字节级恢复。精确恢复请使用 Git 或备份。
- 行操作以 LF 分隔，已有 CR 保留，但新文本不会自动转为 CRLF。CLI `--text` 按 shell 实际传入值处理，跨平台多行文本建议使用 `--text-file` 或 stdin。
- `apply --json --check` 只输出 NDJSON，`--json` 与 `--dry-run` 互斥。MCP 使用启动进程的文件权限和工作目录，面向可信本地客户端。

## 开发与发布

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo +1.89.0 check --workspace --all-targets --locked
cargo deny check
cargo bench --locked
cargo package --locked
cargo run --locked -p xtask -- sync-integrations
```

Linux/macOS 安装器测试：`python3 tests/install_sh.py`。Windows 先运行 `cargo build --locked`，再在 PowerShell 运行 `./tests/install_ps1.ps1`。测试使用本地压缩包和模拟 HTTP，不会下载正式发布包。

发布前更新版本与变更记录，通过 CI 后推送与 Cargo 版本一致的 `vX.Y.Z` 标签。发布流程会先检查并构建六个平台，全部成功后统一发布压缩包、SHA256、安装器和集成包。手动触发仅构建产物。crates.io 由维护者单独执行 `cargo publish --locked`。更多说明见 [架构文档](docs/architecture.md)。
