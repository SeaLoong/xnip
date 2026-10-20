#!/usr/bin/env node
/** Offline JSONL worker for unchanged, pinned DSH text executors and its MCP bridge. */
import fs from "node:fs/promises"
import path from "node:path"
import os from "node:os"
import readline from "node:readline"
import { fileURLToPath, pathToFileURL } from "node:url"
import { CACHE, verifyPrepared } from "./dsh_setup.mjs"

const contexts = new Map()
let loaded
async function load() {
  if (!loaded) loaded = (async () => {
    const metadata = await verifyPrepared()
    const runtime = await import(pathToFileURL(path.join(CACHE, "runtime.mjs")).href)
    return { metadata, runtime }
  })()
  return loaded
}

function optionsOf(request) {
  return {
    root: path.resolve(request.root),
    mode: request.mode ?? "fs-observed",
    xnipExecutable: request.options?.xnipExecutable ?? request.xnipExecutable,
    receipts: request.options?.receipts ?? request.receipts ?? "text",
  }
}

async function fixture(request) {
  if (typeof request.root !== "string" || !path.isAbsolute(request.root)) throw new Error("execute requires an absolute fixture root")
  const options = optionsOf(request)
  if (!(await fs.stat(options.root)).isDirectory()) throw new Error("Fixture root must be an existing directory")
  const key = JSON.stringify(options)
  let value = contexts.get(key)
  let setupElapsedMs = 0
  if (!value) {
    const { runtime } = await load()
    const started = performance.now()
    value = await runtime.createFixture(options)
    setupElapsedMs = performance.now() - started
    contexts.set(key, value)
  }
  return { value, setupElapsedMs }
}

async function close(request = {}) {
  for (const [key, value] of contexts) {
    if (request.root && path.resolve(request.root) !== value.options.root) continue
    if (request.mode && request.mode !== value.options.mode) continue
    contexts.delete(key)
    await value.ctx.fiber.dispose()
  }
}

export async function handle(request) {
  try {
    const { metadata, runtime } = await load()
    if (request.action === "describe") {
      const root = await fs.mkdtemp(path.join(os.tmpdir(), "xnip-dsh-describe-"))
      const descriptions = []
      try {
        for (const mode of runtime.MODES) {
          const xnipExecutable = request.options?.xnipExecutable ?? request.xnipExecutable
          if (mode === "xnip-mcp" && !xnipExecutable) {
            descriptions.push({ mode, status: "requires_xnip_executable", tools: [] })
            continue
          }
          const value = await runtime.createFixture({ root, mode, xnipExecutable, receipts: request.options?.receipts ?? request.receipts ?? "text" })
          try { descriptions.push({ status: "available", ...await runtime.describeFixture(value) }) }
          finally { await value.ctx.fiber.dispose() }
        }
      } finally { await fs.rm(root, { recursive: true, force: true }) }
      return {
        ok: true, status: "available", ...metadata, modes: descriptions,
        context_adapters: [
          "Official Session in memory and a host-owned agent/session identity; no AgentLoop or paid model.",
          "fs-observed and str-observed mount real SandboxPolicy, SandboxedFileSystem and FsPolicy in workspace-write mode.",
          "Bare modes deliberately omit observation policy and filesystem confinement, using unchanged LocalFileSystem.",
          "Text-only tool composition: no image attachments, UI, credentials, telemetry or formatting plugins.",
          "No implicit reads or synthetic fs/observed events. Caller must execute real read/view before observed mutation.",
          "MCP mode uses the official client/bridge and an explicitly supplied real executable; it does not inherit native fs guards.",
          "Canonical value and UI meta are internal layers; model_visible_result comes from actual result.content.",
        ],
      }
    }
    if (request.action === "describe-context") {
      const { value, setupElapsedMs } = await fixture(request)
      return { ok: true, status: "available", ...await runtime.describeFixture(value), setup_elapsed_ms: setupElapsedMs }
    }
    if (request.action === "reset" || request.action === "close") {
      await close(request)
      return { ok: true, closed: true }
    }
    if (request.action !== "execute") return { ok: false, adapter_error: "Unknown action" }
    const { value, setupElapsedMs } = await fixture(request)
    const nativeInput = structuredClone(request.arguments)
    const result = await runtime.executeFixture(value, request.tool, nativeInput)
    return {
      ...result,
      error: result.ok ? undefined : result.model_visible_result,
      native_input: nativeInput, setup_elapsed_ms: setupElapsedMs,
      metadata: {
        backend: "deepseek-harness", commit: metadata.commit, version: metadata.version, mode: value.options.mode,
        execution_level: metadata.execution_level,
        transport: value.options.mode === "xnip-mcp" ? "official DSH MCP bridge over real stdio" : "official DSH ToolRuntime",
        canonical_value_is_not_native_model_content: true,
        full_jsonrpc_envelope_captured: false,
      },
    }
  } catch (error) {
    return {
      ok: false, status: request.action === "describe" ? "unavailable" : undefined,
      adapter_error: error?.stack ?? String(error), error: error?.message ?? String(error),
    }
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  // Keep plugin logs away from the JSONL stream; they are diagnostics, not tool text.
  for (const method of ["log", "info", "debug", "warn", "error"]) {
    console[method] = (...args) => process.stderr.write(args.map(value => typeof value === "string" ? value : JSON.stringify(value)).join(" ") + "\n")
  }
  const lines = readline.createInterface({ input: process.stdin, crlfDelay: Infinity })
  try {
    for await (const line of lines) {
      if (!line.trim()) continue
      let request
      try { request = JSON.parse(line) }
      catch (error) { process.stdout.write(JSON.stringify({ ok: false, adapter_error: "Invalid worker JSON: " + error.message }) + "\n"); continue }
      process.stdout.write(JSON.stringify(await handle(request)) + "\n")
    }
  } finally { await close() }
}
