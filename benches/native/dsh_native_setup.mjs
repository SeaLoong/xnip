#!/usr/bin/env node
/** Prepare the official npm hosts used by native E2E and benchmarks. */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { resolveNpmCli } from './node_runtime.mjs';
import { createFixture } from '../../integrations/dsh-native/runtime.mjs';

export const VERSIONS = ['0.2.1-rc.2', '0.2.1-rc.5'];
const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const REGISTRY = 'https://registry.npmjs.org';
const RECEIPT = 'native-host.json';
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
export const hostDirectory = version => path.join(ROOT, 'target/native-hosts', 'dsh-' + version);

async function inputs(version) {
  assert.ok(VERSIONS.includes(version), 'Choose a supported DSH version: ' + VERSIONS.join(', '));
  const directory = path.join(HERE, 'dsh_native', version);
  const manifest = await fs.readFile(path.join(directory, 'package.json'));
  const lock = await fs.readFile(path.join(directory, 'package-lock.json'));
  const provenance = JSON.parse(await fs.readFile(path.join(directory, 'provenance.json'), 'utf8'));
  assert.equal(hash(manifest), provenance.manifest_sha256, 'Update native host manifest provenance');
  assert.equal(hash(lock), provenance.lock_sha256, 'Update native host lock provenance');
  return { manifest, lock, provenance };
}

async function inventory(directory, relative = '') {
  const files = [];
  for (const item of await fs.readdir(path.join(directory, relative), { withFileTypes: true })) {
    const name = path.posix.join(relative, item.name);
    const full = path.join(directory, name);
    if (item.isDirectory()) files.push(...await inventory(directory, name));
    else if (item.isSymbolicLink()) files.push({ file: name, link: await fs.readlink(full) });
    else if (item.isFile()) files.push({ file: name, sha256: hash(await fs.readFile(full)) });
  }
  return files.sort((a, b) => a.file.localeCompare(b.file, 'en'));
}

/** Start the official registry, execute its filesystem tools, and load profile/rewind exports. */
export async function validateHost(directory, version) {
  const requireHost = createRequire(path.join(directory, 'package.json'));
  const load = name => import(pathToFileURL(requireHost.resolve(name)).href);
  const expected = JSON.parse((await inputs(version)).manifest).dependencies;
  const packages = {};
  for (const [name, expectedVersion] of Object.entries(expected)) {
    const manifest = JSON.parse(await fs.readFile(path.join(directory, 'node_modules', name, 'package.json'), 'utf8'));
    assert.equal(manifest.version, expectedVersion, name + ' version');
    packages[name] = manifest.version;
  }
  const [boot, profile, environment, rewind] = await Promise.all([
    '@deepseek-ai/dsh-app-boot', '@deepseek-ai/dsh/profile-boot',
    '@deepseek-ai/dsh-launch-environment', 'dsh-rewind-plugin',
  ].map(load));
  assert.equal(typeof boot.loadProfileDirectory, 'function');
  assert.equal(typeof profile.runProfile, 'function');
  assert.equal(typeof environment.createLaunchEnvironmentSnapshot, 'function');
  assert.equal(typeof rewind.apply, 'function');
  const cwd = await fs.mkdtemp(path.join(directory, '.host-check-'));
  let fixture;
  try {
    fixture = await createFixture({ hostRoot: directory, cwd, native: false });
    let modelCalls = 0;
    fixture.ctx.on('llm/stream', () => { modelCalls++; throw new Error('Host setup executes filesystem tools directly'); });
    const tools = (await fixture.describe()).tools.map(tool => tool.name);
    assert.deepEqual(tools, ['read', 'write', 'edit']);
    const file = path.join(cwd, 'tool-roundtrip.txt');
    const write = await fixture.call('write', { file_path: file, content: 'old\r\n尾巴' });
    assert.equal(write.isError, false, JSON.stringify(write));
    assert.equal((await fixture.call('read', { file_path: file })).isError, false);
    const edit = await fixture.call('edit', { file_path: file, old_string: 'old', new_string: 'new' });
    assert.equal(edit.isError, false, JSON.stringify(edit));
    assert.equal(await fs.readFile(file, 'utf8'), 'new\r\n尾巴');
    assert.equal(modelCalls, 0);
    return { packages, tools, model_calls: modelCalls, profile_exports: true, rewind_exports: true };
  } finally {
    if (fixture) await fixture.close();
    await fs.rm(cwd, { recursive: true, force: true });
  }
}

export async function prepare({ version, directory = hostDirectory(version) }) {
  directory = path.resolve(directory);
  const { manifest, lock, provenance } = await inputs(version);
  await fs.mkdir(directory, { recursive: true });
  await fs.writeFile(path.join(directory, 'package.json'), manifest);
  await fs.writeFile(path.join(directory, 'package-lock.json'), lock);
  await fs.rm(path.join(directory, RECEIPT), { force: true });
  const npm = resolveNpmCli();
  const npmVersion = execFileSync(process.execPath, [npm, '--version'], { encoding: 'utf8' }).trim();
  const command = ['ci', '--prefix', directory, '--ignore-scripts', '--no-audit', '--no-fund',
    '--prefer-offline', '--strict-ssl=true', '--registry=' + REGISTRY];
  execFileSync(process.execPath, [npm, ...command], {
    cwd: ROOT, stdio: ['ignore', 'pipe', 'pipe'], timeout: 300000,
  });
  assert.equal(hash(await fs.readFile(path.join(directory, 'package-lock.json'))), provenance.lock_sha256);
  const validation = await validateHost(directory, version);
  const dependencyFiles = await inventory(path.join(directory, 'node_modules'));
  const receipt = {
    backend: 'deepseek-harness', version, registry: REGISTRY, provenance,
    runtime: { node: process.version, npm: npmVersion, executable: process.execPath },
    install: { command: [process.execPath, npm, ...command], scripts_executed: false },
    validation, dependency_files: dependencyFiles,
    dependency_tree_sha256: hash(JSON.stringify(dependencyFiles)),
  };
  await fs.writeFile(path.join(directory, RECEIPT), JSON.stringify(receipt, null, 2) + '\n');
  return receipt;
}

export async function verifyPrepared({ version, directory = hostDirectory(version) }) {
  directory = path.resolve(directory);
  const { provenance } = await inputs(version);
  const receipt = JSON.parse(await fs.readFile(path.join(directory, RECEIPT), 'utf8'));
  assert.equal(receipt.version, version);
  assert.deepEqual(receipt.provenance, provenance, 'Prepared host input provenance');
  assert.equal(hash(await fs.readFile(path.join(directory, 'package.json'))), provenance.manifest_sha256);
  assert.equal(hash(await fs.readFile(path.join(directory, 'package-lock.json'))), provenance.lock_sha256);
  const files = await inventory(path.join(directory, 'node_modules'));
  assert.equal(hash(JSON.stringify(files)), receipt.dependency_tree_sha256, 'Prepared host dependency tree changed; run setup');
  assert.deepEqual(await validateHost(directory, version), receipt.validation);
  return receipt;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { values } = parseArgs({ options: {
      version: { type: 'string', multiple: true }, destination: { type: 'string' },
      check: { type: 'boolean', default: false }, help: { type: 'boolean', default: false },
    } });
    if (values.help) {
      console.log('Usage: node benches/native/dsh_native_setup.mjs [--version <version>] [--check] [--destination <directory>]\nVersions: ' + VERSIONS.join(', ') + '\nDefault: prepare both hosts under target/native-hosts/dsh-<version>. --destination selects a directory for one --version.');
    } else {
      const versions = [...new Set(values.version ?? VERSIONS)];
      assert.ok(values.destination === undefined || versions.length === 1, '--destination requires one --version');
      for (const version of versions) {
        const directory = path.resolve(values.destination ?? hostDirectory(version));
        const result = await (values.check ? verifyPrepared : prepare)({ version, directory });
        console.log(JSON.stringify({ status: 'available', version, directory,
          tools: result.validation.tools, rewind: result.validation.packages['dsh-rewind-plugin'],
          dependency_tree_sha256: result.dependency_tree_sha256 }));
      }
    }
  } catch (error) {
    console.error(error.stack ?? String(error));
    process.exitCode = 1;
  }
}
