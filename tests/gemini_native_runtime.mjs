import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { hostDirectory, verifyPrepared, prepare, VERSION } from '../benches/native/gemini_native_setup.mjs';
import { installXnipFileSystem } from '../integrations/gemini-native/index.mjs';

export { VERSION };
let prepared;
export async function loadSdk(directory = hostDirectory()) {
  if (!prepared) prepared = (async () => {
    try { await verifyPrepared({ directory }); }
    catch (error) {
      if (error.code !== 'ENOENT') throw error;
      await prepare({ directory });
    }
    const require = createRequire(path.join(directory, 'package.json'));
    return import(pathToFileURL(require.resolve('@google/gemini-cli-core')).href);
  })();
  return prepared;
}

/** Actual Config, tools, validation, policy bus and confirmation; no agent loop. */
export async function prepareFixture({ cwd, native = true, executable, policyDecision = 'allow',
  createClient, adapterOptions = {}, sandbox, directory = hostDirectory(),
  install = installXnipFileSystem } = {}) {
  const sdk = await loadSdk(directory);
  const temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'xnip-gemini-session-'));
  const responses = path.join(temporary, 'responses.jsonl');
  await fs.writeFile(responses, '');
  const config = new sdk.Config({
    sessionId: 'xnip-native-tools', targetDir: cwd, cwd, model: 'gemini-2.5-flash',
    debugMode: false, interactive: false, sandbox, telemetry: { enabled: false },
    usageStatisticsEnabled: false, enableHooks: false, disableLLMCorrection: true,
    fakeResponses: responses, policyEngineConfig: { defaultDecision: policyDecision },
  });
  // The official fake-response option initializes the normal BaseLlmClient.
  // Its empty response file makes any accidental generation fail and increment
  // the SDK's own call counter, without provider credentials or network calls.
  await config.refreshAuth(undefined);
  const generator = config.getContentGenerator().getWrapped();
  const bus = config.getMessageBus();
  const events = [];
  bus.subscribe('tool-confirmation-request', request => {
    events.push({ type: 'confirmation-request', tool: request.toolCall.name });
    void bus.publish({ type: 'tool-confirmation-response', correlationId: request.correlationId,
      requiresUserConfirmation: true, confirmed: false });
  });
  bus.subscribe('tool-policy-rejection', request => events.push({ type: 'policy-rejection', tool: request.toolCall.name }));
  const registry = new sdk.ToolRegistry(config, bus);
  for (const Tool of [sdk.ReadFileTool, sdk.EditTool, sdk.WriteFileTool]) registry.registerTool(new Tool(config, bus));
  let installation;
  try {
    if (native) installation = await install(config, { executable, createClient, ...adapterOptions });
  } catch (error) {
    await config.dispose();
    await fs.rm(temporary, { recursive: true, force: true });
    throw error;
  }
  const tools = registry.getAllTools();
  return {
    config, registry, tools, events, sdk, installation, adaptation: installation,
    version: VERSION,
    get modelCalls() { return generator.callCounter; },
    describe() { return { tools: registry.getFunctionDeclarations(), model_calls: generator.callCounter }; },
    async call(name, args, { signal = new AbortController().signal,
      confirm = sdk.ToolConfirmationOutcome.ProceedOnce } = {}) {
      const execute = async () => {
        const tool = registry.getTool(name);
        assert.ok(tool, 'registered Gemini tool: ' + name);
        const invocation = tool.build(structuredClone(args));
        const confirmation = await invocation.shouldConfirmExecute(signal);
        if (confirmation) {
          events.push({ type: 'confirmation-details', tool: name, details: confirmation });
          if (confirm === false) return { confirmation };
          await confirmation.onConfirm(confirm);
          if (confirm === sdk.ToolConfirmationOutcome.Cancel || confirm === sdk.ToolConfirmationOutcome.ModifyWithEditor) {
            return { confirmation, cancelled: true };
          }
        }
        const result = await invocation.execute(signal);
        assert.equal(generator.callCounter, 0, 'tool execution stays outside model generation');
        return result;
      };
      return installation ? installation.run(execute) : execute();
    },
    async close() {
      if (installation) await installation.close();
      await config.dispose();
      assert.equal(generator.callCounter, 0, 'fixture completed with zero model calls');
      await fs.rm(temporary, { recursive: true, force: true });
    },
  };
}
