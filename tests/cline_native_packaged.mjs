import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { pathToFileURL } from 'node:url';
import { loadCline, HOST } from '../benches/native/cline_native_setup.mjs';
import { resolveNpmCli } from '../benches/native/node_runtime.mjs';
import { executable, patch } from './cline_native_runtime.mjs';

const root = path.resolve(import.meta.dirname, '..');
const execute = promisify(execFile);

test('private installed npm export resolves its native cache and restores the official Cline backend', async t => {
  const directory = await fs.mkdtemp(path.join(root, 'target/cline-native-packaged-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const binary = path.join(directory, process.platform === 'win32' ? 'xnip.exe' : 'xnip');
  await fs.copyFile(executable, binary);
  if (process.platform === 'linux') await execute('strip', ['--strip-debug', binary]);
  const output = path.join(directory, 'artifacts');
  await execute(process.env.XNIP_TEST_PYTHON ?? 'python3', [path.join(root, 'npm/prepare.py'),
    '--host-binary', binary, '--out', output, '--pack'], { cwd: root, timeout: 120000 });
  const report = JSON.parse(await fs.readFile(path.join(output, 'preparation.json'), 'utf8'));
  assert.equal(report.publication, 'disabled-private-packages');
  const tarball = path.join(output, report.tarballs.find(item => item.name === 'xnip-cli').filename);
  const project = path.join(directory, 'project');
  await fs.mkdir(project);
  await fs.writeFile(path.join(project, 'package.json'), JSON.stringify({ name: 'cline-native-fixture', private: true, type: 'module' }));
  await execute(process.execPath, [resolveNpmCli(), 'install', '--ignore-scripts', '--offline',
    '--no-audit', '--no-fund', '--legacy-peer-deps', tarball], { cwd: project, timeout: 120000 });
  await fs.symlink(path.join(process.env.XNIP_CLINE_NATIVE_HOST ?? HOST, 'node_modules/@cline'),
    path.join(project, 'node_modules/@cline'), process.platform === 'win32' ? 'junction' : 'dir');
  const installed = path.join(project, 'node_modules/xnip-cli');
  const manifest = JSON.parse(await fs.readFile(path.join(installed, 'package.json'), 'utf8'));
  assert.equal(manifest.exports['./cline-native'], './integrations/cline-native/index.mjs');
  const { createXnipClineTools } = await import(pathToFileURL(path.join(installed, manifest.exports['./cline-native'])).href);
  const previousCache = process.env.XNIP_NPM_CACHE_DIR;
  process.env.XNIP_NPM_CACHE_DIR = path.join(directory, 'execution-cache');
  t.after(() => { if (previousCache === undefined) delete process.env.XNIP_NPM_CACHE_DIR; else process.env.XNIP_NPM_CACHE_DIR = previousCache; });
  const host = await loadCline(process.env.XNIP_CLINE_NATIVE_HOST ?? HOST);
  const options = { cwd: project, enableEditor: false, enableApplyPatch: true, enableSearch: false,
    enableBash: false, enableWebFetch: false, enableSkills: false, enableAskQuestion: false };
  const backend = await createXnipClineTools(options);
  t.after(() => backend.close());
  let modelCalls = 0;
  const config = { systemPrompt: 'Installed Cline native tool-only contract', toolExecution: 'sequential',
    model: { async stream() { modelCalls++; throw new Error('Tool-only package verification'); } } };
  const agent = new host.Agent({ ...config, tools: backend.tools });
  await agent.ensureInitialized();
  assert.deepEqual([...agent.tools.keys()], ['read_files', 'apply_patch']);
  const call = async (runtime, input, id) => (await runtime.executeToolCalls([
    { type: 'tool-call', toolCallId: id, toolName: 'apply_patch', input: { input } },
  ]))[0].content[0].output;
  const created = await call(agent, patch('*** Add File: a', '+old'), 'create');
  assert.equal(created.success, true, JSON.stringify(created));
  const edited = await call(agent, patch('*** Update File: a', '@@', '-old', '+new'), 'edit');
  assert.equal(edited.success, true, JSON.stringify(edited));
  assert.equal(await fs.readFile(path.join(project, 'a'), 'utf8'), 'new');
  assert.ok((await fs.readdir(process.env.XNIP_NPM_CACHE_DIR, { recursive: true })).some(name => /xnip(?:\.exe)?$/.test(name)));
  await backend.close();
  const restored = new host.Agent({ ...config, tools: host.createBuiltinTools(options) });
  await restored.ensureInitialized();
  assert.deepEqual([...restored.tools.keys()], [...agent.tools.keys()]);
  assert.equal((await call(restored, patch('*** Update File: a', '@@', '-new', '+restored'), 'restore')).success, true);
  assert.equal(await fs.readFile(path.join(project, 'a'), 'utf8'), 'restored');
  assert.equal(modelCalls, 0);
});
