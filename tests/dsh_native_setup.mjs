/** Execute frozen installs and real host tool validation in fresh directories. */
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { VERSIONS, hostDirectory } from '../benches/native/dsh_native_setup.mjs';
import { CACHE as historicalCache } from '../benches/native/dsh_setup.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const script = path.join(root, 'benches/native/dsh_native_setup.mjs');

for (const version of VERSIONS) {
  test(`frozen DSH ${version} setup starts official tools, verifies contents and repeats npm ci`, { timeout: 600000 }, async t => {
    assert.notEqual(hostDirectory(version), historicalCache, 'Native full host owns a separate dependency tree from the source adapter');
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'xnip-native-host-'));
    t.after(() => fs.rm(directory, { recursive: true, force: true }));
    const args = [script, '--version', version, '--destination', directory];
    const run = (...extra) => JSON.parse(execFileSync(process.execPath, [...args, ...extra], {
      cwd: os.tmpdir(), encoding: 'utf8', timeout: 300000, maxBuffer: 4 * 1024 * 1024,
    }).trim());
    const prepared = run();
    assert.equal(prepared.status, 'available');
    assert.equal(prepared.version, version);
    assert.equal(prepared.rewind, '0.19.2');
    assert.deepEqual(prepared.tools, ['read', 'write', 'edit']);
    assert.deepEqual(run('--check'), prepared);
    const receipt = JSON.parse(await fs.readFile(path.join(directory, 'native-host.json'), 'utf8'));
    assert.equal(receipt.install.scripts_executed, false);
    assert.equal(receipt.validation.model_calls, 0);
    assert.equal(receipt.validation.profile_exports, true);
    assert.equal(receipt.validation.rewind_exports, true);
    assert.equal(receipt.validation.packages['@deepseek-ai/cordis-plugin-group'], '1.0.5');
    assert.equal(receipt.validation.packages['@deepseek-ai/dsh-compaction'], version);
    assert.ok(receipt.dependency_files.length > 1000);
    assert.match(receipt.provenance.lock_sha256, /^[0-9a-f]{64}$/);

    const tool = path.join(directory, 'node_modules/@deepseek-ai/dsh-tool-fs/lib/index.js');
    await fs.appendFile(tool, '\n// setup integrity fixture\n');
    const changed = spawnSync(process.execPath, [...args, '--check'], {
      cwd: os.tmpdir(), encoding: 'utf8', timeout: 300000,
    });
    assert.equal(changed.status, 1);
    assert.match(changed.stderr, /Prepared host dependency tree changed/);
    const sentinel = path.join(directory, 'node_modules/.previous-install');
    await fs.writeFile(sentinel, 'npm ci replaces the previous dependency tree\n');
    assert.deepEqual(run(), prepared);
    await assert.rejects(fs.access(sentinel), { code: 'ENOENT' });
    assert.deepEqual(run('--check'), prepared);
  });
}
