import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createFixture, host } from './pi_native_runtime.mjs';
import { resolveNpmCli } from '../benches/native/node_runtime.mjs';

const root = path.resolve(import.meta.dirname, '..');
const execute = promisify(execFile);

test('installed defaults preserve MCP coexistence, explicit tool selection and official unload restoration', async t => {
  const directory = await fs.mkdtemp(path.join(root, 'target/pi-native-packaged-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const binary = path.join(directory, process.platform === 'win32' ? 'xnip.exe' : 'xnip');
  await fs.copyFile(process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip'), binary);
  if (process.platform === 'linux') await execute('strip', ['--strip-debug', binary]);
  const output = path.join(directory, 'artifacts');
  await execute(process.env.XNIP_TEST_PYTHON ?? 'python3', [path.join(root, 'npm/prepare.py'),
    '--host-binary', binary, '--out', output, '--pack'], { cwd: root, timeout: 120000 });
  const report = JSON.parse(await fs.readFile(path.join(output, 'preparation.json'), 'utf8'));
  assert.equal(report.publication, 'disabled-private-packages');
  const tarball = path.join(output, report.tarballs.find(x => x.name === 'xnip-cli').filename);
  const project = path.join(directory, 'project');
  await fs.mkdir(project);
  await fs.writeFile(path.join(project, 'package.json'), JSON.stringify({ name: 'pi-native-fixture', private: true }));
  await execute(process.execPath, [resolveNpmCli(), 'install', '--ignore-scripts', '--offline',
    '--no-audit', '--no-fund', '--legacy-peer-deps', tarball], { cwd: project, timeout: 120000 });
  const installed = path.join(project, 'node_modules/xnip-cli');
  const manifest = JSON.parse(await fs.readFile(path.join(installed, 'package.json'), 'utf8'));
  assert.equal(manifest.exports['./pi-native'], './integrations/pi-native/index.mjs');
  assert.deepEqual(manifest.pi.extensions, ['./integrations/pi/index.mjs']);
  const previousCache = process.env.XNIP_NPM_CACHE_DIR;
  const previousAgentDir = process.env.PI_CODING_AGENT_DIR;
  const executionCache = path.join(directory, 'execution-cache');
  process.env.XNIP_NPM_CACHE_DIR = executionCache;
  process.env.PI_CODING_AGENT_DIR = path.join(directory, 'isolated-pi-agent');
  t.after(() => {
    if (previousCache === undefined) delete process.env.XNIP_NPM_CACHE_DIR;
    else process.env.XNIP_NPM_CACHE_DIR = previousCache;
    if (previousAgentDir === undefined) delete process.env.PI_CODING_AGENT_DIR;
    else process.env.PI_CODING_AGENT_DIR = previousAgentDir;
  });
  const runtime = await createFixture({ cwd: project, native: false, tools: ['read', 'edit', 'write', 'mcp__*'],
    extensionPath: path.join(installed, 'integrations/pi-native/index.mjs'),
    extensionPaths: manifest.pi.extensions.map(entry => path.join(installed, entry)),
    extensionFactories: [host.createMcpExtension({
      loadConfig: () => ({ servers: [], errors: [], autoEnableCodemode: false }),
      logPath: path.join(directory, 'mcp.log'),
    })],
    hooks(pi) {
      pi.registerMcpServer('unrelated', { command: binary, args: ['mcp'], cwd: project,
        env: { XNIP_AUTO_UPDATE: '0' }, exposure: 'direct' });
    },
  });
  t.after(() => runtime.close());
  await runtime.waitForDirectMcp();
  const originalTools = runtime.session.getActiveToolNames().sort();
  assert.ok(originalTools.includes('mcp__xnip__xnip_edit'), JSON.stringify({ active: originalTools,
    registered: runtime.session.getAllTools().map(x => x.name), errors: runtime.errors,
    servers: runtime.resourceLoader.getExtensions().runtime.mcpServers.list(),
    log: await fs.readFile(path.join(directory, 'mcp.log'), 'utf8').catch(() => '') }));
  assert.ok(originalTools.includes('mcp__unrelated__xnip_edit'));
  assert.match(JSON.stringify(runtime.session.getAllTools().find(x => x.name === 'edit').sourceInfo), /builtin/);
  await runtime.setNative(true);
  await runtime.waitForDirectMcp();
  assert.deepEqual(runtime.session.getActiveToolNames().sort(), originalTools,
    'Selecting the native extension preserves the existing MCP loadout');
  // This fixture acts as the user selecting their active tool list through the
  // public API. The native extension itself leaves MCP exposure and loadout alone.
  const editingExtras = new Set(['mcp__xnip__xnip_edit', 'mcp__xnip__xnip_apply', 'mcp__xnip__xnip_recover']);
  runtime.session.setActiveToolsByName(originalTools.filter(name => !editingExtras.has(name)));
  const selected = runtime.session.getActiveToolNames();
  for (const name of ['read', 'write', 'edit']) assert.equal(selected.filter(value => value === name).length, 1);
  for (const name of editingExtras) assert.ok(!selected.includes(name));
  for (const name of originalTools.filter(value => value.startsWith('mcp__unrelated__'))) assert.ok(selected.includes(name));
  for (const name of ['mcp__xnip__xnip_peek', 'mcp__xnip__xnip_find']) assert.ok(selected.includes(name));
  const registrations = runtime.resourceLoader.getExtensions().runtime.mcpServers.list();
  assert.deepEqual(registrations.map(entry => entry.name).sort(), ['unrelated', 'xnip']);
  assert.ok(runtime.session.getAllTools().some(tool => tool.name === 'mcp__xnip__xnip_edit'),
    'An inactive MCP declaration remains registered');
  assert.match(JSON.stringify(runtime.session.getAllTools().find(x => x.name === 'edit').sourceInfo), /pi-native/);
  const cached = await fs.readdir(executionCache, { recursive: true });
  assert.ok(cached.some(x => /xnip(?:\.exe)?$/.test(x)), 'Default resolver prepares the native executable cache');
  let childPid;
  if (process.platform === 'linux') {
    const { stdout } = await execute('ps', ['-o', 'pid=,args=', '--ppid', String(process.pid)]);
    const row = stdout.split('\n').find(line => line.includes(executionCache) && line.includes('native serve'));
    assert.ok(row, 'Default extension starts its real native serve subprocess');
    childPid = row.trim().split(/\s+/)[0];
  }
  const created = await runtime.call('write', { path: 'a', content: '\ufeffone=1\r\ntwo=2' });
  assert.equal(created.isError, false, JSON.stringify(created.result));
  const edited = await runtime.call('edit', { path: 'a', edits: [
    { oldText: 'one=1', newText: 'one=10' }, { oldText: 'two=2', newText: 'two=20' },
  ] });
  assert.equal(edited.isError, false, JSON.stringify(edited.result));
  assert.deepEqual(await fs.readFile(path.join(project, 'a')), Buffer.from('\ufeffone=10\r\ntwo=20'));
  const read = await runtime.call('read', { path: 'a' });
  assert.equal(read.isError, false, JSON.stringify(read.result));
  const foreignRead = await runtime.call('mcp__unrelated__xnip_peek', { file: 'a', all: true });
  assert.equal(foreignRead.isError, false, JSON.stringify(foreignRead.result));
  await runtime.setNative(false);
  await runtime.waitForDirectMcp();
  runtime.session.setActiveToolsByName(originalTools);
  assert.deepEqual(runtime.session.getActiveToolNames().sort(), originalTools);
  assert.deepEqual(runtime.resourceLoader.getExtensions().runtime.mcpServers.list(), registrations);
  assert.match(JSON.stringify(runtime.session.getAllTools().find(x => x.name === 'edit').sourceInfo), /builtin/);
  if (childPid) await assert.rejects(fs.access(`/proc/${childPid}`), { code: 'ENOENT' });
  const restored = await runtime.call('edit', { path: 'a', edits: [{ oldText: 'two=20', newText: 'two=200' }] });
  assert.equal(restored.isError, false, JSON.stringify(restored.result));
  assert.deepEqual(await fs.readFile(path.join(project, 'a')), Buffer.from('\ufeffone=10\r\ntwo=200'));
  const restoredMcp = await runtime.call('mcp__xnip__xnip_edit', { file: 'a', old: 'two=200', text: 'two=2000' });
  assert.equal(restoredMcp.isError, false, JSON.stringify(restoredMcp.result));
  assert.deepEqual(await fs.readFile(path.join(project, 'a')), Buffer.from('\ufeffone=10\r\ntwo=2000'));
  assert.equal(runtime.session.isIdle, true);
  assert.equal(runtime.session.agent.state.messages.length, 0);
  assert.equal(runtime.modelCalls, 0);
  assert.deepEqual(runtime.errors, []);
});
