# xnip

> Precise text editing CLI for LLM agents.

xnip 把 LLM agent 的「读一段 → 生成新一段 → 整段替换」压缩为「文件路径 + 位置 + 新内容」一条命令，token 消耗降低 ≥ 70%。

- 单一 Rust 静态二进制，全平台分发
- 7 个子命令：`peek` / `find` / `replace` / `insert` / `move` / `indent` / `apply`
- `apply` 接受三种格式（原生紧凑 / JSON / YAML）的批量编辑清单
- 原子提交、可预览（`--dry-run`）、参数对称可逆（`--revert`）
- **`xnip mcp`**：内置 [Model Context Protocol](https://modelcontextprotocol.io/) stdio server，与 Claude Desktop / Cursor / Cline / Continue / Zed 等插拔即用

## 状态

✅ **v0.1.0**：代码就绪（305 测试全过、`clippy --pedantic -D warnings` / `fmt --check` 全绿）；待打 tag 发布 6 个跨平台二进制。设计规范见 [`PLAN.md`](./PLAN.md)，版本历史见 [`CHANGELOG.md`](./CHANGELOG.md)。

## 安装

### 从源码安装（当前可用）

```sh
git clone https://github.com/SeaLoong/xnip
cd xnip
cargo install --path .
```

### 二进制发版后

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

## 与 LLM agent 集成

两条互补路径：

1. **如果你的 agent 支持 [Model Context Protocol](https://modelcontextprotocol.io/)**（如 Claude Desktop / Claude Code / Cursor / Cline / Continue / Zed）：

   ```json
   { "mcpServers": { "xnip": { "command": "xnip", "args": ["mcp"] } } }
   ```

   加入配置后重启，8 个工具会出现在 agent 工具面板中。完整手册：[`docs/mcp.md`](./docs/mcp.md)。

2. **你的 agent 靠 system prompt / skill 注入**（如 Aider / GitHub Copilot / AGENTS.md 生态）：

   拷贝对应的集成包（见 `integrations/`），中间 agent 会主动调用 `xnip <subcmd>`。

两者可以同时启用。

## 设计

- 项目无关：不感知项目结构、不读项目配置、不假设语言
- 跨平台一致：所有平台跑同一份编译产物
- 机器友好：stderr 给人，stdout 给机器；可选 `--json` NDJSON
- 原子写入：tmpfile 同目录 → 校验 → atomic rename；默认 **不** 写 `.bak`（依赖 git；`--backup` 启用）
- 参数对称可逆：`--revert` 用同样参数反向执行；不可逆即报错

完整设计见 [`PLAN.md`](./PLAN.md)。

## License

MIT — 见 [`LICENSE`](./LICENSE)。
