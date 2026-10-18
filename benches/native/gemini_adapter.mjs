#!/usr/bin/env node
// Runs the installed, unmodified Gemini EditTool. Only its surrounding config is adapted.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import readline from "node:readline";
import util from "node:util";
import { pathToFileURL } from "node:url";
import net from "node:net";
import tls from "node:tls";
import http from "node:http";
import https from "node:https";

let diagnostics = [];
for (const level of ["log", "warn", "error", "debug", "info"]) {
  console[level] = (...args) => diagnostics.push({ level, text: util.format(...args) });
}
const blockedNetwork = [];
function denyNetwork() {
  blockedNetwork.push("network access blocked by offline benchmark");
  throw new Error("OFFLINE_BENCHMARK_NETWORK_BLOCKED");
}
net.connect = denyNetwork;
net.createConnection = denyNetwork;
tls.connect = denyNetwork;
http.request = denyNetwork;
http.get = denyNetwork;
https.request = denyNetwork;
https.get = denyNetwork;
globalThis.fetch = denyNetwork;

const arg = process.argv.indexOf("--module-root");
const candidates = [
  arg >= 0 ? process.argv[arg + 1] : process.env.GEMINI_CORE_PATH,
  process.env.APPDATA && path.join(process.env.APPDATA, "npm/node_modules/@google/gemini-cli/node_modules/@google/gemini-cli-core"),
  "/usr/local/lib/node_modules/@google/gemini-cli/node_modules/@google/gemini-cli-core",
].filter(Boolean);
const core = candidates.find((candidate) => fs.existsSync(path.join(candidate, "dist/src/tools/edit.js")));
const digest = (data) => crypto.createHash("sha256").update(data).digest("hex");
let EditTool;
let StandardFileSystemService;
let initializationError;
let metadata;
const started = performance.now();
try {
  if (!core) throw new Error("installed @google/gemini-cli-core not found; set GEMINI_CORE_PATH");
  ({ EditTool } = await import(pathToFileURL(path.join(core, "dist/src/tools/edit.js")).href));
  ({ StandardFileSystemService } = await import(pathToFileURL(path.join(core, "dist/src/services/fileSystemService.js")).href));
  const manifest = JSON.parse(fs.readFileSync(path.join(core, "package.json"), "utf8"));
  const inputs = ["package.json", "dist/src/tools/edit.js", "dist/src/tools/tools.js",
    "dist/src/services/fileSystemService.js", "dist/src/utils/textUtils.js",
    "dist/src/utils/llm-edit-fixer.js", "dist/src/tools/diffOptions.js"];
  metadata = {
    status: "available",
    backend: "gemini-edit",
    execution_level: "native_tool_executor",
    version: manifest.version,
    module_root: path.resolve(core),
    entry: "EditTool.build(arguments).execute(AbortSignal)",
    source: manifest.repository,
    source_files: inputs.map((name) => ({
      path: name, sha256: digest(fs.readFileSync(path.join(core, name))),
    })),
    context_adapters: ["temporary workspace restriction", "official StandardFileSystemService",
      "telemetry disabled", "model requests recorded and blocked", "confirmation outside execution scope"],
    model_calls_performed: 0,
  };
} catch (error) {
  initializationError = String(error.stack ?? error);
  metadata = { status: "unavailable", backend: "gemini-edit", error: initializationError };
}
metadata.worker_startup_ms = performance.now() - started;

function configuration(root, modelRequests) {
  const absolute = path.resolve(root);
  return {
    getTargetDir: () => absolute,
    getFileSystemService: () => new StandardFileSystemService(),
    validatePathAccess: (file) => {
      const relative = path.relative(absolute, path.resolve(file));
      return relative === ".." || relative.startsWith(".." + path.sep) || path.isAbsolute(relative)
        ? "benchmark path is outside the temporary workspace" : null;
    },
    getDisableLLMCorrection: () => false,
    getUsageStatisticsEnabled: () => false,
    getTelemetryEnabled: () => false,
    getTelemetryLogPromptsEnabled: () => false,
    getSessionId: () => "offline-native-benchmark",
    getBaseLlmClient: () => ({
      generateJson: async (request) => {
        // Preserve the exact native repair request, but never invoke an inference provider.
        const { abortSignal, ...serializable } = request;
        modelRequests.push(serializable);
        throw new Error("OFFLINE_BENCHMARK_MODEL_REQUIRED");
      },
    }),
  };
}

async function handle(request) {
  if (request.action === "describe") {
    if (EditTool) {
      const tool = new EditTool(configuration(process.cwd(), []));
      return { ...metadata, schema: tool.schema };
    }
    return metadata;
  }
  if (!EditTool) return { ok: false, unavailable: true, error: initializationError };
  if (request.action !== "execute") throw new Error("unknown adapter action");
  diagnostics = [];
  const modelRequests = [];
  const tool = new EditTool(configuration(request.root, modelRequests));
  const input = structuredClone(request.arguments);
  const before = blockedNetwork.length;
  const time = performance.now();
  try {
    const result = await tool.build(input).execute(new AbortController().signal);
    return {
      ok: !result.error, arguments: input, result,
      model_visible_result: result.llmContent ?? null,
      elapsed_ms: performance.now() - time,
      model_requests_blocked: modelRequests,
      network_requests_blocked: blockedNetwork.length - before,
      diagnostics,
    };
  } catch (error) {
    return {
      ok: false, arguments: input, error: String(error.stack ?? error),
      model_visible_result: null,
      elapsed_ms: performance.now() - time,
      model_requests_blocked: modelRequests,
      network_requests_blocked: blockedNetwork.length - before,
      diagnostics,
    };
  }
}

for await (const line of readline.createInterface({ input: process.stdin, crlfDelay: Infinity })) {
  try {
    process.stdout.write(JSON.stringify(await handle(JSON.parse(line))) + "\n");
  } catch (error) {
    process.stdout.write(JSON.stringify({ ok: false, adapter_error: String(error.stack ?? error) }) + "\n");
  }
}
process.exit(0);
