中文 | [English](README.md)

# xnip

**用更少的重复上下文，完成精准文本编辑。** 编码 agent 传入已知的原文本块与替换内容，xnip 精确修改目标、保留未修改字节，并返回精简结果。

- **复用已知上下文。** 需要时补读缺失内容；已知如何修改时直接编辑。
- **只传需要的变化。** 精确文本块避免在每次编辑中重复输出整份文件或完整长行。
- **保持输入输出精简。** 简短回执、有限读取与预览、默认五个 MCP 工具，减少日常上下文负担。
- **保证定位明确。** 原文本缺失或存在歧义时不写入；显式 `all` 才做全局替换。精确编辑保留 CRLF 与末尾换行字节。
- **跨 harness 复用。** CLI 与 stdio MCP 共享文件生命周期、批量、补丁、行与锚点编辑能力。

## 实测编辑成本

Benchmark 实际运行 **Codex、Gemini CLI、OpenCode、aider 的编辑执行器**，与 xnip 在相同任务上比较：**13 个场景 × 12 种配置 × 3 次重复**，每次执行后检查目标文件字节。计数使用 `tiktoken 0.14.0 / o200k_base`。

![原生编辑工具相对 xnip 的输入成本](docs/benchmarks/charts/overview.svg)

下表为部分配置，比例均为 **原生工具 / xnip**，**xnip = 1.00x**。几何平均让每个场景权重相同。

| 编辑器 / 配置 | 全部场景编辑输入 | 字节一致配对子集输入 | 一致配对数 | 全部输入 + 已捕获返回 |
|---|---:|---:|---:|---:|
| xnip edit | 1.00x | 1.00x | 13/13 | 1.00x |
| OpenCode edit | 1.11x | 1.11x | 13/13 | 1.20x |
| Codex apply_patch | 2.61x | 2.73x | 12/13 | 3.05x |
| Gemini context | 3.81x | 3.81x | 13/13 | 4.35x |
| aider diff / LF | 2.52x | 2.78x | 11/13 | 返回未知 |
| aider whole / LF | 1.89x | 2.25x | 11/13 | 返回未知 |

**优势明显的场景：** 长行中的局部替换只需 **22 个 xnip 输入 token**，Codex 补丁需要 **10,037**。在 2,000 行文件中修改一处，xnip 需要 **36**，aider whole / LF 需要 **19,007**。这些场景的差距主要来自重复传输未变化的文本。

**差距较小的场景：** OpenCode 的字面编辑器与 xnip 已很接近。原生补丁也擅长合并修改：同文件批量场景中，Codex 输入为 **45 token**，分别调用 xnip edit 为 **46**。xnip 也提供 `apply` 批量入口；这里的原生对照配置测量的是 `edit`。

“全部”包含输出字节不一致的原生执行；配对子集只保留双方每次均得到预期字节的任务，各配置子集不同。已捕获返回指实际观察到的工具文本，aider 不可取得的返回保持未知。这些是编辑器实测，不能直接当作完整 agent 会话或计费总量。

[完整结果与逐场景图表](docs/benchmarks/results.md) · [原始证据](docs/benchmarks/results.json) · [方法与复现](docs/benchmarks/README.md)

## 安装

稳定版预构建二进制无需 Rust。两种安装器均校验下载压缩包的 SHA256：

~~~sh
# macOS / Linux
curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh
~~~

~~~powershell
# Windows
Invoke-RestMethod https://github.com/SeaLoong/xnip/releases/latest/download/install.ps1 | Invoke-Expression
~~~

也可从 [GitHub Releases](https://github.com/SeaLoong/xnip/releases) 下载。支持平台、PATH、固定版本、校验与卸载见[安装说明](docs/installation.md)。

源码安装需要 **Rust 1.89+**：

~~~sh
cargo install --git https://github.com/SeaLoong/xnip --tag v0.3.0 --locked xnip
~~~

## 编辑

~~~sh
xnip edit config.txt --old 'PORT=8080' --text 'PORT=3000'
xnip edit src/new.txt --create --text-file content.txt
xnip edit old-name.txt --rename-to new-name.txt
xnip edit obsolete.txt --delete
xnip apply edits.json --dry-run
xnip apply edits.json
~~~

字面替换要求 old 非空且唯一；有歧义时扩展原文本上下文，有意全局替换时使用 `--all`。`--dry-run` 预览，`--check` 校验，两者均不写入。需要跨 shell 可靠传入多行字节时使用文件或 stdin。

已知行范围、锚点、移动与缩进可用专门命令。`peek` 与 `find` 用于补足上下文；仓库发现与语义分析继续使用 harness 的搜索/语言工具。参见 [CLI 教程](docs/cli.md)、[edit 手册](docs/edit.md)和[示例](docs/examples.md)。

读取默认限制为 200 行、100 个搜索命中、12,000 个 payload 字节；预览默认限制 12,000 字节。截断会明确提示，对应限制设为零可取消。`edit/apply/recover` 默认返回精简回执，`--output json` 返回完整机器结果，`--output human` 展开状态与预览。

## Agent 集成

复用 PATH 上的 CLI，或配置 `xnip mcp` 使用结构化参数。MCP 默认提供 `edit/apply/peek/find/recover`；`--toolset full` 增加行级工具与诊断。多个已知文本块可一次调用 `xnip_apply({edits:[{file,old,text},...]})`。配置与返回字段见 [MCP 手册](docs/mcp.md)。

| 客户端 | 指南 | 放置位置 |
|---|---|---|
| Codex | [普通指令与指南](docs/codex.md) | 合并 AGENTS 片段；指南放在 `~/.codex/guides/xnip-editing/` |
| Claude Code | [skill](integrations/claude-code/SKILL.md) | `.claude/skills/xnip/`，保留 references |
| Cursor | [规则](integrations/cursor/xnip.mdc) | 项目规则，`alwaysApply: false` |
| Copilot | [简短指令](integrations/copilot/xnip.md) | 项目指令 |
| Aider | [约定](integrations/aider/CONVENTIONS.md) | 项目约定 |
| 其他 agent | [通用 skill](integrations/generic/SKILL.md) | 编辑任务按需加载，保留 references |

[AGENTS 片段](integrations/agents-md/AGENTS.md)提供项目级精简入口。[Agent 指南](docs/SKILL.md)负责选择操作，详细规则按需加载。Codex 使用普通指令与指南，不增加 `$xnip-editing` 菜单项。

## 静默更新

单例独立进程后台检查版本、下载并校验，不阻塞编辑，也不显示更新消息。通常等此前使用空闲后，由下一次调用打开安装窗口：CLI 空闲窗口为 30 分钟，MCP 按连接存活判断。已准备版本最长延期 24 小时，已有会话继续运行已加载的程序。

调度与内部状态见[更新说明](docs/updates.md)。固定版本或离线环境设置 `XNIP_AUTO_UPDATE=0`。

## 限制

- 批量计划先验证全部操作，再逐文件提交；提交失败尝试回滚。遇到退出码 4，重试前检查[恢复记录](docs/recovery.md)。
- 原内容校验不能锁住外部写入者。重写保留普通权限，[其他文件系统元数据有明确限制](docs/architecture.md#persistence-and-limits)。
- 备份在完整文件名后追加 `.bak`，会覆盖已有 sidecar。精确恢复使用 Git/备份；`--revert` 只支持[受限逆操作](docs/apply-format.md#--revert-semantics)。
- 行操作以 LF 分隔，需要 CRLF 时由调用方提供相应 payload。CLI 字符串遵循 shell 转义。
- MCP 使用启动用户的权限和工作目录，面向可信本地客户端。

## 文档与开发

| 需求 | 文档 |
|---|---|
| 安装与接入 | [安装](docs/installation.md)、[Codex](docs/codex.md)、[MCP](docs/mcp.md) |
| 编辑与自动化 | [CLI](docs/cli.md)、[edit](docs/edit.md)、[清单](docs/apply-format.md)、[补丁](docs/patch.md)、[退出码](docs/error-codes.md) |
| 失败恢复 | [recovery](docs/recovery.md) |
| 设计与验证 | [架构](docs/architecture.md)、[术语](GLOSSARY.md)、[测试](docs/testing.md)、[更新测试](docs/update-testing.md)、[benchmark](docs/benchmarks/README.md) |
| 贡献与发布 | [贡献指南](CONTRIBUTING.md)、[发布流程](docs/releasing.md)、[变更记录](CHANGELOG.md) |

~~~sh
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked -p xtask -- sync-integrations
~~~

平台、依赖、最低 Rust 和打包检查见[贡献指南](CONTRIBUTING.md)。MIT；参见 [LICENSE](LICENSE) 与[第三方声明](THIRD_PARTY_LICENSES.md)。
