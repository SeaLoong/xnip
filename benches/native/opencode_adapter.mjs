#!/usr/bin/env node
/**
 * Offline, non-model JSONL executor of OpenCode's unchanged native tools.
 * First use downloads pinned official sources and isolated npm dependencies.
 * Nothing is installed globally; tool bodies and filesystem mutation are native.
 */
import fs from "node:fs/promises"
import path from "node:path"
import { fileURLToPath, pathToFileURL } from "node:url"
import { createHash } from "node:crypto"
import { createRequire } from "node:module"
import { execFileSync } from "node:child_process"
import readline from "node:readline"

const HERE = path.dirname(fileURLToPath(import.meta.url))
const ROOT = path.resolve(HERE, "../..")
const CACHE = path.join(ROOT, "target/native-bench/opencode-v1.19.3")
const VERSION = "1.19.3"
const COMMIT = "e3c4eb242585dcd77be1a867391b7c25fd47ce52"
const BASE = "https://raw.githubusercontent.com/anomalyco/opencode/" + COMMIT + "/"
const SOURCES = [
  ["edit.ts", "packages/opencode/src/tool/edit.ts", "f84d9d242137e1f18ce912188efb7a97b71bf4f255e0a69f16a1fe9d1ff236d4"],
  ["edit.txt", "packages/opencode/src/tool/edit.txt", "4426ccf60241fe41d01bbafc1e7450ea6538003f9fca863ab0210492a74647f8"],
  ["apply_patch.ts", "packages/opencode/src/tool/apply_patch.ts", "9bc5a1384a5e1b1cdd21943cb529dcde6e5e596706dee2111ccdb999d6b0ba22"],
  ["apply_patch.txt", "packages/opencode/src/tool/apply_patch.txt", "2e88f3a8fb30723c4fd6a084ef20716dec6b6ea66aa943dde675e81c5cb1cc4e"],
  ["tool.ts", "packages/opencode/src/tool/tool.ts", "fed2bc482a40aff4fe21a7939c753bc0f36110e5ec0e8dbba1410509d4deaf88"],
  ["fs-util.ts", "packages/core/src/fs-util.ts", "8deaf7c7b809425da81315019b79d9044592eead53a066e6263886a9c11bf860"],
  ["bom.ts", "packages/opencode/src/util/bom.ts", "6384aefb788bf4ac5ed90eb763c695e3209e7ded1d1811ed7eeeb91f805c7586"],
  ["patch.ts", "packages/opencode/src/patch/index.ts", "f7c068803be8db53d5bff07b31bec17c2556d31ab67e43b684a3b53aaa1da403"],
  ["truncate.ts", "packages/opencode/src/tool/truncate.ts", "d3dc9e7402de74a7652ceb9f7796c970efc4958597b51a94fa264697366ccea5"],
  ["external-directory.ts", "packages/opencode/src/tool/external-directory.ts", "b86477bd434aef57caf9fb1d3d72249be37c66e299aa1a864a601e1600e36bda"],
  ["instance-context.ts", "packages/opencode/src/project/instance-context.ts", "9ccfe8c7e39dc085b1e6abc1a4a70d5c4ae97fcbc5447118f17c91e5c4a099c5"],
  ["LICENSE", "LICENSE", "625f0f619133f89bbbb2abe37369613dfa1885eba1e50d02170deb62bb42cb6b"],
  ["upstream-package.json", "package.json", "564e182ea9f962fe4ac148b2c93f6679b38cecd62d81abf6b51f5a2a88e03262"],
  ["effect.patch", "patches/effect@4.0.0-beta.83.patch", "f94650ac08f7d851a27e7c21f544bc5b41f7f91783171211197a72d14d2cda36"],
]
const DEPENDENCIES = {
  "effect": "4.0.0-beta.83",
  "@effect/platform-node": "4.0.0-beta.83",
  "@effect/platform-node-shared": "4.0.0-beta.83",
  "diff": "8.0.2",
  "mime-types": "3.0.1",
  "esbuild": "0.28.2",
}
const CONTEXT_ADAPTERS = [
  "LSP disabled: touchFile is a no-op and diagnostics are empty.",
  "Automatic formatting disabled: Format.file returns false.",
  "Permissions preauthorized by the fixture runner; requests are retained verbatim.",
  "Filesystem watcher/event publications captured without broadcasting.",
  "Agent lookup returns a fixture identity without permissions or models.",
  "Instance directory/worktree are the fixture root; unused glob/context helpers reject use.",
  "Dependency layer registration exposed to the harness; the official FSUtil layer is unchanged.",
  "Truncation storage filename uses a local monotonic identity and temporary directory.",
  "The upstream Effect patch only changes HTTP SSE JSON schema; that unused module is not loaded.",
]
function sha256(bytes) { return createHash("sha256").update(bytes).digest("hex") }
let runtime
let evidence
let unavailable

async function prepare() {
  if (runtime) return
  if (unavailable) throw new Error(unavailable)
  await fs.mkdir(CACHE, { recursive: true })
  const revisions = []
  for (const [local, upstream, expected] of SOURCES) {
    const dest = path.join(CACHE, local)
    let bytes
    try { bytes = await fs.readFile(dest) } catch (error) {
      if (error.code !== "ENOENT") throw error
      let downloadError
      for (let attempt = 0; attempt < 3; attempt++) {
        try {
          const response = await fetch(BASE + upstream, { signal: AbortSignal.timeout(20000) })
          if (!response.ok) throw new Error("Cannot acquire official source " + upstream + ": HTTP " + response.status)
          bytes = Buffer.from(await response.arrayBuffer())
          break
        } catch (error) { downloadError = error }
      }
      if (!bytes) throw new Error("Cannot acquire official source " + upstream + ": " + (downloadError?.cause?.message ?? downloadError?.message ?? String(downloadError)))
      if (sha256(bytes) !== expected) throw new Error("Official download hash mismatch: " + upstream)
      await fs.writeFile(dest, bytes, { flag: "wx" })
    }
    const actual = sha256(bytes)
    if (actual !== expected) throw new Error("Official source cache hash mismatch: " + dest)
    revisions.push({ file: upstream, local_path: dest, source_url: BASE + upstream, sha256: actual, unchanged: true })
  }
  const packagePath = path.join(CACHE, "package.json")
  const wanted = JSON.stringify({ private: true, type: "module", dependencies: DEPENDENCIES, overrides: { "@effect/platform-node-shared": "4.0.0-beta.83" } }, null, 2) + "\n"
  let old = ""
  try { old = await fs.readFile(packagePath, "utf8") } catch {}
  if (old !== wanted) await fs.writeFile(packagePath, wanted)
  const lockedBytes = await fs.readFile(path.join(HERE, "opencode_package-lock.json"))
  const lockPath = path.join(CACHE, "package-lock.json")
  let oldLock
  try { oldLock = await fs.readFile(lockPath) } catch {}
  const lockChanged = !oldLock || !oldLock.equals(lockedBytes)
  if (lockChanged) await fs.writeFile(lockPath, lockedBytes)
  const require = createRequire(packagePath)
  let installed = !lockChanged && old === wanted
  for (const [name, version] of Object.entries(DEPENDENCIES)) {
    try {
      const manifest = JSON.parse(await fs.readFile(path.join(CACHE, "node_modules", name, "package.json"), "utf8"))
      if (manifest.version !== version) installed = false
    } catch { installed = false }
  }
  if (!installed) {
    const npmCli = path.join(path.dirname(process.execPath), "node_modules/npm/bin/npm-cli.js")
    try { await fs.access(npmCli) } catch {
      throw new Error("npm CLI unavailable beside Node; isolated dependencies have not been provisioned")
    }
    execFileSync(process.execPath, [npmCli, "ci", "--prefix", CACHE, "--ignore-scripts", "--no-audit", "--no-fund"], {
      cwd: ROOT, stdio: ["ignore", "pipe", "pipe"], timeout: 180000,
    })
  }
  const esbuild = await import(pathToFileURL(require.resolve("esbuild")).href)
  const external = new Set(["effect", "@effect/platform-node", "diff", "mime-types"])
  const officialAliases = {
    "./tool": "tool.ts", "./truncate": "truncate.ts", "./external-directory": "external-directory.ts",
    "../project/instance-context": "instance-context.ts", "../patch": "patch.ts",
    "../util/bom": "bom.ts", "@/util/bom": "bom.ts", "@opencode-ai/core/fs-util": "fs-util.ts",
    "./edit": "edit.ts", "./index": "patch.ts", ".": "patch.ts", "./edit.txt": "edit.txt", "./apply_patch.txt": "apply_patch.txt",
  }
  const contextImports = new Set([
    "@/lsp/lsp", "@opencode-ai/core/filesystem", "@opencode-ai/core/filesystem/watcher",
    "@/event-v2-bridge", "../format", "@/effect/instance-state", "@/agent/agent",
    "./util/glob", "./effect/service-use", "./effect/app-node", "./effect/app-node-platform",
    "@opencode-ai/core/effect/layer-node", "@/permission/evaluate", "@/config/config",
    "./schema", "./truncation-dir", "@/util/local-context",
  ])
  await esbuild.build({
    entryPoints: [path.join(HERE, "opencode_runtime.ts")], outfile: path.join(CACHE, "runtime.mjs"),
    bundle: true, platform: "node", format: "esm", target: "node24", loader: { ".txt": "text" },
    logLevel: "silent",
    plugins: [{ name: "native-opencode-context", setup(build) {
      build.onResolve({ filter: /.*/ }, args => {
        if (external.has(args.path) || args.path.startsWith("node:")) return { path: args.path, external: true }
        if (args.path.startsWith("native-opencode/")) return { path: path.join(CACHE, args.path.slice("native-opencode/".length) + ".ts") }
        if (args.path === "./opencode_stubs") return { path: path.join(HERE, "opencode_stubs.ts") }
        if (!args.importer.startsWith(CACHE + path.sep)) return
        if (args.path === "path" || args.path === "fs" || args.path === "fs/promises") return { path: args.path, external: true }
        if (officialAliases[args.path]) return { path: path.join(CACHE, officialAliases[args.path]) }
        if (contextImports.has(args.path)) return { path: path.join(HERE, "opencode_stubs.ts") }
        return { errors: [{ text: "Unrecognized official import; refusing to stub: " + args.path }] }
      })
    }}],
  })
  runtime = await import(pathToFileURL(path.join(CACHE, "runtime.mjs")).href)
  evidence = {
    backend: "opencode-edit", version: VERSION, commit: COMMIT,
    package_source: "https://registry.npmjs.org/opencode-ai/" + VERSION,
    execution_level: "official tool executor, not full agent/model",
    native_tool: "EditTool/ApplyPatchTool + Tool validation/output wrapper + FSUtil + Bom + Patch",
    source_revisions: revisions, dependencies: DEPENDENCIES,
    dependency_lock_sha256: sha256(lockedBytes),
    runtime_bundle_sha256: sha256(await fs.readFile(path.join(CACHE, "runtime.mjs"))),
    context_adapters: CONTEXT_ADAPTERS, model_calls: 0, global_configuration_changes: false,
    runtime: { executable: process.execPath, version: process.version },
  }
}

export async function handle(request) {
  try { await prepare() } catch (error) {
    unavailable = error?.message ?? String(error)
    return { ok: false, status: "unavailable", error: unavailable, backend: "opencode-edit", version: VERSION, commit: COMMIT }
  }
  if (request.action === "describe") {
    return { ok: true, status: "available", ...evidence, tools: await runtime.describe() }
  }
  if (request.action !== "execute") return { ok: false, error: "Unknown action", native_input: request }
  const nativeInput = structuredClone(request.arguments)
  const result = await runtime.execute({ ...request, root: path.resolve(request.root), arguments: nativeInput })
  return { ...result, native_input: nativeInput,
    metadata: { ...result.metadata, backend: evidence.backend, version: VERSION, commit: COMMIT,
      execution_level: evidence.execution_level, context_adapters: CONTEXT_ADAPTERS } }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const lines = readline.createInterface({ input: process.stdin, crlfDelay: Infinity })
  for await (const line of lines) {
    if (!line.trim()) continue
    try { process.stdout.write(JSON.stringify(await handle(JSON.parse(line))) + "\n") }
    catch (error) { process.stdout.write(JSON.stringify({ ok: false, error: error?.message ?? String(error) }) + "\n") }
  }
}
