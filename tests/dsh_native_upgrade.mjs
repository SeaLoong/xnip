import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { createFixture } from '../integrations/dsh-native/runtime.mjs';
import { validateHostGovernance } from '../integrations/dsh-native/host-governance.mjs';
import { VERSIONS } from '../benches/native/dsh_native_setup.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const execute = promisify(execFile);
const executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip');
const hostsDirectory = process.env.XNIP_DSH_NATIVE_HOSTS_DIR ?? path.join(root, 'target/native-hosts');
const oldHost = path.join(hostsDirectory, 'dsh-0.2.1-rc.5');
const newHost = path.join(hostsDirectory, 'dsh-0.2.2-rc.1');
const versions = process.env.XNIP_DSH_NATIVE_VERSIONS ? JSON.parse(process.env.XNIP_DSH_NATIVE_VERSIONS) : VERSIONS;

if (versions.includes('0.2.1-rc.5') && versions.includes('0.2.2-rc.1')) test('original 0.6.4 bundle and manifest upgrade through current verify and uninstall', async t => {
  const fixture = path.join(root, 'tests/fixtures/dsh-native-0.6.4');
  const provenance = JSON.parse(await fs.readFile(path.join(fixture, 'provenance.json'), 'utf8'));
  assert.equal(provenance.commit, '45230e595efaf9c449ef7dd3a55f2776c127ef99');
  for (const [file, expected] of Object.entries(provenance.files_sha256)) {
    assert.equal(createHash('sha256').update(await fs.readFile(path.join(fixture, file))).digest('hex'), expected, file);
  }
  const oldPlugin = path.join(fixture, 'integrations/dsh-native');
  const currentPlugin = path.join(root, 'integrations/dsh-native');
  const home = await fs.mkdtemp(path.join(root, 'target/dsh-original-0.6.4-'));
  t.after(() => fs.rm(home, { recursive: true, force: true }));
  const profile = path.join(home, 'profiles/web');
  const env = { ...process.env, DSH_HOME: home, XNIP_AUTO_UPDATE: '0' };
  const originalExecutable = path.join(home, 'original-installed-xnip');
  // CI keeps the historical adapter/state fixture offline. A supplied release
  // payload also exercises the old native server; otherwise use the current
  // protocol-compatible server from a separate installation path.
  await fs.copyFile(process.env.XNIP_TEST_LEGACY_BINARY ?? executable, originalExecutable);
  await fs.chmod(originalExecutable, 0o755);
  if (process.env.XNIP_TEST_LEGACY_BINARY) {
    assert.equal((await execute(originalExecutable, ['--version'], { env })).stdout.trim(), 'xnip 0.6.4');
  }
  const native = async (verb, host, plugin, extra = [], cli = executable) => JSON.parse((await execute(cli,
    ['native', verb, '--profile', profile, '--plugin', plugin, '--dsh', path.join(host, 'node_modules/.bin/dsh'), ...extra],
    { env, timeout: 120000 })).stdout);
  await execute(path.join(oldHost, 'node_modules/.bin/dsh'), ['--profile', 'web', '--dump-config'], { env });
  const packagePath = path.join(profile, 'package.json');
  await fs.symlink(path.join(oldHost, 'node_modules'), path.join(profile, 'node_modules'), 'dir');
  const patchBefore = await fs.readFile(path.join(profile, 'cordis.patch.yml'));
  assert.equal((await native('install', oldHost, oldPlugin, [], originalExecutable)).phase, 'active');
  const statePath = path.join(profile, '.xnip-native/install-manifest.json');
  const oldState = JSON.parse(await fs.readFile(statePath, 'utf8'));
  // The published manifest predates the optional revalidation marker.
  delete oldState.rebased;
  oldState.xnip_version = '0.6.4';
  await fs.writeFile(statePath, JSON.stringify(oldState, null, 2) + '\n');
  const installedPackage = path.join(profile, '.xnip-native-package/package.json');
  const installedBefore = await fs.readFile(installedPackage);
  assert.equal(JSON.parse(installedBefore).version, '0.6.4');
  assert.equal(JSON.parse(installedBefore).peerDependencies['@deepseek-ai/dsh-fs'], '>=0.2.1-rc.2 <0.2.2');
  const upgraded = JSON.parse(await fs.readFile(packagePath, 'utf8'));
  upgraded.userUpdate = { preserve: true };
  await fs.writeFile(packagePath, JSON.stringify(upgraded, null, 2) + '\n');
  await fs.unlink(path.join(profile, 'node_modules'));
  await fs.symlink(path.join(newHost, 'node_modules'), path.join(profile, 'node_modules'), 'dir');
  const lockPath = path.join(profile, 'pnpm-lock.yaml');
  const lockBytes = Buffer.from('# user lock after dependency upgrade\nlockfileVersion: 9\n');
  await fs.writeFile(lockPath, lockBytes);
  const manifestBefore = await fs.readFile(statePath);
  await fs.rename(originalExecutable, originalExecutable + '.retained');
  await assert.rejects(native('verify', newHost, currentPlugin), /Official Loader did not activate the composed filesystem provider/);
  assert.deepEqual(await fs.readFile(statePath), manifestBefore);
  assert.deepEqual(await fs.readFile(installedPackage), installedBefore);
  await fs.rename(originalExecutable + '.retained', originalExecutable);
  const verified = await native('verify', newHost, currentPlugin);
  assert.equal(verified.host_version, '0.2.2-rc.1');
  assert.ok(verified.verification.checks.some(check => check.name === 'host-governance' && check.status === 'passed'));
  assert.deepEqual(await fs.readFile(installedPackage), installedBefore);
  assert.equal((await native('uninstall', newHost, currentPlugin)).phase, 'restored');
  upgraded.dsh.profile.bundles = upgraded.dsh.profile.bundles.filter(entry => entry !== './.xnip-native-package');
  assert.deepEqual(JSON.parse(await fs.readFile(packagePath, 'utf8')), upgraded);
  assert.deepEqual(await fs.readFile(lockPath), lockBytes);
  assert.deepEqual(await fs.readFile(path.join(profile, 'cordis.patch.yml')), patchBefore);
  assert.equal((await native('verify', newHost, currentPlugin, ['--restored'])).phase, 'restored');
});

if (versions.includes('0.2.1-rc.5') && versions.includes('0.2.2-rc.1')) test('installed compact bundle survives real host upgrade and preserves upgraded user files on uninstall and recovery', async t => {
  const directory = await fs.mkdtemp(path.join(root, 'target/dsh-upgrade-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const home = path.join(directory, 'home');
  const profile = path.join(home, 'profiles/web');
  const temp = path.join(directory, 'temp');
  const alias = path.join(directory, 'temp-alias');
  await fs.mkdir(temp);
  await fs.symlink(temp, alias, 'dir');
  const env = { ...process.env, DSH_HOME: home, TMPDIR: alias, XNIP_AUTO_UPDATE: '0' };
  await execute(path.join(oldHost, 'node_modules/.bin/dsh'), ['--profile', 'web', '--dump-config'], { env });
  const packagePath = path.join(profile, 'package.json');
  await fs.symlink(path.join(oldHost, 'node_modules'), path.join(profile, 'node_modules'), 'dir');
  const patchBefore = await fs.readFile(path.join(profile, 'cordis.patch.yml'));
  const native = async (verb, host, extra = []) => JSON.parse((await execute(executable,
    ['native', verb, '--profile', profile, '--plugin', path.join(root, 'integrations/dsh-native'),
      '--dsh', path.join(host, 'node_modules/.bin/dsh'), ...extra], { env, timeout: 120000 })).stdout);
  assert.equal((await native('install', oldHost, ['--compact-tools'])).phase, 'active');
  const observed = JSON.parse(await fs.readFile(path.join(profile, '.xnip-native/install-manifest.json')));
  assert.match(observed.installed_observation.catalog, /Replacement; empty deletes/);
  const statePath = path.join(profile, '.xnip-native/install-manifest.json');
  const stateBefore = await fs.readFile(statePath);
  const provider = path.join(profile, '.xnip-native-package/index.mjs');
  const providerBefore = await fs.readFile(provider);
  await fs.appendFile(provider, '\n// user modification\n');
  await assert.rejects(native('verify', newHost), /installed native bundle changed/);
  assert.deepEqual(await fs.readFile(statePath), stateBefore);
  assert.ok((await fs.readFile(provider, 'utf8')).endsWith('// user modification\n'));
  await fs.writeFile(provider, providerBefore);
  const upgraded = JSON.parse(await fs.readFile(packagePath, 'utf8'));
  upgraded.userUpgrade = { hostVersion: '0.2.2-rc.1', keep: ['custom', 'configuration'] };
  await fs.writeFile(packagePath, JSON.stringify(upgraded, null, 2) + '\n');
  if (process.platform !== 'win32') await fs.chmod(packagePath, 0o640);
  await fs.unlink(path.join(profile, 'node_modules'));
  await fs.symlink(path.join(newHost, 'node_modules'), path.join(profile, 'node_modules'), 'dir');
  const lockPath = path.join(profile, 'pnpm-lock.yaml');
  await fs.writeFile(lockPath, '# current user lock after host upgrade\nlockfileVersion: 9\n');
  const lockBefore = await fs.readFile(lockPath);
  const verified = await native('verify', newHost);
  assert.equal(verified.host_version, '0.2.2-rc.1');
  assert.equal(verified.verification.functional_edit, true);
  assert.ok(verified.verification.checks.some(check => check.name === 'host-governance' && check.status === 'passed'));
  assert.equal((await native('uninstall', newHost)).phase, 'restored');
  upgraded.dsh.profile.bundles = upgraded.dsh.profile.bundles.filter(x => x !== './.xnip-native-package');
  assert.deepEqual(JSON.parse(await fs.readFile(packagePath, 'utf8')), upgraded);
  if (process.platform !== 'win32') assert.equal((await fs.stat(packagePath)).mode & 0o777, 0o640);
  assert.deepEqual(await fs.readFile(lockPath), lockBefore);
  assert.deepEqual(await fs.readFile(path.join(profile, 'cordis.patch.yml')), patchBefore);
  assert.equal((await native('verify', newHost, ['--restored'])).phase, 'restored');
  assert.equal(await fs.access(path.join(profile, '.xnip-native-package')).then(() => true, () => false), false);

  const upgradedBaseline = await fs.readFile(packagePath);
  await execute(executable, ['native', 'clean', '--state-dir', path.join(profile, '.xnip-native'), '--purge-user-data'], { env });
  await fs.unlink(path.join(profile, 'node_modules'));
  await fs.symlink(path.join(oldHost, 'node_modules'), path.join(profile, 'node_modules'), 'dir');
  assert.equal((await native('install', oldHost, ['--compact-tools'])).phase, 'active');
  await fs.unlink(path.join(profile, 'node_modules'));
  await fs.symlink(path.join(newHost, 'node_modules'), path.join(profile, 'node_modules'), 'dir');
  assert.equal((await native('verify', newHost)).host_version, '0.2.2-rc.1');
  const interrupted = JSON.parse(await fs.readFile(statePath, 'utf8'));
  assert.equal(interrupted.rebased, true);
  interrupted.phase = 'recovery_required';
  interrupted.failed_phase = 'verify';
  await fs.writeFile(statePath, JSON.stringify(interrupted, null, 2) + '\n');
  const recovered = await native('recover', newHost);
  assert.equal(recovered.phase, 'restored');
  assert.equal(recovered.verification.functional_edit, true);
  assert.ok(recovered.verification.checks.every(check => check.status === 'passed'));
  const restoredState = JSON.parse(await fs.readFile(statePath, 'utf8'));
  assert.equal(restoredState.rebased, false);
  assert.notEqual(restoredState.baseline_observation.prompt, interrupted.baseline_observation.prompt);
  assert.deepEqual(await fs.readFile(packagePath), upgradedBaseline);
  assert.deepEqual(await fs.readFile(lockPath), lockBefore);
  assert.deepEqual(await fs.readFile(path.join(profile, 'cordis.patch.yml')), patchBefore);
  assert.equal((await native('verify', newHost, ['--restored'])).phase, 'restored');
});

if (versions.includes('0.2.2-rc.3')) test('an unrelated broken plugin and its changes stay outside native installation, verification, recovery and removal', async t => {
  const host = path.join(hostsDirectory, 'dsh-0.2.2-rc.3');
  const home = await fs.mkdtemp(path.join(root, 'target/dsh-unrelated-plugin-'));
  t.after(() => fs.rm(home, { recursive: true, force: true }));
  const profile = path.join(home, 'profiles/web');
  const env = { ...process.env, DSH_HOME: home, XNIP_AUTO_UPDATE: '0' };
  const native = async (verb, extra = []) => JSON.parse((await execute(executable,
    ['native', verb, '--profile', profile, '--plugin', path.join(root, 'integrations/dsh-native'),
      '--dsh', path.join(host, 'node_modules/.bin/dsh'), ...extra], { env, timeout: 120000 })).stdout);
  await execute(path.join(host, 'node_modules/.bin/dsh'), ['--profile', 'web', '--dump-config'], { env });
  const unrelated = path.join(profile, 'node_modules/xnip-unrelated-fixture');
  await fs.mkdir(unrelated, { recursive: true });
  await fs.writeFile(path.join(unrelated, 'package.json'), JSON.stringify({
    name: 'xnip-unrelated-fixture', version: '0.0.1', type: 'module', exports: './index.mjs',
    dsh: { bundle: { patch: './cordis.patch.yml' } },
  }));
  const imported = path.join(home, 'unrelated-module-was-imported');
  const modulePath = path.join(unrelated, 'index.mjs');
  await fs.writeFile(modulePath, `import { writeFileSync } from 'node:fs';\nwriteFileSync(${JSON.stringify(imported)}, 'imported');\nthrow new Error('Unrelated fixture refused startup');\nexport const name = 'xnip-unrelated-fixture';\nexport function apply() {}\n`);
  await assert.rejects(import(pathToFileURL(modulePath).href), /Unrelated fixture refused startup/);
  await fs.rm(imported);
  const unrelatedPatch = path.join(unrelated, 'cordis.patch.yml');
  const rows = [{ insert: [{ id: 'xnip-unrelated-probe', name: './index.mjs' }] }];
  await fs.writeFile(unrelatedPatch, JSON.stringify(rows) + '\n');
  const packagePath = path.join(profile, 'package.json');
  const metadata = JSON.parse(await fs.readFile(packagePath, 'utf8'));
  metadata.dependencies = { ...metadata.dependencies, 'xnip-unrelated-fixture': '0.0.1' };
  metadata.dsh.profile.bundles.push('xnip-unrelated-fixture');
  await fs.writeFile(packagePath, JSON.stringify(metadata, null, 2) + '\n');
  const baselinePackage = await fs.readFile(packagePath);
  const userPatch = await fs.readFile(path.join(profile, 'cordis.patch.yml'));
  const lockPath = path.join(profile, 'pnpm-lock.yaml');
  await fs.writeFile(lockPath, '# user dependency lock\nlockfileVersion: 9\n');
  const userLock = await fs.readFile(lockPath);
  const statePath = path.join(profile, '.xnip-native/install-manifest.json');
  const corePassed = (report, installed = true) => {
    assert.equal(report.verification.functional_edit, true);
    assert.ok(report.verification.checks.every(check => check.status === 'passed'));
    if (installed) assert.ok(report.verification.checks.some(check => check.name === 'host-governance'));
    assert.equal(report.verification.checks.some(check => check.name.includes('ecosystem') || check.name === 'profile-governance'), false);
  };
  corePassed(await native('install'));
  corePassed(await native('verify'));
  const interrupted = JSON.parse(await fs.readFile(statePath, 'utf8'));
  assert.doesNotMatch(interrupted.installed_observation.catalog, /xnip-unrelated-probe/);
  // Retained snapshots are real. This phase record represents a process
  // interruption after writing the verified installation, before completion.
  interrupted.phase = 'recovery_required';
  interrupted.failed_phase = 'verify';
  await fs.writeFile(statePath, JSON.stringify(interrupted, null, 2) + '\n');
  rows.push({ id: 'xnip-unrelated-probe', disabled: true });
  await fs.writeFile(unrelatedPatch, JSON.stringify(rows) + '\n');
  const recovered = await native('recover');
  assert.equal(recovered.phase, 'restored');
  corePassed(recovered, false);
  assert.deepEqual(await fs.readFile(packagePath), baselinePackage);
  assert.deepEqual(JSON.parse(await fs.readFile(unrelatedPatch, 'utf8')), rows);
  rows[1].disabled = false;
  await fs.writeFile(unrelatedPatch, JSON.stringify(rows) + '\n');
  const restored = await native('verify', ['--restored']);
  assert.equal(restored.phase, 'restored');
  corePassed(restored, false);
  await execute(executable, ['native', 'clean', '--state-dir', path.join(profile, '.xnip-native'), '--purge-user-data'], { env });
  corePassed(await native('install'));
  const packageBeforeUpgrade = await fs.readFile(packagePath);
  const beforeUpgrade = JSON.parse(await fs.readFile(statePath, 'utf8'));
  rows[1].disabled = true;
  await fs.writeFile(unrelatedPatch, JSON.stringify(rows) + '\n');
  corePassed(await native('verify'));
  const afterUpgrade = JSON.parse(await fs.readFile(statePath, 'utf8'));
  assert.deepEqual(afterUpgrade.installed_observation, beforeUpgrade.installed_observation);
  assert.doesNotMatch(afterUpgrade.installed_observation.catalog, /xnip-unrelated-probe/);
  assert.deepEqual(await fs.readFile(packagePath), packageBeforeUpgrade);
  assert.deepEqual(await fs.readFile(lockPath), userLock);
  assert.equal((await native('uninstall')).phase, 'restored');
  assert.deepEqual(await fs.readFile(packagePath), baselinePackage);
  assert.deepEqual(await fs.readFile(path.join(profile, 'cordis.patch.yml')), userPatch);
  assert.deepEqual(await fs.readFile(lockPath), userLock);
  assert.deepEqual(JSON.parse(await fs.readFile(unrelatedPatch, 'utf8')), rows);
  assert.equal(await fs.access(imported).then(() => true, () => false), false);
});

for (const version of versions) {
  test(`compact tools preserve real policy, presentation, bytes and official governance on ${version}`, async t => {
    const cwd = await fs.mkdtemp(path.join(root, 'target/dsh-compact-'));
    t.after(() => fs.rm(cwd, { recursive: true, force: true }));
    const options = { hostRoot: path.join(hostsDirectory, 'dsh-' + version), cwd, executable,
      toolConfig: version.startsWith('0.2.2-') ? { applyPatch: true } : {} };
    const original = await createFixture(options);
    const compact = await createFixture({ ...options, compact: true });
    try {
      const old = (await original.describe()).tools;
      const next = (await compact.describe()).tools;
      assert.deepEqual(old.map(x => x.name), next.map(x => x.name));
      assert.ok(JSON.stringify(next).length < JSON.stringify(old).length);
      const file = path.join(cwd, 'compact.txt');
      await fs.writeFile(file, '\ufeffkeep\r\nold\n尾巴');
      assert.equal((await compact.call('read', { file_path: file })).isError, false);
      const result = await compact.call('edit', { file_path: file, old_string: 'old', new_string: 'new' });
      assert.equal(result.isError, false, JSON.stringify(result));
      assert.deepEqual(await fs.readFile(file), Buffer.from('\ufeffkeep\r\nnew\n尾巴'));
      if (version.startsWith('0.2.2-')) {
        assert.ok(next.some(tool => tool.name === 'apply_patch'));
        const patchFile = path.join(cwd, 'patch.txt');
        assert.equal((await compact.call('write', { file_path: patchFile, content: 'old\n' })).isError, false);
        const patch = await compact.call('apply_patch', { input: '*** Begin Patch\n*** Update File: patch.txt\n@@\n-old\n+new\n*** End Patch' });
        assert.equal(patch.isError, false, JSON.stringify(patch));
        assert.equal(await fs.readFile(patchFile, 'utf8'), 'new\n');
      }
      assert.equal(result.content[0].text, 'edited');
      const denied = await compact.call('edit', { file_path: file, old_string: '', new_string: 'bad' });
      assert.equal(denied.isError, true);
      assert.deepEqual(await fs.readFile(file), Buffer.from('\ufeffkeep\r\nnew\n尾巴'));
    } finally { await compact.close(); await original.close(); }
    assert.equal((await validateHostGovernance({ ...options, compact: true })).passed, true);
  });
}
