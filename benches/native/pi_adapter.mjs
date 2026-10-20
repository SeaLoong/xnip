#!/usr/bin/env node
/** Native Pi edit/write + its real MCP transport/result adapter, without model calls. */
import fs from "node:fs/promises";
import path from "node:path";
import readline from "node:readline";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { resolveNpmCli } from "./node_runtime.mjs";

export const COMMIT = "fdce48eb404ec014adaf2ac10a3a53bc6b1740c2";
export const VERSION = "1.3.0";
const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "../..");
const CACHE = path.join(ROOT, "target/native-bench/pi-v1.3.0");
const UPSTREAM = path.join(ROOT, "target/native-bench/pi-upstream");
const SOURCE = path.join(UPSTREAM, "packages/coding-agent/src");
const URL = "https://github.com/earendil-works/pi";
const sha = value => createHash("sha256").update(value).digest("hex");
let preparing, runtime, evidence;

async function prepareRuntime() {
  if (runtime) return;
  await fs.mkdir(CACHE, { recursive: true });
  try { await fs.access(path.join(UPSTREAM, ".git")); } catch {
    execFileSync("git", ["-c", "core.longpaths=true", "clone", "--depth", "1", "--filter=blob:none", "--sparse", URL + ".git", UPSTREAM],
      { cwd: ROOT, stdio: ["ignore", "pipe", "pipe"], timeout: 180000 });
  }
  let revision = execFileSync("git", ["-C", UPSTREAM, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
  if (revision !== COMMIT) {
    execFileSync("git", ["-C", UPSTREAM, "fetch", "--depth", "1", "origin", COMMIT], { timeout: 180000, stdio: ["ignore", "pipe", "pipe"] });
    execFileSync("git", ["-C", UPSTREAM, "checkout", "--detach", COMMIT], { stdio: ["ignore", "pipe", "pipe"] });
  }
  execFileSync("git", ["-C", UPSTREAM, "sparse-checkout", "set", "packages/coding-agent", "packages/ai", "packages/agent", "packages/tui"],
    { timeout: 180000, stdio: ["ignore", "pipe", "pipe"] });
  const changed = execFileSync("git", ["-C", UPSTREAM, "diff", "HEAD", "--", "packages/coding-agent/src", "packages/ai/src", "packages/tui/src", "packages/agent/src"]);
  if (changed.length) throw new Error("Refusing modified official Pi executor source");
  const manifest = await fs.readFile(path.join(HERE, "pi_package.json"));
  const lock = await fs.readFile(path.join(HERE, "pi_package-lock.json"));
  let installed = false;
  try {
    installed = (await fs.readFile(path.join(CACHE, "package-lock.json"))).equals(lock)
      && (await fs.readFile(path.join(CACHE, "package.json"))).equals(manifest)
      && JSON.parse(await fs.readFile(path.join(CACHE, "node_modules/@earendil-works/pi-coding-agent/package.json"), "utf8")).version === VERSION;
  } catch {}
  if (!installed) {
    await fs.writeFile(path.join(CACHE, "package.json"), manifest);
    await fs.writeFile(path.join(CACHE, "package-lock.json"), lock);
    const npmCli = resolveNpmCli();
    execFileSync(process.execPath, [npmCli, "ci", "--prefix", CACHE, "--ignore-scripts", "--no-audit", "--no-fund"],
      { cwd: ROOT, stdio: ["ignore", "pipe", "pipe"], timeout: 240000 });
  }
  const require = createRequire(path.join(CACHE, "package.json"));
  const { build } = await import(pathToFileURL(require.resolve("esbuild")).href);
  const aliases = {
    "native-pi-validation": path.join(UPSTREAM, "packages/ai/src/utils/validation.ts"),
    "native-pi-package-manager": path.join(SOURCE, "core/package-manager.ts"),
    "native-pi-settings": path.join(SOURCE, "core/settings-manager.ts"),
    "native-pi-loader": path.join(SOURCE, "core/extensions/loader.ts"),
    "native-pi-event-bus": path.join(SOURCE, "core/event-bus.ts"),
    "native-pi-mcp-runtime": path.join(SOURCE, "extensions/mcp/runtime.ts"),
    "native-pi-mcp-tools": path.join(SOURCE, "extensions/mcp/tools.ts"),
  };
  const outfile = path.join(CACHE, "runtime.mjs");
  const result = await build({
    absWorkingDir: ROOT, entryPoints: [path.join(HERE, "pi_runtime.ts")], outfile, bundle: true, platform: "node", format: "esm",
    target: "node24", packages: "external", nodePaths: [path.join(CACHE, "node_modules")],
    metafile: true, logLevel: "silent",
    plugins: [{ name: "pinned-pi-sources", setup(build) {
      // These dynamic JIT helpers belong to interactive module loading and are
      // never used by loadExtensionFromFactory. Keep actual published helpers
      // lazy rather than pulling the entire model/provider application into a
      // no-model editing kernel.
      build.onResolve({ filter: /^\.\/(virtual-modules|jiti-static-loader|jiti-loader)\.ts$/ }, args => {
        if (path.basename(args.importer) !== "loader.ts") return;
        return { path: path.join(CACHE, "node_modules/@earendil-works/pi-coding-agent/dist/core/extensions",
          path.basename(args.path).replace(/\.ts$/, ".js")), external: true };
      });
      build.onResolve({ filter: /^native-pi/ }, args => ({
        path: aliases[args.path] ?? path.join(SOURCE, "core/tools", args.path.slice("native-pi/".length) + ".ts"),

      }));
    } }],
  });
  const revisions = [];
  for (const input of Object.keys(result.metafile.inputs)) {
    const absolute = path.resolve(ROOT, input);
    if (!absolute.startsWith(UPSTREAM + path.sep)) continue;
    const file = path.relative(UPSTREAM, absolute).split(path.sep).join("/");
    revisions.push({ file, sha256: sha(await fs.readFile(absolute)), source_url: URL + "/blob/" + COMMIT + "/" + file, unchanged: true });
  }
  runtime = await import(pathToFileURL(outfile).href + "?hash=" + sha(await fs.readFile(outfile)));
  evidence = {
    backend: "pi", version: VERSION, commit: COMMIT, source_revisions: revisions,
    dependency_lock_sha256: sha(lock), runtime_bundle_sha256: sha(await fs.readFile(outfile)),
    execution_level: "official Pi tool executor with local IO; no AgentLoop or model",
    native_tools: ["createEditTool", "createWriteTool", "withFileMutationQueue", "validateToolArguments"],
    contracts: { edit: "original-file coordinates; unique non-overlapping edits; native fuzzy/BOM/EOL behavior",
      write: "whole-file UTF-8 create/overwrite; no supplied-old guard",
      mcp: "real Pi extension registration, SDK stdio connection and createMcpToolDefinition projection" },
    context_adapters: [
      "Fixture root supplies cwd, rather than an interactive session.",
      "No LLM, UI renderCall/renderResult, formatter, telemetry or AgentLoop is constructed.",
      "Native schema validation, argument preparation, content/details result, matching, queue and filesystem IO are unchanged.",
      "MCP lifecycle fixture invokes actual registered session handlers with a cwd-only context; full Pi hooks/permissions are not simulated.",
    ],
    model_calls: 0, global_configuration_changes: false,
    runtime: { executable: process.execPath, version: process.version },
  };
}
export async function prepare() {
  preparing ??= prepareRuntime();
  return preparing;
}
export async function handle(request) {
  try {
    await prepare();
    if (request.action === "reset") { await runtime.reset(); return { ok: true }; }
    if (request.action === "resolve-package") return { ok: true, resources: await runtime.resolvePackage({ ...request, root: path.resolve(request.root) }) };
    if (request.action === "lifecycle") return { ok: true, ...await runtime.lifecycle({ ...request, root: path.resolve(request.root) }) };
    if (request.action === "describe") {
      if (request.mode === "xnip-mcp") {
        return { ok: true, status: "available", ...evidence, mode: "xnip-mcp",
          ...await runtime.describeBridge({ ...request, root: path.resolve(request.root) }) };
      }
      return { ok: true, status: "available", ...evidence, tools: runtime.describe(ROOT) };
    }
    if (request.action !== "execute") throw new Error("Unknown action: " + request.action);
    const input = { ...request, root: path.resolve(request.root), arguments: structuredClone(request.arguments) };
    const result = request.mode === "xnip-mcp" ? await runtime.executeBridge(input) : await runtime.execute(input);
    return { ...result, native_input: request.arguments, metadata: { ...result.metadata,
      backend: request.mode === "xnip-mcp" ? "pi-xnip-mcp" : "pi", version: VERSION, commit: COMMIT,
      execution_level: evidence.execution_level, context_adapters: evidence.context_adapters } };
  } catch (error) {
    return { ok: false, status: runtime ? "error" : "unavailable", adapter_error: error?.message ?? String(error),
      error: error?.message ?? String(error), backend: "pi",
      version: VERSION, commit: COMMIT };
  }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const lines = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  try {
    for await (const line of lines) {
      if (!line.trim()) continue;
      let response;
      try { response = await handle(JSON.parse(line)); }
      catch (error) { response = { ok: false, error: error?.message ?? String(error) }; }
      process.stdout.write(JSON.stringify(response) + "\n");
    }
  } finally { await runtime?.reset(); }
}
