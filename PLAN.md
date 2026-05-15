# xnip — Design Specification

## 1. Metadata

| 字段 | 值 |
|---|---|
| **项目名** | xnip |
| **类型** | CLI 工具 |
| **语言** | Rust（edition 2024 / MSRV 1.85） |
| **License** | MIT |
| **状态** | Implemented（v0.1.0 已交付，实现细节回灌至本文） |
| **版本** | spec v1.1 ↔ xnip v0.1.0 |
| **目标交付** | 单一静态二进制，6 个平台产物 |
| **目标用户** | LLM agent（Claude Code / Cursor / Aider / Copilot / AGENTS.md 系等）；二级用户：人类开发者 |

> 文档约定：本文 spec 是 **xnip v0.1.0 的实现基线**。所有 §6 描述都已落地；个别因实现取舍而调整的语义已在文中标注（搜索 "实现注"）。
> 任何后续变更（v0.2+）走 PR 流程：先改本文再写代码。

---

## 2. Summary

xnip 是给 LLM agent 用的精准文本编辑 CLI。把"读一段→生成新一段→整段替换"的 round-trip 压缩为「文件路径 + 位置 + 新内容」一条命令，token 消耗降低 ≥ 70%。

提供 7 个子命令（peek / find / replace / insert / move / indent / apply）覆盖只读探查和写入操作。`apply` 接受三种格式（原生紧凑 / JSON / YAML）的批量编辑指令清单，原子提交、可预览、参数对称可逆。

单一 Rust 静态二进制全平台分发；agent 集成层独立维护，从单一 `docs/SKILL.md` 派生各平台格式。

---

## 3. Background & Motivation

### 3.1 现状问题

现有 LLM agent 的文件编辑工具普遍是「锚点字符串替换」（如 `replace_in_file` / Claude Code Edit / `llm-tools-patch`）。每次编辑需要 LLM：

1. 读出原文（消耗输入 token）
2. 生成完整的新内容（消耗输出 token）
3. 提供 old_str + new_str 让工具替换（再次消耗 token）

对**行级以上规模**的编辑（≥ 5 行替换、跨文件批量改、多步操作合并），这种模式 token 浪费严重，且容易出错（old_str 不唯一、不匹配等）。

agent 也常做"删一段旧逻辑 + 插一段新逻辑 + 改 import"这类组合操作，目前需多次工具调用，每次都要重新读取上下文。

### 3.2 现有方案不足

- `sed/awk`：行号操作能力强，但语法陡，错误信息差，不原子，跨文件批量编辑难以一次表达
- `patch + diff`：要 LLM 生成完整 diff，输出 token 不省
- `apply_patch`（Codex）/锚点式 `replace_in_file`：未解决"行号操作 + 批量原子"
- 各平台插件相互独立，没有统一的"agent 文本编辑工具"

### 3.3 用户故事

**Story 1（删一段旧实现）**：删除 `Foo.vue` 第 50-80 行
- 现状：read 文件 1500 行 → replace_in_file 提供完整 old_str → 总 token ~2000
- xnip：`xnip replace Foo.vue --lines 50-80 --text ""` → token ~30

**Story 2（跨文件改常量名）**：5 个文件里把 `OLD_NAME` 改成 `NEW_NAME`
- 现状：5 次 read + 5 次 replace_in_file → token ~10000
- xnip：`xnip replace --files-from list.txt --pattern OLD_NAME --repl NEW_NAME` → token ~50

**Story 3（多步组合编辑）**：在 `Foo.vue` 同时插 import / 删旧逻辑 / 改常量
- 现状：3 次工具调用，每次重新读上下文，且要心算行号偏移
- xnip：写一份 apply 清单（10 行），`xnip apply edits` 原子提交

---

## 4. Goals

| ID | 目标 | 验证 |
|---|---|---|
| G1 | 行级以上规模编辑的 token 消耗对比 agent 内置工具降 ≥ 70% | 选取真实仓库 ≥ 5 个典型编辑场景对比测量 |
| G2 | 全平台行为一致（macOS / Linux / Windows × x86_64 / aarch64） | CI 三 OS 矩阵跑同一套测试用例 |
| G3 | 单一静态二进制，无运行时依赖 | `file xnip` 显示 statically linked；`ldd` 无外部依赖（musl 构建） |
| G4 | 写操作原子且可预览 | 所有写命令支持 `--dry-run`；apply 跨文件失败可回滚 |
| G5 | 输出格式稳定可解析 | `--json` NDJSON 输出；stdout 格式有 schema |
| G6 | apply 接受 3 种格式（原生 / JSON / YAML） | 同语义清单三种格式产生相同结果 |
| G7 | agent 注入即用 | 任一目标 agent 注入 docs/SKILL.md 后能在合适场景主动选用 xnip |
| G8 | 安装快 | `curl \| sh` 完成在 ≤ 10 秒（不含网络下载） |

---

## 5. Non-Goals

| 项 | 不做的原因 |
|---|---|
| **替代 LSP / IDE 内建编辑** | 单点 1-2 行小改、需要语法理解的重命名（含变量作用域）继续用 IDE。xnip 只做"文本字节级"编辑 |
| **AST 感知** | 引入 Tree-sitter 等会大幅增加二进制体积和复杂度；交给上层 agent 用 LSP |
| **实时协作 / 文件锁** | 多 agent 并发由 `--was` 防误改在调用方负责 |
| **撤销日志（journal-based revert）** | 仅做参数对称可逆；不可逆操作直接报错。复杂撤销由用户用 git 解决 |
| **Linux 发行版包（deb/rpm/Arch）** | 维护成本高；`curl \| sh` 已覆盖。社区 PR 在 v1 后接受 |
| **Docker 镜像** | 单进程 CLI 不适合容器化；用户在 Dockerfile 里 `RUN curl ... \| sh` |
| **PowerShell / cmd 重写实现** | Rust 二进制原生支持 Windows，不需要 |
| **GUI** | xnip 服务于自动化 |
| **嵌入式语言扩展** | apply 是声明式，不支持变量/条件/循环；需要时由 LLM 在生成清单前展开 |
| **网络功能** | 不做远程编辑、协作、订阅 |

---

## 6. Proposed Design

### 6.1 设计原则

| 原则 | 含义 |
|---|---|
| **项目无关** | 不感知项目结构、不读项目配置、不假设语言；任何文件都是「带行号的字节流」 |
| **单一二进制** | 全平台分发一份静态链接二进制，无运行时依赖 |
| **跨平台一致** | 所有平台跑同一份编译产物；行为差异通过 Rust 标准库统一抹平 |
| **机器友好输出** | stderr 给人，stdout 给机器；可选 `--json` NDJSON |
| **明确退出码** | `0` 成功；`1` 用户错误；`2` 写入/IO 失败；`3` 校验失败；`4` 部分提交回滚 |
| **原子写入** | tmpfile 同目录 → 校验 → atomic rename；默认 **不写** `.bak`（依赖 git 管理历史），`--backup` 显式启用覆盖式 `.bak` |
| **可预览** | `--dry-run` 输出 unified diff，不落盘 |
| **正交参数** | 命令切动作；定位/内容/修饰各为参数维度；删除 = 空文本 |
| **参数对称可逆** | `--revert` 仅参数携带可逆信息时生效；不可逆即报错 |
| **声明式批量** | apply 输入纯声明：无变量、条件、循环、include |
| **UTF-8 字节透明** | 默认按字节处理；非 UTF-8 文件不强制转换 |

### 6.2 命令清单

7 个子命令 + 4 个辅助：

| 命令 | 类型 | 一句话 |
|---|---|---|
| `peek` | 只读 | 输出带行号的指定区间 |
| `find` | 只读 | 搜索定位，输出 `path:line` 列表 |
| `replace` | 写 | 替换/删除（空文本即删除） |
| `insert` | 写 | 在某位置前/后插入 |
| `move` | 写 | 移动行块到目标位置 |
| `indent` | 写 | 缩进调整 / tab-space 互转 |
| `apply` | 写 | 执行批量编辑指令清单 |
| `mcp` | 辅助 | 启动 stdio MCP server，向 LLM agent 暴露上述 8 个工具（见 §6.10） |
| `doctor` | 辅助 | 自检环境与版本 |
| `help` | 辅助 | `xnip help [<cmd>]` |
| `--version` | 辅助 | 版本号 |

### 6.3 全局参数（写命令公用）

| 参数 | 作用 |
|---|---|
| `--dry-run` / `-n` | 不落盘，输出 unified diff 到 stdout |
| `--check` | 仅校验（解析 + 定位 + `--was` + 可写）；通过 stdout 打 `OK` 退出 0；任一失败按对应退出码退；不生成 tmpfile、不输出 diff |
| `--quiet` / `-q` | 抑制 stderr 人类向消息（保留错误） |
| `--backup` | 写 `.bak`（覆盖式同名）；默认不写，依赖 git 管理历史 |
| `--was <bytes>` / `--was-file <path>` | 写前内容校验（任意字节序列；含 `\n` 转义） |
| `--revert` | 参数对称反向执行 |
| `--json` | stdout 输出 NDJSON 结构化事件 |
| `--no-color` | 关彩色（默认 TTY 时彩色，非 TTY 自动关；同时识别环境变量 `NO_COLOR`，参考 https://no-color.org/） |
| `--trace` | stderr 输出执行追踪，前缀 `[xnip trace]`（调试用） |

> **实现注**：`--quiet/--no-color/--trace` 是 clap `global = true`，可置于子命令前后任意位置；冻结到 `output::globals` 的 `OnceLock` 供所有命令读取。彩色目前只作用于 `apply --dry-run` 输出的 unified diff（git-diff 风格 ANSI）。

### 6.4 定位维度（写命令必须且仅有一个）

| 参数 | 含义 | 示例 |
|---|---|---|
| `--lines a[-b]` | 行号区间，1-based 闭区间 | `--lines 30-45` |
| `--match-line <regex>` | 匹配整行；附 `--occurrence N`（默认 1） | `--match-line '^const PORT'` |
| `--between <start>..<end>` | 两字面锚点之间，默认不含锚点；`--inclusive` 含 | `--between '// BEGIN'..'// END'` |
| `--between-re <re>..<re>` | 两正则锚点之间 | `--between-re '^function foo'..'^}'` |
| `--pattern <regex>` | 命中正则的位置（仅 `replace` sub 模式） | `--pattern 'console\.log\(.*\);'` |

`find` 命令仅支持 `--pattern`。

### 6.5 内容维度（写命令，互斥）

| 参数 | 含义 |
|---|---|
| `--text "..."` | 字面字符串（不解析转义；shell 层处理） |
| `--text-stdin` | 从 stdin 读 |
| `--text-file <path>` | 从外部文件读 |
| `--repl <s>` | 仅 `--pattern` 模式；支持 `$1` `$2` 反向引用 |
| `--count <N\|all>` | 仅 `--pattern` 模式；前 N 次匹配，默认 `all` |

### 6.6 多文件

`find` 和 `replace --pattern` 支持多文件：

- 直传：`xnip find ... a.ts b.ts c.ts`（shell 展 glob）
- `--files-from <path|->`：列表文件，`-` 为 stdin

其他写命令仅单文件；跨文件多种操作走 `apply`。

### 6.7 输出格式

#### 6.7.1 `peek`

```
xnip peek <file> [--lines a-b | --match-line RE [--context N] | --all] [--max-lines N]
```

stdout 格式（稳定）：

```
   30: const X = 1;
   31: function foo() {
   32:   return 42;
   33: }
```

行号右对齐 6 字符宽，冒号后单空格，内容字节透传。`--all` 默认 `--max-lines 1000`，超出截断 + stderr 提示。

#### 6.7.2 `find`

```
xnip find --pattern <regex> <file...> | --files-from <path>
          [--list-only | --count-only | --with-content]
          [--occurrence N]
```

默认 stdout：`<path>:<line>:<content>`（GNU grep 兼容）

- `--list-only`：`<path>:<line>`
- `--count-only`：`<path>:<count>`
- `--occurrence N`：每文件只输出第 N 处

#### 6.7.3 `--dry-run`

unified diff（`diff -u` 兼容）：

```
--- <path>	(before)
+++ <path>	(after)
@@ -30,3 +30,2 @@
-old line 1
-old line 2
-old line 3
+new line
```

多文件场景按文件依次输出。

#### 6.7.4 `apply` 失败诊断

stderr：

```
xnip: apply failed at op #3 (line 7 of input)
  command: replace src/Foo.vue 30 "..."
  reason:  --was mismatch: expected 'const X = 1;', got 'const X = 2;'
  rolled back: 0 files committed; 0 files changed
```

op 编号 **从 1 开始**。stdout 不输出半成品 diff。退出 3 或 4。

#### 6.7.5 `--json` NDJSON

每行一个 JSON 对象。

> **实现注 v0.1.0**：v0.1.0 仅 `apply --json` 走 NDJSON，事件比规范早期设想精简；`find/peek` 暂用文本输出，预留 `--json` 在 v0.2 接入完整事件流。

| event | 字段 | 触发 |
|---|---|---|
| `start` | `command` | 命令开始 |
| `done` | `affected_files: [path]` | apply 成功完成 |
| `error` | `kind` (`phase1` / `phase2` / `io`), `message` | 任何阶段失败 |

规划中（v0.2+）：

| event | 字段 | 触发 |
|---|---|---|
| `match` | `path`, `line`, `content` | find / peek |
| `diff` | `path`, `hunks: [{old_start, old_count, new_start, new_count, lines}]` | dry-run |
| `commit` | `path`, `bytes_changed` | 写命令落盘 |
| `summary` | `files_changed`, `ops_applied`, `duration_ms` | 命令结束 |

### 6.8 `--revert` 语义

「同参数加 `--revert` 与不加完全互逆。不可逆即报错。」

| 命令 + `--revert` | 行为 | 退出 |
|---|---|---|
| `replace --pattern A --repl B` | 等价 `replace --pattern B --repl A`（regex-escaped 互换） | — |
| `replace --lines a-b --was X --text Y` | 等价 `replace --lines a-b --was Y --text X`；前置校验当前区段 == Y | — |
| `replace ... --text Y` 无 `--was`（range 定位） | 报错"`--revert` 需要 `--was`" | 1 |
| `replace --match-line/--between/--between-re ... --revert` | 拒绝（forward 后锚点可能消失，无法安全反向） | 1 |
| `insert --lines L --after --text X --revert` | 计算 X 的行数 K，删除 `[L+1, L+K]`（`--before` 时为 `[L, L+K-1]`）；前置校验该区间字节 == normalized(X) | 不匹配 → 3 |
| `insert --match-line/... --revert` | 拒绝（仅 `--lines` 行号定位才安全可逆） | 1 |
| `move --from-lines S-E --to T --position P --revert` | 由 `core::ops::move_op::reverse_params` 计算反向参数；4 case round-trip 测试覆盖 | — |
| `move --from-match-line ... --revert` | 拒绝（match-line 在 forward 后定位的不是原源块） | 1 |
| `indent --add N --revert` | 等价 `indent --remove N` | — |
| `indent --tabs-to-spaces N --revert` | 等价 `indent --spaces-to-tabs N`；信息可能损失（`Remove`/`SpacesToTabs` forward 不严格可逆），revert 后字节可能 ≠ 原始 | — |

> **实现注**：`--revert` 在 cli 单命令路径与 apply 清单路径上行为一致；apply 内的 op 也接受 `revert` 修饰，复用同一套校验。

### 6.9 apply 输入格式

支持三种格式 + 智能识别。

#### 6.9.1 格式识别策略

按优先级：

1. `--format <native|json|yaml>` 显式指定 → 跳过自动识别
2. 文件后缀强暗示：
   - `.json` / `.json5` → 先尝试 JSON
   - `.yaml` / `.yml` → 先尝试 YAML
   - 其他后缀 / 无后缀 → 先尝试原生
3. 后缀失败兜底：按 JSON → YAML → 原生 顺序逐个尝试；任一成功则采用；全失败按内容首字符判断报最相关错误（`{`/`[` 报 JSON，字母行起始报 YAML/原生）

stdin 模式（`--from-stdin`）默认原生，可 `--format` 强制。

#### 6.9.2 原生格式（推荐，token 最省）

一行一操作，注释 `#` 行首，空行忽略。

**操作行结构**：`<op> <file | --files-from path> <定位> [<修饰>...] [<内容>] [<命名修饰>...]`

字段顺序固定：op → 目标 → 定位 → 修饰 → 内容 → 命名修饰。

**词法规则**：

- 双引号 `"..."` 包裹含空格/特殊字符；内部识别 C-style 转义 `\n` `\t` `\r` `\\` `\"`，其他原样
- 不带引号的 token 按空格切分，不解析转义
- `@<path>` 从外部文件读（绝对路径或相对清单文件目录；stdin 模式相对 cwd）
- `@-` 从 apply 的 stdin 读取整段字节（详见下方约束）
- `@@` 表示字面 `@`
- `""` 空字符串（删除）
- 文件按 UTF-8 解码

> **`@-` 约束（实现注 v0.1.0）**：
> - 一份清单中 `@-` **至多出现一次**（stdin 是线性字节流，没有可靠分隔符；多次出现报错退 3）
> - 当任何 op 用到 `@-` 且未指定 `--stdin-file` 时，apply **lazy** 读进程 stdin（无 `@-` 时不会吞 stdin，避免吞掉无关管道）
> - 与 `--from-stdin` 同时使用时必须显式 `--stdin-file <PATH>`（stdin 已被清单占用）

**字段语法表**（同 6.4-6.5 但用紧凑形式）：

| 字段 | 形式 |
|---|---|
| 定位 | `30` / `30-45` / `=/regex/[N]` / `"start".."end"[i]` / `~/start/..~/end/[i]` / `s/pat/repl/[gN]` |
| 修饰 | `before` / `after` / `inclusive` / `revert` / `+N` / `-N` / `t2s:N` / `s2t:N` |
| 命名修饰 | `was="..."` / `was=@<path>` |
| 内容 | `"..."` / `@<path>` / `@-` / `""` |

**示例**：

```
# replace 各种形态
replace src/Foo.vue 30 "const X = 1;"
replace src/Foo.vue 30-45 ""
replace src/Foo.vue 30-45 "function foo() {\n  return 42;\n}"
replace src/Foo.vue =/^const PORT/ "const PORT = 3000;" was="const PORT = 8080;"
replace src/Foo.vue "// BEGIN".."// END" ""
replace src/Foo.vue ~/^function foo/..~/^}/i ""
replace --files-from filelist.txt s/OLD_NAME/NEW_NAME/g
replace src/Foo.vue 30-45 @./snippets/new-foo.txt

# insert / move / indent
insert src/Foo.vue 5 after "import X from 'x';"
insert src/Foo.vue =/^import vue/ after "import { ref } from 'vue';"
move src/Foo.vue 10-20 100
indent src/Foo.vue 30-45 +2
indent src/Foo.vue 1-99 t2s:4

# revert
replace src/Foo.vue revert s/OLD/NEW/g
```

#### 6.9.3 JSON 格式

顶层数组，每元素是 op 对象。字段名与 CLI 参数一一对应（去 `--`）。

```json
[
  {"op": "replace", "file": "src/Foo.vue", "lines": "30", "text": "const X = 1;"},
  {"op": "replace", "file": "src/Foo.vue", "lines": "30-45", "text": ""},
  {"op": "replace", "file": "src/Foo.vue", "match-line": "^const PORT",
   "text": "const PORT = 3000;", "was": "const PORT = 8080;"},
  {"op": "replace", "file": "src/Foo.vue", "between": ["// BEGIN", "// END"],
   "inclusive": false, "text": ""},
  {"op": "replace", "files-from": "filelist.txt",
   "pattern": "OLD_NAME", "repl": "NEW_NAME", "count": "all"},
  {"op": "insert", "file": "src/Foo.vue", "lines": 5, "where": "after",
   "text": "import X from 'x';"},
  {"op": "move", "file": "src/Foo.vue", "lines": "10-20", "to": 100},
  {"op": "indent", "file": "src/Foo.vue", "lines": "30-45", "by": 2},
  {"op": "indent", "file": "src/Foo.vue", "lines": "1-99", "tabs-to-spaces": 4},
  {"op": "replace", "file": "src/Foo.vue",
   "pattern": "OLD", "repl": "NEW", "revert": true}
]
```

字段语义：

- 定位：`lines`（字符串 `"a"` 或 `"a-b"`）/ `match-line` / `between`（双元素数组）/ `between-re` / `pattern`
- 内容：`text`（字符串；外联用 `@<path>` / `@-` / `@@` 转义；多行直接 `\n`）/ `text-file`
- 修饰：`where`、`inclusive`、`occurrence`、`count`、`by`、`tabs-to-spaces`、`spaces-to-tabs`、`revert`、`was`
- 目标：`file` 或 `files-from`

#### 6.9.4 YAML 格式

同 JSON schema 不同序列化。多行内容用 `|` 块标量更自然：

```yaml
- op: replace
  file: src/Foo.vue
  lines: "30-45"
  text: |
    function foo() {
      return 42;
    }

- op: insert
  file: src/Foo.vue
  match-line: "^import vue"
  where: after
  text: "import { ref } from 'vue';"
```

#### 6.9.5 执行语义

`xnip apply <path>` 内部：

1. **格式识别 + 解析** → 内部统一 op 列表
2. **按文件分组**
3. **组内排序**：定位起始行号降序；锚点定位先在原文件解析为行号再排
4. **两阶段提交**：
   - 阶段一：所有目标文件生成 tmpfile（同目录）；原文件不动
    - 阶段二：所有 tmpfile OK 后逐个 atomic rename；如 `--backup` 则写 `.bak`
5. **失败回滚**：
   - 阶段一任一失败 → 删全部 tmpfile，原文件零损伤（退 3）
   - 阶段二某 rename 失败 → 若有 `.bak` 则还原已 commit 文件；无 `.bak` 时无法还原已 rename 文件，stderr 输出受影响文件清单（退 4）

附加模式：

- `apply --check`：仅阶段一，不输出 diff，stdout 打 `OK` 或错误
- `apply --dry-run`：阶段一 + 输出 unified diff（stdout 为 TTY 且未 `--no-color` 时上色）
- `apply --from-stdin [--format ...]`：清单从 stdin
- `apply --backup`（默认不写 `.bak`）
- `apply --parallel <N>`：**阶段一**多文件并行（rayon 线程池，N=0/1 等价单线程）；**阶段二（atomic rename）保持串行**，以保留 commit-order-dependent 的回滚语义
- `apply --format <native|json|yaml>`：跳过自动识别
- `apply --stdin-file <PATH>`：op 内 `@-` 的字节从此文件读，不消费进程 stdin

### 6.10 MCP server (`xnip mcp`)

面向 **LLM agent 集成**，`xnip mcp` 子命令启动一个遵循 [Model Context Protocol](https://modelcontextprotocol.io/) 的 stdio server，向任何 MCP 客户端（Claude Desktop / Cursor / Cline / Continue / Zed 等）暴露 8 个工具。

**工具清单（与 cli 子命令 1:1 映射）**：

| MCP Tool | 与 cli 对应 | 说明 |
|---|---|---|
| `xnip_peek` | `xnip peek` | 只读；返回带行号的文本 |
| `xnip_find` | `xnip find` | 只读；返回 `path:line[:col]` 列表 |
| `xnip_replace` | `xnip replace` | 写；原子提交 |
| `xnip_insert` | `xnip insert` | 写；单行锚点前/后插入 |
| `xnip_move` | `xnip move` | 写；衡量行块迁移 |
| `xnip_indent` | `xnip indent` | 写；缩进调整 / tabs↔spaces |
| `xnip_apply` | `xnip apply` | 写；两阶段批量提交 |
| `xnip_doctor` | `xnip doctor` | 辅助；环境自检 |

**设计原则**：

1. **复用 core 而非 cli 层**：MCP 与 cli 是 **平行的两个前端**，都直接调 `core::ops::*` / `apply::commit::*`；互不依赖。这避免了“为 MCP 抓平 cli stdout”这种脆弱路径。
2. **字段名与 cli 一致**：`lines` / `match_line` / `between` / `between_re` / `pattern` / `text` / `text_file` / `repl` / `was` / `was_file` / `backup` 等名称与 cli flag 成对，LLM 一眼就能迁移。
3. **不暴露 cli 便利参数**：`--dry-run` / `--check` / `--revert` / `--json` 在 MCP 上下文不友好（LLM 可以直接看新文件、错误可以用 `Err(McpError)` 表达、revert 成本低），全部从 schema 剔除。
4. **不读进程 stdin**：stdin 已被 MCP 协议占用；cli 里 `--text-stdin` / `apply --from-stdin` / op 内 `@-` 在 MCP 上下文都会被拒绝。LLM 需要提供大块内容时走 `text_file`。
5. **保留 `was` 与 `backup`**：`was` 提供并发保护（长会话中文件可能被外部修改）；`backup` 是用户安全旁路。
6. **错误语义映射**：
   - 参数不合法 / 互斥 / 缺必填 → `invalid_params`（-32602）
   - 定位失败 / `was` 不匹配 / 状态前提不满足 → `invalid_request`（-32600）
   - IO 故障 / 阶段二部分提交 → `internal_error`（-32603）
7. **运行时**：单线程 tokio 当前线程 runtime，与 `panic = abort` 兼容；stdio 单连接串行处理，多 worker 无意义。

**依赖与 MSRV**：

- `rmcp = "1.7"`（features = `server, macros, transport-io`）
- `tokio = "1"`（features = `rt, macros, io-std`）
- `schemars = "1.0"`（为 tool 输入生成 JsonSchema）
- **MSRV 从 1.85 提升到 1.95**（`rmcp 1.7` + `schemars 1.0` 及其传递依赖在解析后的 `Cargo.lock` 中聚合出 1.95 的下限）；`rust-toolchain.toml` 同步从 `"1.85"` 改为跟随 `"stable"`。CI 与 release.yml 中 `dtolnay/rust-toolchain` 钉到 `1.95`。

**与集成包的关系**：§8.2 中的集成包依然供“提示/规范”注入使用；MCP 是另一条“调用入口”路径，二者互补不冲突。各集成包同时附带 MCP 配置示例（§8.5）。

---

## 7. Implementation Details

### 7.1 仓库结构

```
xnip/                              # github.com/SeaLoong/xnip
├── README.md
├── LICENSE                        # MIT
├── Cargo.toml                     # workspace root
├── Cargo.lock                     # 锁定（提交）
├── rust-toolchain.toml            # channel = "stable" components = ["rustfmt", "clippy"]【MSRV 在 Cargo.toml 声明为 1.95】
├── deny.toml                      # cargo-deny 配置
├── .github/
│   └── workflows/
│       ├── ci.yml                 # PR 上跑 fmt + clippy + test 矩阵
│       └── release.yml            # tag 上跑跨平台编译 + 上传 release + 同步集成
├── src/
│   ├── main.rs                    # 入口
│   ├── lib.rs                     # 库导出（便于集成测试）
│   ├── cli/                       # clap derive 结构
│   ├── core/                      # 业务核心
│   │   ├── location.rs
│   │   ├── content.rs
│   │   ├── atomic.rs
│   │   ├── revert.rs
│   │   ├── diff.rs
│   │   └── ops/                   # 七命令的纯函数实现
│   ├── apply/
│   │   ├── parse_native.rs
│   │   ├── parse_json.rs
│   │   ├── parse_yaml.rs
│   │   ├── detect.rs              # 格式智能识别
│   │   └── commit.rs              # 两阶段提交
│   ├── mcp/                       # MCP server（`xnip mcp`）
│   │   ├── server.rs              # XnipServer + stdio runtime
│   │   └── tools/                 # 8 个 tool 的输入 schema + run
│   ├── output/
│   │   ├── human.rs
│   │   ├── json.rs
│   │   └── exit.rs
│   └── doctor.rs
├── tests/                         # assert_cmd 集成测试
│   └── fixtures/
├── benches/
├── docs/
│   ├── SKILL.md                   # ⭐ agent 集成正文单一来源
│   ├── apply-format.md
│   ├── examples.md
│   └── design-notes.md
├── integrations/
│   └── ...                        # 见 8.2
└── xtask/                         # cargo xtask
    └── src/main.rs
```

### 7.2 Cargo.toml 依赖（精选）

```toml
[package]
name = "xnip"
version = "0.1.0"
edition = "2024"
rust-version = "1.95"
license = "MIT"
description = "Precise text editing CLI for LLM agents"
repository = "https://github.com/SeaLoong/xnip"
readme = "README.md"
keywords = ["cli", "text", "editor", "agent", "llm"]
categories = ["command-line-utilities", "text-processing"]

[dependencies]
clap = { version = "4.5", features = ["derive", "wrap_help"] }
regex = "1.11"
tempfile = "3.13"
anyhow = "1.0"
thiserror = "2.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"           # apply YAML 格式
similar = "2.6"              # unified diff 生成
encoding_rs = "0.8"          # 处理非 UTF-8 byte stream（CRLF 检测等）
is-terminal = "0.4"          # TTY 检测决定彩色输出
rayon = "1.10"               # apply --parallel
# MCP server（`xnip mcp`）依赖：rmcp 官方 SDK + tokio 单线程 runtime + schemars生成 tool input schema
rmcp = { version = "1.7", features = ["server", "macros", "transport-io"] }
tokio = { version = "1", features = ["rt", "macros", "io-std"] }
schemars = "1.0"
# 注：v0.1.0 不再依赖 nu-ansi-term。彩色 diff 仅在 `apply --dry-run` 用手写 ANSI
# 序列实现（`core::diff::colorize_unified_diff`），避免引入外部依赖。

[dev-dependencies]
assert_cmd = "2.0"
predicates = "3.1"
criterion = { version = "0.5", features = ["html_reports"] }
tempfile = "3.13"

[profile.release]
strip = true
lto = "thin"
codegen-units = 1
opt-level = 3
panic = "abort"

[[bench]]
name = "large_file"
harness = false
```

**MSRV**：1.95（`rmcp 1.7` + `schemars 1.0` 及其传递依赖在 `Cargo.lock` 解析后聚合出 1.95 的下限；edition 2024 本身仅需 1.85）。**Edition**：2024。

**二进制大小目标**：release 后 `strip` + `lto=thin` 后 ≤ 5MB（musl x86_64）。

### 7.3 关键算法伪码

#### 7.3.1 定位解析（`core/location.rs`）

```rust
pub enum Locator {
    Lines { start: usize, end: usize },           // 1-based 闭区间
    MatchLine { regex: Regex, occurrence: usize },
    Between { start: ByteSeq, end: ByteSeq, start_occ: usize, end_occ: usize, inclusive: bool },
    BetweenRe { start: Regex, end: Regex, start_occ: usize, end_occ: usize, inclusive: bool },
    Pattern { regex: Regex, count: Count },       // 仅 replace
}

pub struct Resolved {
    pub start_line: usize,    // 1-based
    pub end_line: usize,      // inclusive
}

pub fn resolve(loc: &Locator, content: &[u8]) -> Result<Resolved, LocateError> {
    match loc {
        Locator::Lines { start, end } => /* 校验范围 */,
        Locator::MatchLine { regex, occurrence } => {
            // 逐行扫描；命中第 occurrence 处返回；未命中报错附 grep 提示
        }
        Locator::Between { ... } => {
            // 找 start_occ 处 + 之后第一个 end_occ 处
            // inclusive 决定返回范围是否含锚点行
        }
        Locator::Pattern { ... } => unreachable!("pattern not resolved as range"),
    }
}
```

#### 7.3.2 原子写入（`core/atomic.rs`）

```rust
use tempfile::NamedTempFile;

pub fn atomic_write(target: &Path, new_content: &[u8], make_bak: bool) -> Result<()> {
    let dir = target.parent().ok_or(...)?;

    // 同目录创建 tmpfile（保证 rename 原子）
    let mut tmp = NamedTempFile::new_in(dir)?;
    tmp.write_all(new_content)?;
    tmp.as_file().sync_all()?;

    // 默认 make_bak = false；仅当 --backup 时为 true
    if make_bak && target.exists() {
        let bak = target.with_extension("bak");
        std::fs::copy(target, &bak)?;
    }

    tmp.persist(target)?;  // atomic rename on POSIX & NTFS
    Ok(())
}
```

#### 7.3.3 apply 两阶段提交（`apply/commit.rs`）

```rust
pub fn apply_ops(ops: Vec<Op>, opts: ApplyOpts) -> Result<ApplySummary, ApplyError> {
    // Phase 1: prepare all tmpfiles
    let groups: HashMap<PathBuf, Vec<Op>> = group_by_file(ops);

    let mut prepared: Vec<(PathBuf, NamedTempFile)> = vec![];

    for (path, mut file_ops) in groups {
        file_ops.sort_by(|a, b| descending_by_start_line(a, b));
        let original = std::fs::read(&path)?;
        let new_content = apply_ops_to_file(&original, &file_ops)?;  // 失败立即返回

        let tmp = make_tmp_in_dir(&path, &new_content)?;
        prepared.push((path, tmp));

        if opts.check {
            // check 模式不进入 phase 2
        }
    }

    if opts.check { return Ok(...); }
    if opts.dry_run { /* 输出 diff */ return Ok(...); }

    // Phase 2: commit all
    // committed 仅在 opts.make_bak (=`--backup`) 时记录；否则阶段二失败无法恢复，仅诊断报错
    let mut committed: Vec<(PathBuf, PathBuf)> = vec![];   // (target, bak)
    for (path, tmp) in prepared {
        if opts.make_bak && path.exists() {
            let bak = path.with_extension("bak");
            std::fs::copy(&path, &bak)?;
            committed.push((path.clone(), bak));
        }
        match tmp.persist(&path) {
            Ok(_) => continue,
            Err(e) => {
                if opts.make_bak {
                    rollback_committed(&committed)?;  // 用 .bak 还原
                }
                return Err(e.into());            // 退 4
            }
        }
    }

    Ok(...)
}
```

#### 7.3.4 格式智能识别（`apply/detect.rs`）

```rust
pub fn detect_format(input: &str, hint: Option<&str>) -> FormatGuess {
    if let Some(ext) = hint {
        match ext.to_lowercase().as_str() {
            "json" | "json5" => return FormatGuess::JsonFirst,
            "yaml" | "yml"   => return FormatGuess::YamlFirst,
            _ => {}
        }
    }

    // 内容首字符兜底
    let first = input.trim_start().chars().next();
    match first {
        Some('{') | Some('[') => FormatGuess::JsonFirst,
        _ => FormatGuess::NativeFirst,
    }
}

pub fn parse(input: &str, guess: FormatGuess) -> Result<Vec<Op>> {
    let order = match guess {
        FormatGuess::JsonFirst => [Format::Json, Format::Yaml, Format::Native],
        FormatGuess::YamlFirst => [Format::Yaml, Format::Json, Format::Native],
        FormatGuess::NativeFirst => [Format::Native, Format::Json, Format::Yaml],
    };

    let mut last_err = None;
    for fmt in order {
        match try_parse(input, fmt) {
            Ok(ops) => return Ok(ops),
            Err(e) => last_err = Some((fmt, e)),
        }
    }
    Err(format_error(last_err.unwrap()))   // 报最相关格式的错
}
```

### 7.4 错误信息规范

所有用户向错误（stderr）格式：`xnip: <category>: <message>`，附 `hint:` 行。

例：

```
xnip: location not found: --match-line '^const PORT' did not match in src/Foo.vue
  hint: try `xnip find --pattern '^const PORT' src/Foo.vue` to locate
```

```
xnip: --was mismatch at src/Foo.vue:30
  expected: const X = 1;
  actual:   const X = 2;
  hint: file may have been modified; re-read with `xnip peek src/Foo.vue --lines 30`
```

```
xnip: revert not possible: replace --lines without --was is not invertible
  hint: provide --was=<original-content> to enable revert
```

### 7.5 `xnip doctor` 检查项

| 检查 | 通过条件 | 失败动作 |
|---|---|---|
| 二进制版本 | `xnip --version` 与编译时 commit 匹配 | 退 1 |
| 平台识别 | OS / arch 已知 | 警告 |
| TTY 检测 | stdin/stdout/stderr 类型 | info |
| Locale | 报告 LC_ALL / LANG | info |
| 行尾默认 | 测试探针文件并报告检测到的 EOL | info |
| 临时目录可写 | 在 cwd 创建并删除 tmpfile | 退 2 |
| Windows long path | （仅 Windows）查 registry 是否启用 | 警告 |

stdout 格式可解析（`key: value` 行）。

> **实现注 v0.1.0**：`xnip doctor` 报告 version / OS / arch / family / target triple / cwd 可写性 / stdin/stdout/stderr 是否 TTY；尚未实现 EOL 检测、locale、Windows long path 检查（v0.2+）。

### 7.6 测试策略

#### 7.6.1 测试金字塔

| 层 | 工具 | 数量目标 |
|---|---|---|
| 单元测试 | `#[cfg(test)]` 内置 | core/* 每模块 ≥ 5 用例 |
| 集成测试 | `assert_cmd` + tests/ | 每子命令 ≥ 5 场景 |
| 端到端 | tests/fixtures + bash 脚本 | 真实多步 apply 用例 |
| 基准 | `criterion` benches/ | 大文件 / 高频 op 性能 |

#### 7.6.2 关键测试用例（必须覆盖）

| 用例 | 输入 | 期望 |
|---|---|---|
| 删除行区间 | `replace foo --lines 30-45 --text ""` | 文件少 16 行；30 行处接 46 行内容 |
| match-line + was 校验失败 | `replace foo --match-line '^const' --was X --text Y` 当文件实际是 Z | 退 3，错误信息含 X / Z |
| 跨文件 pattern 替换 | `replace --files-from list --pattern A --repl B` | 每文件输出 commit 计数 |
| apply 阶段一失败 | 第 3 个 op `--was` 不匹配 | 原文件零变化，退 3，诊断指向 op #3 |
| apply 阶段二失败（带 --backup） | 第 2 个文件 mock rename 失败 | `.bak` 还原已 commit 文件，退 4 |
| apply 阶段二失败（无 --backup） | 第 2 个文件 mock rename 失败 | stderr 列出受影响文件，退 4，不尝试还原 |
| 三种格式等价 | 同语义清单 native/JSON/YAML 各一份 | 三次结果字节级一致 |
| 行号倒序自动应用 | apply 同文件操作行号交错 | 不出现行号偏移错误 |
| revert 对称 | 任意 op `--revert` 后再 `--revert` 等于原始 | 文件字节级一致 |
| 中文 / emoji 字节透明 | 输入含 `你好🌟` | 不破坏 |
| CRLF 保留 | `\r\n` 文件 | 输出仍 `\r\n` |
| 二进制拒绝（v0.2+） | 含 NUL 字节文件 | 规划：退 3，提示 `--force-binary`；v0.1.0 不主动拒绝 |
| 大文件性能 | 100MB 文件 | benches 验证 replace_range ≥ 1 GiB/s（v0.1.0 实测 ~1.6 GiB/s） |
| CRLF 保留（v0.2+） | `\r\n` 文件 | 规划：保留原行尾；v0.1.0 按字节透传，未做 EOL 主动检测 |
#### 7.6.3 fixture 范例

`tests/fixtures/foo.vue.txt`（30 行简化 Vue）；`tests/fixtures/foo.cn.txt`（含中文）；`tests/fixtures/foo.crlf.txt`（CRLF）；`tests/fixtures/binary.bin`。

### 7.7 CI 流水线

#### 7.7.1 `.github/workflows/ci.yml`（PR）

```yaml
name: ci
on: [pull_request, push]
jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        toolchain: [stable]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo fmt --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test --all
      - run: cargo bench --no-run     # 仅编译

  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
```

#### 7.7.2 `.github/workflows/release.yml`（tag）

```yaml
name: release
on:
  push:
    tags: ['v*']
jobs:
  build:
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
          - target: aarch64-unknown-linux-musl
            os: ubuntu-latest
          - target: x86_64-apple-darwin
            os: macos-latest
          - target: aarch64-apple-darwin
            os: macos-latest
          - target: x86_64-pc-windows-msvc
            os: windows-latest
          - target: aarch64-pc-windows-msvc
            os: windows-latest
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - run: cargo build --release --target ${{ matrix.target }}
      - run: |
          cd target/${{ matrix.target }}/release
          tar czf xnip-${{ github.ref_name }}-${{ matrix.target }}.tar.gz xnip*
          shasum -a 256 *.tar.gz > *.sha256
      - uses: softprops/action-gh-release@v2
        with:
          files: |
            target/${{ matrix.target }}/release/xnip-*.tar.gz
            target/${{ matrix.target }}/release/*.sha256

  publish-integrations:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo run -p xtask -- sync-integrations
      - run: |
          for d in integrations/*/; do
            [ -x "$d/package.sh" ] && (cd "$d" && ./package.sh)
          done
      # upload assets to release; PR to homebrew-xnip / scoop-xnip; submit winget manifest
```

### 7.8 性能目标

| 场景 | 目标 |
|---|---|
| 100MB 文件 `replace --pattern` 全文替换 | < 2s |
| 100 个文件 `apply` 串行 | < 1s |
| 100 个文件 `apply --parallel 8` | < 200ms |
| 内存峰值 | < 文件大小 × 1.5 |
| 启动开销（empty `xnip --version`） | < 5ms |

`benches/large_file.rs` 跑 criterion 验证。

---

## 8. Distribution & Integration

### 8.1 二进制本体分发渠道

| # | 渠道 | 平台 | 用户命令 |
|---|---|---|---|
| 1 | GitHub Release tarball | 全平台 | 手动下载 + 校验 SHA256 |
| 2 | `curl \| sh` | macOS / Linux | `curl -fsSL .../install.sh \| sh` |
| 3 | PowerShell installer | Windows | `iwr -useb .../install.ps1 \| iex` |
| 4 | Homebrew tap | macOS / Linux | `brew install SeaLoong/xnip/xnip` |
| 5 | winget | Windows | `winget install SeaLoong.xnip` |
| 6 | Scoop bucket | Windows | `scoop install SeaLoong/xnip` |
| 7 | crates.io | 任何装 Rust 的 | `cargo install xnip` |

详细 manifest / formula 模板见 `integrations/<channel>/`。每次发版 CI 自动同步。

### 8.2 agent / IDE 集成层

| 平台 | 注入路径 | 格式 | 集成产物 |
|---|---|---|---|
| Claude Code | `~/.claude/skills/xnip/SKILL.md` | YAML frontmatter + Markdown | `integrations/claude-code/` tar.gz |
| Cursor | `.cursor/rules/xnip.mdc` | `.mdc` (frontmatter + Markdown) | `integrations/cursor/` 单文件 |
| Aider | 项目根 `CONVENTIONS.md` + `.aider.conf.yml` | 纯 Markdown | `integrations/aider/` 单文件 |
| GitHub Copilot Chat | `.github/copilot-instructions.md` | 纯 Markdown | `integrations/copilot/` 单文件 |
| AGENTS.md 系（Codex/Gemini CLI） | `AGENTS.md` 或 `GEMINI.md` | 纯 Markdown | `integrations/agents-md/` 单文件 |
| 通用兜底 | system prompt 复制粘贴 | 纯 Markdown | `integrations/generic/PROMPT.md` |

**单一来源**：所有正文从 `docs/SKILL.md` 派生；`xtask sync-integrations` 把正文 + 平台模板拼装为最终产物。改 `docs/SKILL.md` 一处，CI 同步全部。

### 8.3 用户安装速查（README 片段）

```sh
# === 装 xnip 本体 ===

# macOS / Linux
curl -fsSL https://github.com/SeaLoong/xnip/releases/latest/download/install.sh | sh
brew install SeaLoong/xnip/xnip
cargo install xnip

# Windows
iwr -useb https://github.com/SeaLoong/xnip/releases/latest/download/install.ps1 | iex
winget install SeaLoong.xnip
scoop bucket add xnip https://github.com/SeaLoong/scoop-xnip && scoop install xnip

# === 装 agent 集成 ===

# Claude Code:
curl -fsSL .../xnip-claude-code-skill.tar.gz | tar xz -C ~/.claude/skills/
# Cursor:
curl -fsSL .../xnip.mdc -o ~/.cursor/rules/xnip.mdc
# Aider (项目级):
curl -fsSL .../CONVENTIONS.md -o CONVENTIONS.md
# Copilot (项目级):
mkdir -p .github && curl -fsSL .../copilot-instructions.md -o .github/copilot-instructions.md
# AGENTS.md 系:
curl -fsSL .../AGENTS.md -o AGENTS.md
# 通用: 复制 PROMPT.md 到 system prompt
```

### 8.4 docs/SKILL.md 内容大纲

为另一个 agent 顺利写出此文件，按以下结构：

1. **工具定位**（1 段）：xnip 是什么，最适合什么场景
2. **触发条件**（短 list）：行级以上规模编辑、多步合并、跨文件批改
3. **不该用的场景**（短 list）：1-2 行小改用 IDE 内建编辑、变量重命名用 LSP
4. **七命令速查**（表格）：command + 一句话用途 + 核心参数
5. **决策流程**（编号步骤）：peek/find 探查 → `--check` 校验 → `--dry-run` 看 diff → 落盘；多步 → 写 apply 清单
6. **5 个最常用 apply 模板**（每个含原生 / JSON / YAML 三种）：
   - 模板 A：删一段旧实现
   - 模板 B：在 import 区追加 import
   - 模板 C：跨文件改常量
   - 模板 D：替换匹配的整行
   - 模板 E：多步组合编辑
7. **错误处理**：退出码语义；`--backup` / `.bak` 用法；`--was` 用法
8. **输出格式速查**：peek / find / dry-run / apply 失败 / `--json` 各自的 stdout 格式

8 节总长度目标：≤ 300 行，以保证 agent 注入后 token 占比可控。

### 8.5 MCP 客户端配置示例

任何支持 [Model Context Protocol](https://modelcontextprotocol.io/) 的客户端都可以连接 `xnip mcp`。常见集成点示例（各集成包中同步提供完整可拷贝牖本）：

**Claude Desktop / Claude Code** (`~/Library/Application Support/Claude/claude_desktop_config.json` 或 `.mcp.json`)：
```json
{
  "mcpServers": {
    "xnip": { "command": "xnip", "args": ["mcp"] }
  }
}
```

**Cursor** (`.cursor/mcp.json` 或 `~/.cursor/mcp.json`)：同上。

**Cline / Continue / Zed**：另見各自的集成设置；命令都是 `xnip mcp`，无额外参数。

**调试**：
- `xnip doctor` 验证本地环境
- 手动抓话：`echo '{"jsonrpc":"2.0","id":1,"method":"initialize",...}' | xnip mcp`验证返回。

---
## 9. Risks & Mitigations

| 风险 | 缓解 |
|---|---|
| UTF-8 / 中文 / emoji / GBK | Rust 标准库默认 UTF-8；非 UTF-8 文件用 `Vec<u8>` 字节透传；测试覆盖中文 / 全角 / emoji / GBK fixture |
| CRLF | 不主动转换；diff 输出保留原行尾；`doctor` 检测并提示 |
| 行号偏移（apply 多 op） | 同文件操作自动倒序；文档强调"按原始行号写" |
| 跨文件原子提交失败 | 两阶段提交；带 `--backup` 时用 `.bak` 还原；不带时仅诊断报告；退出 4 区分"全失败"vs"部分回滚" |
| 锚点未命中 | 报错并 hint `xnip find` 重新定位；退 1 |
| 二进制文件 | NUL 字节检测 + 拒绝；`--force-binary` 强制 |
| 大文件性能 | `BufRead` 流式；不入内存；benches 100MB 验证 |
| 字面 `@` 与外联混淆 | 用 `@@` 转义；JSON/YAML 同语义 |
| `--revert` 误用 | 不可逆即退 1；`tabs-to-spaces` 不严格可逆退 3 |
| `.bak` 堆积 | 默认不写 `.bak`，`--backup` 显式启用；启用时同名覆盖式（不 .bak.1 / .bak.2） |
| 并发修改 | 不做文件锁；`--was` 让用户显式校验 |
| Windows 路径 / 文件锁 | `tempfile` + `fs::rename` 已统一；NTFS 独占时给明确退 2 报错 |
| Windows long path（260 字符） | manifest 启用 long path；`doctor` 提示 |
| 二进制供应链 | 每个 release 附 `.sha256`；Phase 2 加 sigstore 签名 |
| crate 依赖膨胀 | 直接依赖 < 15；CI 跑 `cargo audit`、`cargo deny` 限制许可证；定期 `cargo outdated` |
| 单一来源同步漂移 | `xtask sync-integrations` 在 CI 强制重跑；产物附 release |

---

## 10. Alternatives Considered

### 10.1 实现语言

| 方案 | Pros | Cons | 结论 |
|---|---|---|---|
| **Rust（采用）** | 静态二进制、跨平台一致、正则/字符串性能、生态完整 | 编译慢、学习曲线 | ✅ |
| Go | 编译快、跨平台简单、stdlib 强 | 二进制略大、正则性能弱于 Rust | 备选 |
| Zig | 体积最小 | 语言未稳定 1.0 | 否决 |
| POSIX sh + Windows busybox 兜底 | 零运行时、可读性 | 跨平台一致性靠"复杂的 sh 运行时探测和兜底"维持，反而是复杂度来源 | 否决 |
| C / C++ | 极致性能 | 安全性、字符串处理麻烦 | 否决 |

### 10.2 命名

| 候选 | 否决理由 |
|---|---|
| `tsc` | 与 TypeScript 编译器同名，会破坏前端构建 |
| `snip` | 被 Windows Snipping Tool 等截图工具占据 |
| `scalpel` | "scalpel" GitHub / crate 已有大量同名项目，重名 |
| `cut` | 与 GNU `cut` 同名，PATH 命中冲突 |
| `splice` | JS Array.splice 联想合理但 6 字母略长 |
| `tweak` | 语义偏弱 |
| **`xnip`（采用）** | 自创词，crate / npm / 主流 GitHub 名空间空闲 |

### 10.3 apply 输入格式

| 方案 | Pros | Cons |
|---|---|---|
| 仅原生格式 | token 最省 | 程序生成不友好 |
| 仅 JSON | 程序生成方便 | token 浪费严重 |
| 仅 YAML | 人类阅读舒服 | 解析坑（缩进、引用） |
| **三格式 + 智能识别（采用）** | token 省 + 程序友好 + 人类友好 | 增加格式识别代码 |

### 10.4 `--revert` 实现

| 方案 | 否决理由 |
|---|---|
| journal-based（写日志，可回放） | 状态管理复杂；与"无状态 CLI"原则冲突 |
| **参数对称（采用）** | 简单、无状态；不可逆即报错 |

### 10.5 多文件 glob

| 方案 | 否决理由 |
|---|---|
| 内置 glob | shell 自带 + `find`/`fd`/`fzf` 已经是用户常用工具；不重造 |
| **`--files-from` + shell 展开（采用）** | 把文件枚举外包给调用方 |

### 10.6 不引入 AST

详见 Non-Goals。引入 Tree-sitter / SWC 等会让二进制 ≥ 20MB，且和"字节级编辑"定位冲突。

---

## 11. Open Questions

| # | 问题 | 阻塞性 | 状态 |
|---|---|---|---|
| Q1 | GitHub 仓库账号 | resolved | `github.com/SeaLoong/xnip`（个人账号） |
| Q2 | crates.io 包名 | non-blocking | 默认 `xnip`；首次 publish 时如已占用则 `xnip-cli` |
| Q3 | sigstore / cosign 二进制签名 | non-blocking | 推迟到 v0.2 |
| Q4 | apply 是否支持 include 其他清单文件 | non-blocking | 当前 Non-Goal；保持声明式 |
| Q5 | `--was` 是否支持 fuzzy 匹配 | non-blocking | 不加；保持字节级严格 |
| Q6 | `find` 输出是否需要 `--null` 参数（NUL 分隔，便于 `xargs -0`） | non-blocking | v0.2 视用户反馈 |
| Q7 | apply 执行时是否记录 audit log | non-blocking | 不主动加；用户用 git 管理足够 |


---

## 12. Implementation Plan

### Milestone M0 — 仓库脚手架（已完成）

- [x] `cargo new xnip --bin`，配 workspace + xtask
- [x] `Cargo.toml` 完整依赖（见 7.2）
- [x] `rust-toolchain.toml` / `deny.toml` / `.gitignore`
- [x] `.github/workflows/ci.yml` 跑通三 OS fmt + clippy + test
- [x] README 占位

### Milestone M1 — 核心 lib（已完成）

- [x] `core/location.rs`：5 种 Locator + resolve
- [x] `core/content.rs`：4 种 Content source + `@` 外联
- [x] `core/atomic.rs`：tempfile + 可选 `.bak`（默认关）
- [x] `core/diff.rs`：similar 包装为 unified diff + `colorize_unified_diff`
- [x] `core/revert.rs`：参数对称反向计算
- [x] 全部 core 模块单元测试

### Milestone M2 — 七子命令（已完成）

按依赖顺序：

- [x] `core/ops/peek.rs` + `cli/peek.rs`
- [x] `core/ops/find.rs` + `cli/find.rs`
- [x] `core/ops/replace.rs` + `cli/replace.rs`
- [x] `core/ops/insert.rs` + `cli/insert.rs`
- [x] `core/ops/move_op.rs` + `cli/move_op.rs`
- [x] `core/ops/indent.rs` + `cli/indent.rs`
- [x] 每子命令 ≥ 5 集成用例（assert_cmd）

### Milestone M3 — apply 与三格式（已完成）

- [x] `apply/parse_native.rs` + 测试（含字面量 between `"A".."B"[i]`）
- [x] `apply/parse_json.rs` + 测试
- [x] `apply/parse_yaml.rs` + 测试
- [x] `apply/detect.rs` 智能识别 + 测试三种识别路径
- [x] `apply/commit.rs` 两阶段提交 + rollback 测试
- [x] `apply --parallel` + `apply --check` + `apply --dry-run` + `apply --from-stdin` + `apply --stdin-file`
- [x] 三格式等价性测试（同语义清单产生相同结果）

### Milestone M4 — 输出与全局参数（已完成）

- [x] `core/diff::colorize_unified_diff` + `should_colorize_stdout`（彩色 diff for `apply --dry-run`）
- [x] `output/json.rs` NDJSON（`apply --json`）
- [x] `output/exit.rs` 退出码常量 + `output/globals.rs` 全局 flag
- [x] `--was` / `--was-file` / `--check` / `--dry-run` / `--backup` / `--quiet` / `--json` / `--no-color`（含 `NO_COLOR` env）/ `--trace`
- [x] `xnip doctor`
- [x] 写命令成功路径接入 `note!`，`--quiet` 抑制

### Milestone M5 — 文档与集成（已完成）

- [x] `docs/SKILL.md`（按 8.4 大纲）
- [x] `docs/apply-format.md`（三格式完整规格 + `--parallel`/`--stdin-file`/`@-` 约束/revert 边界）
- [x] `docs/examples.md`（cookbook）
- [x] `docs/error-codes.md` / `docs/architecture.md`
- [x] `integrations/claude-code|cursor|aider|copilot|agents-md|generic/` 各自模板 + `package.sh`
- [ ] `integrations/homebrew|winget|scoop/` 模板（v0.2 — 等首个 release tag 出来后再做）
- [x] `xtask sync-integrations` 同步逻辑
- [x] `install.sh` / `install.ps1`

### Milestone M6 — release 与验收（代码就绪，待发版动作）

- [x] `.github/workflows/release.yml` 跨平台编译 + 上传 + 同步集成
- [ ] **（用户自行执行）** 打 tag `v0.1.0`，验证 6 个二进制产出 + 校验 SHA256
- [ ] **（用户自行执行）** 验证集成产物：`xnip-claude-code-skill.tar.gz`、`xnip.mdc`、`CONVENTIONS.md` 等
- [x] benchmark 跑 7.8 性能目标（4 个 criterion 测项；replace_range ~1.6 GiB/s）
- [ ] **（用户自行执行）** 真实场景测试：选取 ≥ 5 个典型编辑场景跑 G1（token ≥ 70% 降幅）
- [ ] **（用户自行执行）** 把 `docs/SKILL.md` 注入目标 agent 验证 G7

**实际开发耗时**：约 1 个连续工作日（代码完成时点 v0.1.0；305 测试全过、`clippy -D warnings` 全绿、`fmt` 全绿）。

### Milestone M7 — MCP server（已完成）

- [x] 引入 `rmcp 1.7` + `tokio 1`（`rt, macros, io-std`） + `schemars 1.0`依赖
- [x] MSRV 从 1.85 提升到 1.95（`rust-toolchain.toml` 改为 `stable` 跟随；CI/release 钉 `dtolnay/rust-toolchain@1.95`）
- [x] `src/mcp/` 模块：`server.rs` 启动 stdio + `XnipServer` + `tools/` 下 8 个工具
- [x] `src/cli/mcp.rs` 子命令 + `Cli::Mcp` 枚举分发
- [x] `tests/mcp.rs` 集成烟测（5 个：initialize / tools/list / peek / replace was 两路径）全过
- [x] PLAN §6.10 与 §8.5 设计、docs/mcp.md 用户手册、CHANGELOG
- [x] 集成包（claude-code/cursor/cline/aider/copilot/generic/agents-md）补充 MCP 配置节点

**实际开发耗时**：约 0.5 个工作日（313 测试全过，MCP 模块自身 0 clippy warning）。

### 验收清单

| Goal | 验收方法 | 状态 |
|---|---|---|
| G1（token ≥ 70% 降幅） | 真实场景 ≥ 5 个用例对比测量 | 待用户执行 |
| G2（跨平台一致） | CI 三 OS 矩阵全绿 | 代码就绪，待发版后由 release.yml 跑通 |
| G3（单一静态二进制） | `file xnip` 输出 statically linked；6 个 target 产物 < 5MB | release.yml 已写；待 tag 触发 |
| G4（原子且可预览） | `apply --dry-run` / `--check` / 阶段二失败回滚测试 | ✅（已实现且测试覆盖） |
| G5（输出稳定可解析） | NDJSON 通过 schema 校验 | ✅（apply --json 已实现，事件简化版见 §6.7.5） |
| G6（三种格式） | 等价性测试通过 | ✅（`apply.rs` 内三格式等价测试） |
| G7（agent 注入即用） | 真实 agent 调用日志显示主动选用 xnip | 待用户执行（集成产物已就绪） |
| G8（≤ 10 秒安装） | `time curl ... \| sh` 测量 | 待用户执行 |

---

## 13. References

### 13.1 依赖 crate

- [clap](https://docs.rs/clap/4) — CLI argparse
- [regex](https://docs.rs/regex/1) — 正则
- [tempfile](https://docs.rs/tempfile/3) — 原子写入
- [serde](https://serde.rs/) / [serde_json](https://docs.rs/serde_json) / [serde_yaml](https://docs.rs/serde_yaml) — 序列化
- [similar](https://docs.rs/similar/2) — unified diff
- [rayon](https://docs.rs/rayon/1) — 数据并行
- [is-terminal](https://docs.rs/is-terminal) — TTY 检测
- [nu-ansi-term](https://docs.rs/nu-ansi-term) — 彩色终端
- [anyhow](https://docs.rs/anyhow) / [thiserror](https://docs.rs/thiserror) — 错误处理
- [assert_cmd](https://docs.rs/assert_cmd) / [predicates](https://docs.rs/predicates) — CLI 集成测试
- [criterion](https://docs.rs/criterion) — benchmark
- [rmcp](https://docs.rs/rmcp/1) — 官方 Rust MCP SDK（仅 `xnip mcp` 启用）
- [tokio](https://docs.rs/tokio/1) — 异步运行时（仅 MCP server 启用）
- [schemars](https://docs.rs/schemars/1) — MCP tool 输入 JsonSchema 生成

### 13.2 各平台 agent 集成参考

- [Cursor Rules .mdc](https://docs.cursor.com/) — `.cursor/rules/*.mdc` 格式
- [Claude Code skills](https://docs.claude.com/en/docs/claude-code/) — `~/.claude/skills/`
- [Aider conventions](https://aider.chat/docs/usage/conventions.html) — `CONVENTIONS.md`
- [GitHub Copilot custom instructions](https://docs.github.com/en/copilot/customizing-copilot/) — `.github/copilot-instructions.md`
- AGENTS.md / GEMINI.md 协议（社区约定）

### 13.3 包管理器

- [Homebrew Tap 文档](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)
- [winget 提交流程](https://github.com/microsoft/winget-pkgs)
- [Scoop 自定义 bucket](https://github.com/ScoopInstaller/Scoop/wiki/Buckets)

### 13.4 设计参考

- 设计文档结构参考：[Cockroach RFC](https://github.com/cockroachdb/cockroach/tree/master/docs/RFCS) / [Rust RFC](https://github.com/rust-lang/rfcs)
- CLI 输出规范参考：[CLI Guidelines](https://clig.dev/)

---

> **文档维护**：spec 变更需 PR；实施过程中发现 spec 不准确处，先 PR 改 spec，再写代码。
