/** Thin fixture context around unchanged pinned Pi executors and MCP bridge. */
import path from "node:path";
import { pathToFileURL } from "node:url";
import { createEditTool, createEditToolDefinition } from "native-pi/edit";
import { createWriteTool, createWriteToolDefinition } from "native-pi/write";
import { validateToolArguments } from "native-pi-validation";
import { createExtensionRuntime, loadExtensionFromFactory } from "native-pi-loader";
import { createEventBus } from "native-pi-event-bus";
import { McpServerConnection, createDefaultTransport, McpOAuthCredentialStore } from "native-pi-mcp-runtime";
import { createMcpToolDefinition, createMcpToolName } from "native-pi-mcp-tools";
import { createXnipExtension } from "../../integrations/pi/index.mjs";
import { DefaultPackageManager } from "native-pi-package-manager";
import { SettingsManager } from "native-pi-settings";

export async function resolvePackage(request: any) {
  const manager = new DefaultPackageManager({ cwd: request.root,
    agentDir: path.join(request.root, ".pi-fixture-agent"), settingsManager: SettingsManager.inMemory() });
  return manager.resolveExtensionSources([path.resolve(request.packageRoot)], { temporary: true });
}

function schema(tool: any) {
  return { name: tool.name, description: tool.description, schema: tool.parameters,
    ...(tool.outputSchema ? { outputSchema: tool.outputSchema } : {}) };
}
function definitions(root: string) {
  return { edit: createEditToolDefinition(root), write: createWriteToolDefinition(root) };
}
export function describe(root: string) {
  const tools = definitions(root);
  return { edit: { ...schema(tools.edit), promptSnippet: tools.edit.promptSnippet,
      promptGuidelines: tools.edit.promptGuidelines },
    write: { ...schema(tools.write), promptSnippet: tools.write.promptSnippet,
      promptGuidelines: tools.write.promptGuidelines } };
}
export async function execute(request: any) {
  const started = performance.now();
  try {
    const tool = request.tool === "write" ? createWriteTool(request.root) : createEditTool(request.root);
    if (!["edit", "write"].includes(request.tool)) throw new Error("Unknown Pi tool: " + request.tool);
    const input = tool.prepareArguments?.(structuredClone(request.arguments)) ?? structuredClone(request.arguments);
    const args = validateToolArguments(tool, { type: "toolCall", id: "native-benchmark", name: tool.name, arguments: input });
    const controller = new AbortController();
    if (request.aborted) controller.abort();
    const result = await tool.execute("native-benchmark", args, controller.signal);
    return { ok: true, content: result.content, model_visible_result: result.content,
      output: result.content.filter((x: any) => x.type === "text").map((x: any) => x.text).join("\n"),
      native_result: result, elapsed_ms: performance.now() - started };
  } catch (error: any) {
    return { ok: false, content: [], output: "", model_visible_result: null,
      error: error?.message ?? String(error),
      native_error: { name: error?.name, message: error?.message }, elapsed_ms: performance.now() - started };
  }
}

let bridge: any;
async function fire(extension: any, event: string, root: string) {
  for (const handler of extension.handlers.get(event) ?? []) {
    await handler({ type: event, ...(event === "session_shutdown" ? { reason: "quit" } : {}) }, { cwd: root });
  }
}
export async function reset() {
  if (!bridge) return;
  const current = bridge;
  bridge = undefined;
  try { await fire(current.extension, "session_shutdown", current.root); }
  finally {
    await current.connection.close();
    current.runtime.invalidate("benchmark session closed");
    current.events.clear();
  }
}
export async function connectBridge(root: string, executable?: string, extensionPath?: string) {
  const entry = extensionPath ? path.resolve(extensionPath) : undefined;
  if (bridge?.root === root && bridge?.executable === executable && bridge?.extensionPath === entry) return bridge;
  await reset();
  const runtime = createExtensionRuntime();
  const events = createEventBus();
  const factory = entry ? (await import(pathToFileURL(entry).href)).default :
    createXnipExtension({ resolveExecutable: () => executable });
  if (typeof factory !== "function") throw new Error("Pi extension entry must export a default factory");
  const extension = await loadExtensionFromFactory(factory, root, events, runtime, entry ?? "<xnip-pi-benchmark>");
  await fire(extension, "session_start", root);
  const registration = runtime.mcpServers.list()[0];
  if (!registration) throw new Error("Real Pi registerMcpServer produced no registration");
  const connection = new McpServerConnection({
    entry: { name: registration.name, config: registration.config, source: "<xnip-extension>", scope: "extension" },
    cwd: root, createTransport: createDefaultTransport,
    credentials: new McpOAuthCredentialStore(), onTools: () => {},
  });
  try {
    await connection.getClient();
    const tools = new Map(connection.tools.map((tool: any) => {
      const name = createMcpToolName("xnip", tool.name);
      return [tool.name, createMcpToolDefinition({
        server: "xnip", tool, name, exposure: "direct", namespace: { name: "xnip" },
        timeoutMs: connection.timeoutMs, getClient: async () => connection,
      })];
    }));
    bridge = { root, executable, extensionPath: entry, runtime, events, extension, connection, tools };
    return bridge;
  } catch (error) {
    await fire(extension, "session_shutdown", root);
    await connection.close();
    runtime.invalidate("connection failed");
    events.clear();
    throw error;
  }
}
export async function describeBridge(request: any) {
  const current = await connectBridge(request.root, request.xnipExecutable, request.extensionPath);
  return { tools: Object.fromEntries([...current.tools].map(([name, tool]: any) => [name, schema(tool)])),
    server_instructions: current.connection.instructions,
    registration: current.runtime.mcpServers.list(),
    state: current.connection.state, exposure: "direct" };
}
export async function executeBridge(request: any) {
  const setup = performance.now();
  const current = await connectBridge(request.root, request.xnipExecutable, request.extensionPath);
  const setupMs = performance.now() - setup;
  const started = performance.now();
  try {
    const tool = current.tools.get(request.tool);
    if (!tool) throw new Error("Unknown xnip MCP tool: " + request.tool);
    const args = validateToolArguments(tool, {
      type: "toolCall", id: "bridge-benchmark", name: tool.name, arguments: structuredClone(request.arguments),
    });
    const controller = new AbortController();
    if (request.aborted) controller.abort();
    const result = await tool.execute("bridge-benchmark", args, controller.signal);
    return { ok: !result.isError, content: result.content, model_visible_result: result.content,
      output: result.content.filter((x: any) => x.type === "text").map((x: any) => x.text).join("\n"),
      native_result: result, setup_elapsed_ms: setupMs, elapsed_ms: performance.now() - started,
      metadata: { public_tool_name: tool.name, bridge_state: current.connection.state } };
  } catch (error: any) {
    return { ok: false, content: [], output: "", model_visible_result: null, error: error?.message ?? String(error),
      setup_elapsed_ms: setupMs, elapsed_ms: performance.now() - started };
  }
}
export async function lifecycle(request: any) {
  const current = await connectBridge(request.root, request.xnipExecutable, request.extensionPath);
  const registered = current.runtime.mcpServers.list().length;
  const tools = [...current.tools.keys()];
  await fire(current.extension, "session_shutdown", current.root);
  await fire(current.extension, "session_shutdown", current.root);
  const remaining = current.runtime.mcpServers.list().length;
  await reset();
  return { registered, remaining, tools, connection_state: current.connection.state };
}
