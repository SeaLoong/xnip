import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import test from 'node:test';
import { createFixture } from './opencode_native_runtime.mjs';
import { HOST } from '../benches/native/opencode_native_setup.mjs';
import { resolveNpmCli } from '../benches/native/node_runtime.mjs';
const root = path.resolve(import.meta.dirname, '..'),
  exec = promisify(execFile);

test('installed npm plugin resolves its verified executable and restores real OpenCode builtins', async (t) => {
  const directory = await fs.mkdtemp(path.join(root, 'target/opencode-native-package-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const binary = path.join(directory, 'xnip');
  await fs.copyFile(process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip'), binary);
  if (process.platform === 'linux') await exec('strip', ['--strip-debug', binary]);
  const output = path.join(directory, 'prepared');
  await exec(
    process.env.XNIP_TEST_PYTHON ?? 'python3',
    [path.join(root, 'npm/prepare.py'), '--host-binary', binary, '--out', output, '--pack'],
    { cwd: root, timeout: 120000 },
  );
  const report = JSON.parse(await fs.readFile(path.join(output, 'preparation.json'), 'utf8'));
  assert.equal(report.publication, 'disabled-private-packages');
  const tarball = path.join(
    output,
    report.tarballs.find((item) => item.name === 'xnip-cli').filename,
  );
  const project = path.join(directory, 'project');
  await fs.mkdir(project);
  await fs.writeFile(
    path.join(project, 'package.json'),
    JSON.stringify({ name: 'xnip-opencode-local', private: true }),
  );
  await exec(
    process.execPath,
    [
      resolveNpmCli(),
      'install',
      '--ignore-scripts',
      '--offline',
      '--no-audit',
      '--no-fund',
      '--legacy-peer-deps',
      tarball,
    ],
    { cwd: project, timeout: 120000 },
  );
  for (const name of ['@opencode-ai/plugin', 'zod'])
    await fs.cp(path.join(HOST, 'node_modules', name), path.join(project, 'node_modules', name), {
      recursive: true,
    });
  const installed = path.join(project, 'node_modules/xnip-cli');
  const metadata = JSON.parse(await fs.readFile(path.join(installed, 'package.json'), 'utf8'));
  assert.equal(metadata.exports['./opencode'], './integrations/opencode/index.mjs');
  const cache = path.join(directory, 'native-cache');
  const runtime = await createFixture({
    cwd: project,
    entry: pathToFileURL(path.join(installed, 'integrations/opencode/index.mjs')).href,
    childEnvironment: { XNIP_NPM_CACHE_DIR: cache },
  });
  t.after(() => runtime.close());
  assert.equal((await runtime.describe()).tools.xnip_edit, true);
  const created = await runtime.call('xnip_write', {
    filePath: 'a.txt',
    content: '\ufeffold\r\n尾巴',
  });
  assert.equal(created.ok, true, created.error);
  const edited = await runtime.call('xnip_edit', {
    filePath: 'a.txt',
    oldString: 'old',
    newString: 'new',
  });
  assert.equal(edited.ok, true, edited.error);
  assert.deepEqual(
    await fs.readFile(path.join(project, 'a.txt')),
    Buffer.from('\ufeffnew\r\n尾巴'),
  );
  assert.ok((await fs.readdir(cache, { recursive: true })).some((file) => file.endsWith('/xnip')));
  await runtime.setNative(false);
  assert.equal((await runtime.describe()).tools.xnip_edit, undefined);
  const restored = await runtime.call('edit', {
    filePath: path.join(project, 'a.txt'),
    oldString: 'new',
    newString: 'restored',
  });
  assert.equal(restored.ok, true, restored.error);
  assert.deepEqual(
    await fs.readFile(path.join(project, 'a.txt')),
    Buffer.from('\ufeffrestored\r\n尾巴'),
  );
  assert.equal(runtime.modelCalls, 0);
});
