中文 | [English](./README.md)

# xnip

> 为 LLM agent 设计的精准文本编辑 CLI。

xnip 把 LLM agent 的「读一段 → 生成新一段 → 整段替换」压缩为「文件路径 + 位置 + 新内容」一条命令。

- 单一 Rust 二进制，支持 macOS、Linux 和 Windows 的发布目标
- 11 个子命令：`edit` / `peek` / `find` / `replace` / `insert` / `move` / `indent` / `apply` / `recover` / `mcp` / `doctor`
- `apply` 接受原生紧凑、JSON、YAML 和文本补丁格式的批量编辑，逐文件原子写入
- 原子提交、可预览（`--dry-run`）、受限反向操作（`--revert`）
- **`xnip mcp`**：内置 [Model Context Protocol](https://modelcontextprotocol.io/) stdio server，与 Claude Desktop / Cursor / Cline / Continue / Zed 等插拔即用

## 安装

预构建二进制不需要安装 Rust；源码构建需要 **Rust 1.89+**。

### 从源码安装

```sh
git clone https://github.com/SeaLoong/xnip
cd xnip
cargo install --path . --locked
```

### 预构建二进制

可以从 [GitHub Releases](https://github.com/SeaLoong/xnip/releases) 下载，也可以使用下方安装器。两种安装器都会在安装前验证压缩包的 SHA256。

```sh
# macOS / Linux
curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh

# Windows
iwr -useb https://github.com/SeaLoong/xnip/releases/latest/download/install.ps1 | iex

# 任何装 Rust 的平台：从正式版本标签安装
cargo install --git https://github.com/SeaLoong/xnip --tag v0.2.0 --locked xnip
```

支持平台、指定版本、PATH 设置、校验及卸载详见 [安装说明](docs/installation.md)。当前通过 GitHub Release 分发，不提供未经发布验证的 crates.io 安装命令。

## 默认编辑入口

v0.2.0 新增 `xnip edit` / `xnip_edit`：创建文件、按唯一字面文本块替换、删除普通文件、整文件重命名，并支持预览和检查。批量计划也支持补丁导入和失败恢复记录。v0.1.1 用户需升级后使用这些能力。详见 [edit 手册](docs/edit.md)。

```sh
xnip edit config.txt --old 'PORT=8080' --text 'PORT=3000'
xnip edit src/new.txt --create --text-file content.txt
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

可先运行 [入门教程](docs/cli.md)，再查阅 [Agent 指南](docs/SKILL.md)、[示例](docs/examples.md)、[清单格式](docs/apply-format.md) 和 [退出码](docs/error-codes.md)。

## 集成到 LLM Agent

xnip 的目标是成为默认文本编辑入口。skill 负责选择合适的编辑操作，复用已有上下文，不要求每次修改都执行 doctor/peek/find。仓库搜索和语义分析仍使用对应工具，再由 xnip 落实文本修改。

### 安装编辑 skill

使用生成的 [Codex/GitHub skill](.github/skills/xnip-editing/SKILL.md) 或 [Claude Code skill](integrations/claude-code/SKILL.md)，并一起复制 `references` 目录。入口只保留默认编辑流程，详细清单、示例和错误说明按需读取。

| 客户端 | 集成文件 | 放置位置 |
|---|---|---|
| Codex | `.github/skills/xnip-editing/` | 用户技能目录 `~/.codex/skills/xnip-editing/` |
| Claude Code | `integrations/claude-code/` | `.claude/skills/xnip/` |
| Cursor | [规则](integrations/cursor/xnip.mdc) | 项目规则，保持 `alwaysApply: false` |
| Copilot | [简短指令](integrations/copilot/xnip.md) | 项目指令 |
| Aider | [约定](integrations/aider/CONVENTIONS.md) | 项目约定 |
| 其他 agent | [通用 skill](integrations/generic/SKILL.md) | 编辑任务使用，同时保留 references |

[AGENTS.md 片段](integrations/agents-md/AGENTS.md) 提供精简的项目级入口，无需将整本手册反复放入上下文。

### 选择 CLI 或 MCP

CLI 复用现有 shell；MCP 通过结构化 old/text 参数减少引号和多行转义问题。两者都可承担默认编辑流程，注册 MCP 是独立的安装配置动作。v0.2 提供 10 个工具：`xnip_edit` 与 `xnip_apply` 支持预览和检查，`xnip_recover` 支持恢复预览；旧的行级 MCP 写工具直接提交；已发布 v0.1.1 为原来的 8 个工具。

客户端配置统一维护在 [MCP 手册](docs/mcp.md)。读取 skill 不等于需要自动注册新的 MCP 服务。

## 设计理念

- **项目无关**：不感知项目结构、不读项目配置、不假设语言
- **跨平台一致**：各平台使用对应的原生二进制，命令行为保持一致
- **机器友好**：stderr 给人，stdout 给机器；可选 `--json` NDJSON
- **原子写入**：tmpfile 同目录 → 校验 → atomic rename；默认不写 `.bak`（`--backup` 启用）
- **参数对称可逆**：`--revert` 支持受限的反向操作，具体限制见下文

历史设计计划：[`PLAN.md`](./PLAN.md) · 版本历史：[`CHANGELOG.md`](./CHANGELOG.md)

## License

MIT — 见 [`LICENSE`](./LICENSE)。

## 安全边界与使用限制

- `apply` 先校验全部操作，再逐文件提交。提交失败会尝试回滚，返回退出码 4 并保留恢复记录；重试前检查回滚结果，必要时使用 [recover](docs/recovery.md)。检测到外部内容冲突时不会强制恢复，这不等于跨文件瞬间原子可见。
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

参与开发见 [CONTRIBUTING.md](CONTRIBUTING.md)，维护者发布步骤见 [docs/releasing.md](docs/releasing.md)。

[补丁导入](docs/patch.md) · [事务恢复](docs/recovery.md)
