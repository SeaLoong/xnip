/** Actual Pi SDK, loader, registry and agent-core tool pipeline; zero model calls. */
import fs from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { prepare, COMMIT, VERSION } from '../benches/native/pi_adapter.mjs';

const root = path.resolve(import.meta.dirname, '..');
await prepare();
const modules = path.join(root, 'target/native-bench/pi-v1.3.0/node_modules');
// Pi publishes an import-only root export. These are the exact public import
// targets from the pinned package manifests, rather than require() targets.
const sdk = pathToFileURL(path.join(modules, '@earendil-works/pi-coding-agent/dist/index.js')).href;
export const host = await import(sdk);
const { runToolCall } = await import(pathToFileURL(path.join(modules, '@earendil-works/pi-agent-core/dist/index.js')).href);
export { COMMIT, VERSION };

export async function createFixture({ cwd, executable, native = true, hooks = () => {}, extensionPath,
  extensionPaths = [], extensionFactories = [], tools = ['read', 'edit', 'write'] }) {
  const agentDir = path.join(cwd, '.pi-fixture-agent');
  await fs.mkdir(agentDir, { recursive: true });
  const settingsManager = host.SettingsManager.inMemory();
  const entry = extensionPath ?? path.join(agentDir, 'xnip-native.mjs');
  if (!extensionPath) {
    const adapter = pathToFileURL(path.join(root, 'integrations/pi-native/extension.mjs')).href;
    await fs.writeFile(entry, `import { createXnipNativeExtension } from ${JSON.stringify(adapter)};
import * as host from ${JSON.stringify(sdk)};
export default createXnipNativeExtension({resolveExecutable:()=>${JSON.stringify(executable)},loadHost:async()=>host});
`);
  }
  settingsManager.setExtensionPaths([...extensionPaths, ...(native ? [entry] : [])]);
  const resourceLoader = new host.DefaultResourceLoader({ cwd, agentDir, settingsManager,
    extensionFactories: [hooks, ...extensionFactories], noSkills: true, noPromptTemplates: true, noThemes: true, noContextFiles: true });
  await resourceLoader.reload();
  if (resourceLoader.getExtensions().errors.length) {
    throw new Error(JSON.stringify(resourceLoader.getExtensions().errors));
  }
  const modelRuntime = await host.ModelRuntime.create({ authPath: path.join(agentDir, 'auth.json'),
    modelsPath: null, refreshOnCreate: false, allowModelNetwork: false });
  const { session } = await host.createAgentSession({ cwd, agentDir, resourceLoader, settingsManager,
    modelRuntime, sessionManager: host.SessionManager.inMemory(cwd), tools });
  const errors = [];
  await session.bindExtensions({ onError: error => errors.push(error) });
  if (errors.length) throw new Error(JSON.stringify(errors));
  // A model request is outside this fixture's execution path. Fail visibly if
  // a future SDK or fixture change introduces one.
  let modelCalls = 0;
  session.agent.streamFunction = () => {
    modelCalls++;
    throw new Error('Pi native E2E only executes tools');
  };
  let nextCall = 0;
  return { session, resourceLoader, settingsManager, errors,
    get modelCalls() { return modelCalls; },
    call(name, args, signal = new AbortController().signal) {
      const toolCall = { type: 'toolCall', id: `pi-native-${++nextCall}`, name, arguments: structuredClone(args) };
      const assistantMessage = { role: 'assistant', content: [toolCall], api: 'fixture', provider: 'fixture',
        model: 'fixture', usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0,
          totalTokens: 0, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
        stopReason: 'toolUse', timestamp: 0 };
      return runToolCall(toolCall, { tools: session.agent.state.tools, assistantMessage,
        context: { messages: [], tools: session.agent.state.tools }, signal,
        beforeToolCall: session.agent.beforeToolCall, afterToolCall: session.agent.afterToolCall });
    },
    async setNative(enabled) {
      settingsManager.setExtensionPaths([...extensionPaths, ...(enabled ? [entry] : [])]);
      await session.reload();
      if (resourceLoader.getExtensions().errors.length || errors.length) {
        throw new Error(JSON.stringify([...resourceLoader.getExtensions().errors, ...errors]));
      }
    },
    async waitForDirectMcp() {
      await session.extensionRunner.emitBeforeAgentStart({ messageType: 'user', prompt: 'Tool fixture readiness' }, { cwd });
      if (errors.length) throw new Error(JSON.stringify(errors));
    },
    async close() {
      await session.extensionRunner.emit({ type: 'session_shutdown', reason: 'quit' });
      session.dispose();
    },
  };
}
