import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { prepareFixture } from './gemini_native_runtime.mjs';
import { resolveNpmCli } from '../benches/native/node_runtime.mjs';

const root = path.resolve(import.meta.dirname, '..');
const execute = promisify(execFile);

test('installed Gemini SDK entry resolves the verified payload and restores original IO', async t => {
  const directory = await fs.mkdtemp(path.join(root, 'target/gemini-native-packaged-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const binary = path.join(directory, process.platform === 'win32' ? 'xnip.exe' : 'xnip');
  await fs.copyFile(process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip'), binary);
  if (process.platform === 'linux') await execute('strip', ['--strip-debug', binary]);
  const output = path.join(directory, 'artifacts');
  await execute(process.env.XNIP_TEST_PYTHON ?? 'python3', [path.join(root, 'npm/prepare.py'),
    '--host-binary', binary, '--out', output, '--pack'], { cwd: root, timeout: 120000 });
  const report = JSON.parse(await fs.readFile(path.join(output, 'preparation.json'), 'utf8'));
  assert.equal(report.publication, 'disabled-private-packages');
  const tarball = path.join(output, report.tarballs.find(item => item.name === 'xnip-cli').filename);
  const project = path.join(directory, 'project');
  await fs.mkdir(project);
  await fs.writeFile(path.join(project, 'package.json'), JSON.stringify({ name: 'gemini-native-fixture', private: true }));
  await execute(process.execPath, [resolveNpmCli(), 'install', '--ignore-scripts', '--offline',
    '--no-audit', '--no-fund', '--legacy-peer-deps', tarball], { cwd: project, timeout: 120000 });
  const require = createRequire(path.join(project, 'package.json'));
  const { installXnipFileSystem } = await import(pathToFileURL(require.resolve('xnip-cli/gemini-native')).href);
  const previousCache = process.env.XNIP_NPM_CACHE_DIR;
  process.env.XNIP_NPM_CACHE_DIR = path.join(directory, 'execution-cache');
  t.after(() => {
    if (previousCache === undefined) delete process.env.XNIP_NPM_CACHE_DIR;
    else process.env.XNIP_NPM_CACHE_DIR = previousCache;
  });
  const runtime = await prepareFixture({ cwd: project, install: installXnipFileSystem });
  t.after(() => runtime.close());
  assert.deepEqual(runtime.tools.map(tool => tool.name).sort(), ['read_file', 'replace', 'write_file']);
  const file_path = path.join(project, 'file.txt');
  const written = await runtime.call('write_file', { file_path, content: '\ufeffone=1\r\ntwo=2' });
  assert.equal(written.error, undefined, JSON.stringify(written));
  const edited = await runtime.call('replace', { file_path, old_string: 'one=1', new_string: 'one=10',
    instruction: 'Update the first value.' });
  assert.equal(edited.error, undefined, JSON.stringify(edited));
  assert.equal(await fs.readFile(file_path, 'utf8'), '\ufeffone=10\r\ntwo=2');
  const cached = await fs.readdir(process.env.XNIP_NPM_CACHE_DIR, { recursive: true });
  assert.ok(cached.some(name => /xnip(?:\.exe)?$/.test(name)));
  const tools = runtime.registry.getAllTools();
  await runtime.installation.close();
  for (const tool of tools) assert.equal(runtime.registry.getTool(tool.name), tool);
  const restored = await runtime.call('replace', { file_path, old_string: 'two=2', new_string: 'two=20',
    instruction: 'Update the second value through restored original IO.' });
  assert.equal(restored.error, undefined, JSON.stringify(restored));
  assert.equal(await fs.readFile(file_path, 'utf8'), '\ufeffone=10\r\ntwo=20');
  assert.equal(runtime.modelCalls, 0);
});
