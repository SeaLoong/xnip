import assert from 'node:assert/strict';
import path from 'node:path';
import { loadCline, HOST, verifyCline } from '../benches/native/cline_native_setup.mjs';
import { createXnipClineTools } from '../integrations/cline-native/extension.mjs';

export const executable = process.env.XNIP_TEST_BINARY ?? path.resolve(import.meta.dirname, '../target/debug/xnip');
export const hostDirectory = process.env.XNIP_CLINE_NATIVE_HOST ?? HOST;
export const toolOptions = { enableEditor: false, enableApplyPatch: true,
  enableBash: false, enableSearch: false, enableWebFetch: false, enableSkills: false, enableAskQuestion: false };

/** Actual SDK registry, policy, approval and hooks; tool-only entry avoids model generation. */
export async function createClineFixture({ cwd, native = true, executable: binary = executable,
  toolPolicies, hooks, requestToolApproval, createClient, executorOptions, loadHost } = {}) {
  const provenance = await verifyCline(hostDirectory);
  const host = await (loadHost ? loadHost() : loadCline(hostDirectory));
  const backend = native ? await createXnipClineTools({ ...toolOptions, cwd,
    loadHost: async () => host, resolveExecutable: () => binary, createClient, executorOptions }) : undefined;
  const tools = backend?.tools ?? host.createBuiltinTools({ ...toolOptions, cwd, executorOptions });
  let modelCalls = 0;
  const model = { async stream() { modelCalls++; throw new Error('Cline native tests execute registered tools directly'); } };
  const agent = new host.Agent({ model, systemPrompt: 'Cline tool-only native contract', tools,
    toolExecution: 'sequential', toolPolicies, hooks, requestToolApproval });
  // These are TypeScript-private, ordinary JS methods in the published SDK.
  // Product integrations use its public factories; this is the actual tool
  // dispatch pipeline, entered without Agent.run() or an invented model reply.
  assert.equal(typeof agent.ensureInitialized, 'function');
  assert.equal(typeof agent.executeToolCalls, 'function');
  await agent.ensureInitialized();
  const events = [];
  agent.subscribe(event => events.push(event));
  let nextId = 0;
  return {
    agent, tools, backend, events, provenance,
    get modelCalls() { return modelCalls; },
    describe() { return { version: provenance.version, tools: [...agent.tools.values()].map(tool => ({
      name: tool.name, description: tool.description, inputSchema: tool.inputSchema,
      timeoutMs: tool.timeoutMs, retryable: tool.retryable, maxRetries: tool.maxRetries,
    })) }; },
    async call(name, input, { signal } = {}) {
      agent.abortController = signal ? { signal } : undefined;
      const messages = await agent.executeToolCalls([{ type: 'tool-call', toolCallId: 'native-' + ++nextId,
        toolName: name, input }]);
      assert.equal(modelCalls, 0);
      return messages[0].content.find(part => part.type === 'tool-result');
    },
    async close() { await backend?.close(); assert.equal(modelCalls, 0); },
  };
}

export const patch = (...lines) => ['*** Begin Patch', ...lines, '*** End Patch'].join('\n');
