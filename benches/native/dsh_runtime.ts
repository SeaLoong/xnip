/** Host fixture around unchanged, commit-pinned DSH plugins. No model provider or UI. */
import path from "node:path"
import { Context } from "@deepseek-ai/cordis"
import ToolRuntime from "@deepseek-ai/dsh-tools"
import SystemPrompt, { renderPrompt, renderContextSnapshot } from "@deepseek-ai/dsh-system-prompt"
import LocalFileSystem from "@deepseek-ai/dsh-fs-local"
import SandboxedFileSystem from "@deepseek-ai/dsh-fs-sandbox"
import * as FsPolicy from "@deepseek-ai/dsh-fs-observation-policy"
import * as ToolFs from "@deepseek-ai/dsh-tool-fs"
import * as StrReplaceEditor from "@deepseek-ai/dsh-tool-str-replace-editor"
import SessionProjectionRegistry from "@deepseek-ai/dsh-session-projection"
import SandboxPolicy from "@deepseek-ai/dsh-sandbox-policy"
import WorkingDirectory from "@deepseek-ai/dsh-working-directory"
import { Session, SESSION_FORMAT_VERSION } from "@deepseek-ai/dsh-session"
import * as McpClient from "@deepseek-ai/dsh-mcp-client"

export { Context, ToolRuntime, SystemPrompt, McpClient }
export { loadOptionalPatches, createRuntimeResolution } from "@deepseek-ai/dsh-app-boot"
export { installRuntimeInterception } from "@deepseek-ai/dsh-app-boot/src/profile-resolution/resolver.ts"
export const MODES = ["fs-observed", "fs-bare", "str-observed", "str-bare", "xnip-mcp"] as const
export type Mode = typeof MODES[number]
let nextSession = 0

export async function createFixture(options: { root: string; mode: Mode; xnipExecutable?: string; receipts?: "text" | "structured" }) {
  if (!MODES.includes(options.mode)) throw new Error("Unknown DSH executor mode: " + options.mode)
  if (!path.isAbsolute(options.root)) throw new Error("Fixture root must be absolute")
  const ctx = new Context()
  try {
    await ctx.plugin(SystemPrompt)
    await ctx.plugin(ToolRuntime, { mode: "native" })
    await ctx.plugin(SessionProjectionRegistry)
    const observed = options.mode.endsWith("-observed")
    if (observed) {
      await ctx.plugin(SandboxPolicy, { mode: "workspace-write", workspaceRoot: options.root })
      await ctx.plugin(SandboxedFileSystem, { cwd: options.root })
      await ctx.plugin(FsPolicy)
    } else {
      await ctx.plugin(LocalFileSystem, { cwd: options.root })
    }
    await ctx.plugin(WorkingDirectory, { defaultDirectory: options.root })
    const id = "xnip-benchmark-" + (++nextSession)
    const session = Session.create(id as never, [], {
      version: SESSION_FORMAT_VERSION, id: id as never, createdAt: 0, cwd: options.root, isSeeded: false,
    })
    // The same host-owned session identity as upstream registry integration tests.
    // No AgentLoop, provider, credentials, UI or synthetic fs/observed event.
    const agent = { id, session, options: {}, ctx } as never
    if (options.mode === "xnip-mcp") {
      if (!options.xnipExecutable || !path.isAbsolute(options.xnipExecutable)) throw new Error("xnip-mcp requires an absolute xnipExecutable")
      await ctx.plugin(McpClient, {
        serverName: "xnip", transport: "stdio", command: options.xnipExecutable,
        args: ["mcp", "--receipts", options.receipts ?? "text"], cwd: options.root,
        env: { XNIP_AUTO_UPDATE: "0" }, failOnStartupError: true, reconnect: { enabled: false },
      })
    } else if (options.mode.startsWith("str-")) {
      await ctx.plugin(StrReplaceEditor)
    } else {
      await ctx.plugin(ToolFs, { readImage: false })
    }
    return { ctx, agent, options, nextCall: 0 }
  } catch (error) { await ctx.fiber.dispose(); throw error }
}

export async function describeFixture(fixture: Awaited<ReturnType<typeof createFixture>>) {
  const { ctx, agent } = fixture
  const assembly = await ctx.systemPrompt.assemble({ agent, scope: agent })
  return {
    mode: fixture.options.mode,
    tools: ctx.tools.schemas(agent),
    prompt: renderPrompt(assembly),
    runtime_context: renderContextSnapshot(assembly),
    prompt_assembly: assembly,
    observations: fixture.options.mode.endsWith("-observed") ? "real session read/view required; real version guards and workspace sandbox" : fixture.options.mode === "xnip-mcp" ? "external xnip MCP; native filesystem observation guards and sandbox are not inherited" : "no fs observation policy; local filesystem still real",
    internal_output_boundary: "value and UI meta are retained separately; native model content is result.content",
  }
}

export async function executeFixture(fixture: Awaited<ReturnType<typeof createFixture>>, tool: string, args: unknown) {
  const name = fixture.options.mode === "xnip-mcp" && !tool.startsWith("mcp__") ? "mcp__xnip__" + tool : tool
  const started = performance.now()
  const result = await fixture.ctx.tools.execute({
    signal: new AbortController().signal, callId: ("call-" + (++fixture.nextCall)) as never,
    name, arguments: args, agent: fixture.agent,
  })
  const text = result.content.filter(block => block.type === "text").map(block => block.text).join("")
  return {
    ok: !result.isError, content: result.content, model_visible_result: text,
    value: result.isError ? undefined : result.value,
    meta: result.meta, error: result.isError ? result.error ?? { message: text } : undefined,
    native_result: result, tool_name: name, elapsed_ms: performance.now() - started,
  }
}
