# xnip

[![CI](https://github.com/SeaLoong/xnip/actions/workflows/ci.yml/badge.svg)](https://github.com/SeaLoong/xnip/actions/workflows/ci.yml)
[![最新发布](https://img.shields.io/github/release-date/SeaLoong/xnip?label=release)](https://github.com/SeaLoong/xnip/releases/latest)
[![MIT 许可证](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![支持平台](https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-blue)](docs/installation.md#supported-release-targets)

中文 | [English](README.md)

**用更少的重复上下文，完成精准文本编辑。** 编码 agent 传入已知的原文本块与替换内容，xnip 精确修改目标、保留未修改字节，并返回简短回执。

- **只传需要修改的内容。** 复用已知上下文，省去重复发送未变化的文件或长行。
- **明确定位，精准修改。** 原文本缺失或存在歧义时不写入；精确编辑保留 CRLF 与末尾换行字节。
- **跨 agent 复用。** CLI 与 stdio MCP 共用精确编辑、批量、预览和文件生命周期操作。

## 安装

使用对应平台的独立安装器：

~~~sh
# macOS / Linux
curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh
~~~

~~~powershell
# Windows
Invoke-RestMethod https://github.com/SeaLoong/xnip/releases/latest/download/install.ps1 | Invoke-Expression
~~~

也可[下载发布文件](https://github.com/SeaLoong/xnip/releases)，或使用 [npm 包 `xnip-cli`](docs/installation.md#npm)。PATH、校验和及其他选项见[安装说明](docs/installation.md)。

独立二进制支持静默更新；设置 `XNIP_AUTO_UPDATE=0` 可关闭。npm 安装通过 npm 更新。详见[更新行为](docs/updates.md)。

## 完成一次编辑

先预览，再应用：

~~~sh
xnip edit config.env --old 'PORT=8080' --text 'PORT=3000' --dry-run
xnip edit config.env --old 'PORT=8080' --text 'PORT=3000'
~~~

原文本块必须非空且唯一。有歧义时扩展上下文；有意全局替换时使用 `--all`。`--check` 只校验，不写入。

显式重写完整文件可用 `xnip edit generated.txt --write --text-file content.txt`。多行内容使用文件或 stdin 传入，可避免 shell 转义错误。

将多处修改保存为 `edits.json`：

~~~json
{
  "config.env": [["PORT=8080", "PORT=3000"]],
  "src/settings.py": [["DEBUG = True", "DEBUG = False"]]
}
~~~

~~~sh
xnip apply edits.json --format compact --dry-run
xnip apply edits.json --format compact
~~~

同一文件内的文本对依次执行。上下文缺失时用 `peek` 和 `find` 补足。插入、行编辑、补丁、创建、重命名与删除见 [CLI 用法](docs/cli.md)、[编辑示例](docs/examples.md)和[批量格式](docs/apply-format.md)。

## 接入 agent

可直接使用 agent shell 中的 CLI，也可添加 stdio MCP 服务。使用 `mcpServers` 配置的客户端可参考：

~~~json
{
  "mcpServers": {
    "xnip": {
      "command": "xnip",
      "args": ["mcp", "--receipts", "text"]
    }
  }
}
~~~

默认五个工具覆盖编辑、批量、读取、搜索和恢复。`--toolset full` 增加专用工具；客户端需要结构化修改元数据时，可省略 `--receipts text`。相对路径基于服务进程的工作目录。各客户端差异见 [MCP 配置](docs/mcp.md)。

| Agent / 客户端 | 接入指南 |
|---|---|
| Codex | [普通指令与编辑指南](docs/codex.md) |
| Claude Code | [Skill](integrations/claude-code/SKILL.md) |
| Cursor | [项目规则](integrations/cursor/xnip.mdc) |
| GitHub Copilot | [项目指令](integrations/copilot/xnip.md) |
| aider | [约定](integrations/aider/CONVENTIONS.md) |
| DeepSeek Harness | [DSH bundle 与 MCP 插件](docs/dsh.md) |
| Pi | [Extension](docs/pi.md) |
| Goose | [MCP extension](docs/goose.md) |
| 其他 agent | [通用 skill](integrations/generic/SKILL.md) |

希望只添加简短项目指令时，可用 [AGENTS 片段](integrations/agents-md/AGENTS.md)。[操作指南](docs/SKILL.md)帮助 agent 选择编辑方式，并按需读取细节。

## 为什么能节省编辑上下文

Benchmark 比较真实编辑器在 **13 个任务**中的表现。只有每次都得到预期文件字节的结果才进入下表。

![字节正确配置中的最佳实测编辑输入](docs/benchmarks/charts/readme-frontier.svg)

比值为逐任务 **实测原生最小输入 / 实测 xnip 最小输入** 的几何平均，各任务权重相同；高于 **1x** 表示 xnip 输入更少。“保留旧内容校验”要求原内容检查；“允许整文件写入”对双方同时开放有校验的局部编辑与整文件写入。

| xnip 接口 / 允许的操作 | 原生最小 / xnip 最小 | xnip 更少 / 持平 / 原生更少 |
|---|---:|---:|
| MCP · 保留旧内容校验 | 1.23x | 13 / 0 / 0 |
| MCP · 允许整文件写入 | 1.26x | 12 / 1 / 0 |
| CLI stdin · 保留旧内容校验 | 1.55x | 13 / 0 / 0 |
| CLI stdin · 允许整文件写入 | 1.80x | 13 / 0 / 0 |

已知完整文件时，长行中的局部修改只需 **11 个 MCP 输入 token**，实测原生最佳输入（Goose）为 **18**，Codex 的唯一上下文补丁为 **10,037**。

相比结构化回执，精简文本回执让捕获的完整 JSON-RPC 返回 token 约减少 **56%**，五工具发现目录减少 **23.8%**；编辑输入和最终文件结果保持一致。

这些数值是捕获的编辑 payload 成本，不代表完整 agent 用量或计费。CLI 执行命令参数另计。DSH 各策略的结果计入必需的读取，单独列出，不混入上表。

[完整结果与图表](docs/benchmarks/results.md) · [方法与复现](docs/benchmarks/README.md)

## 使用边界

- 原内容检查不锁住外部写入者。需要精确恢复时保留 Git 或备份。
- 批量操作先验证，再逐文件提交。提交或回滚失败后，重试前检查[恢复记录](docs/recovery.md)。
- 读取和预览有明确的输出上限。截断后缩小请求范围，或调整对应限制。
- MCP 使用启动用户的权限和工作目录，面向可信本地客户端。

[安装](docs/installation.md) · [CLI](docs/cli.md) · [Edit 手册](docs/edit.md) · [MCP](docs/mcp.md) · [退出码](docs/error-codes.md) · [变更记录](CHANGELOG.md) · [贡献指南](CONTRIBUTING.md)

采用 MIT 许可证。参见 [LICENSE](LICENSE) 和[第三方声明](THIRD_PARTY_LICENSES.md)。
