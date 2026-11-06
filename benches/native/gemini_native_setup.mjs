#!/usr/bin/env node
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { resolveNpmCli } from './node_runtime.mjs';

export const VERSION = '0.27.2';
const ROOT = path.resolve(import.meta.dirname, '../..');
const INPUTS = path.join(import.meta.dirname, 'gemini_native');
const hash = value => createHash('sha256').update(value).digest('hex');
export const hostDirectory = () => path.join(ROOT, 'target/native-hosts/gemini-' + VERSION);

async function inputs() {
  const manifest = await fs.readFile(path.join(INPUTS, 'package.json'));
  const lock = await fs.readFile(path.join(INPUTS, 'package-lock.json'));
  const provenance = JSON.parse(await fs.readFile(path.join(INPUTS, 'provenance.json'), 'utf8'));
  assert.equal(hash(manifest), provenance.manifest_sha256);
  assert.equal(hash(lock), provenance.lock_sha256);
  return { manifest, lock, provenance };
}

async function inventory(directory, prefix = '') {
  const items = [];
  for (const item of await fs.readdir(path.join(directory, prefix), { withFileTypes: true })) {
    const name = path.posix.join(prefix, item.name);
    const full = path.join(directory, name);
    if (item.isDirectory()) items.push(...await inventory(directory, name));
    else if (item.isSymbolicLink()) items.push({ path: name, link: await fs.readlink(full) });
    else if (item.isFile()) items.push({ path: name, sha256: hash(await fs.readFile(full)) });
  }
  return items.sort((a, b) => a.path.localeCompare(b.path, 'en'));
}

export async function validateHost(directory) {
  const require = createRequire(path.join(directory, 'package.json'));
  const sdk = await import(pathToFileURL(require.resolve('@google/gemini-cli-core')).href);
  const manifest = JSON.parse(await fs.readFile(path.join(directory, 'node_modules/@google/gemini-cli-core/package.json'), 'utf8'));
  assert.equal(manifest.version, VERSION);
  const cwd = await fs.mkdtemp(path.join(directory, '.host-check-'));
  const config = new sdk.Config({ sessionId: 'xnip-setup', targetDir: cwd, cwd,
    model: 'gemini-2.5-flash', debugMode: false, telemetry: { enabled: false }, usageStatisticsEnabled: false });
  try {
    assert.equal(typeof config.setFileSystemService, 'function');
    const registry = new sdk.ToolRegistry(config, config.getMessageBus());
    for (const Tool of [sdk.ReadFileTool, sdk.EditTool, sdk.WriteFileTool]) registry.registerTool(new Tool(config, config.getMessageBus()));
    const tools = registry.getAllTools().map(tool => tool.name).sort();
    assert.deepEqual(tools, ['read_file', 'replace', 'write_file']);
    const file = path.join(cwd, 'roundtrip.txt');
    await config.getFileSystemService().writeTextFile(file, 'official SDK\r\n尾巴');
    const result = await registry.getTool('read_file').build({ file_path: file }).execute(new AbortController().signal);
    assert.equal(result.error, undefined);
    assert.equal(await config.getFileSystemService().readTextFile(file), 'official SDK\r\n尾巴');
    return { package: manifest.name, version: manifest.version, tools, model_calls: 0 };
  } finally {
    await config.dispose();
    await fs.rm(cwd, { recursive: true, force: true });
  }
}

export async function prepare({ directory = hostDirectory() } = {}) {
  directory = path.resolve(directory);
  const { manifest, lock, provenance } = await inputs();
  await fs.mkdir(directory, { recursive: true });
  await fs.writeFile(path.join(directory, 'package.json'), manifest);
  await fs.writeFile(path.join(directory, 'package-lock.json'), lock);
  await fs.rm(path.join(directory, 'native-host.json'), { force: true });
  const npm = resolveNpmCli();
  const args = ['ci', '--prefix', directory, '--ignore-scripts', '--no-audit', '--no-fund',
    '--prefer-offline', '--strict-ssl=true', '--registry=https://registry.npmjs.org'];
  execFileSync(process.execPath, [npm, ...args], { cwd: ROOT, stdio: ['ignore', 'pipe', 'pipe'], timeout: 300000 });
  const validation = await validateHost(directory);
  const dependencyTree = await inventory(path.join(directory, 'node_modules'));
  const receipt = { host: 'gemini-sdk', version: VERSION, provenance, validation,
    runtime: { node: process.version, npm: execFileSync(process.execPath, [npm, '--version'], { encoding: 'utf8' }).trim() },
    install: { command: [process.execPath, npm, ...args], scripts_executed: false },
    dependency_tree_sha256: hash(JSON.stringify(dependencyTree)), dependency_files: dependencyTree };
  await fs.writeFile(path.join(directory, 'native-host.json'), JSON.stringify(receipt, null, 2) + '\n');
  return receipt;
}

export async function verifyPrepared({ directory = hostDirectory() } = {}) {
  directory = path.resolve(directory);
  const { provenance } = await inputs();
  const receipt = JSON.parse(await fs.readFile(path.join(directory, 'native-host.json'), 'utf8'));
  assert.deepEqual(receipt.provenance, provenance);
  assert.equal(hash(await fs.readFile(path.join(directory, 'package.json'))), provenance.manifest_sha256);
  assert.equal(hash(await fs.readFile(path.join(directory, 'package-lock.json'))), provenance.lock_sha256);
  assert.equal(hash(JSON.stringify(await inventory(path.join(directory, 'node_modules')))), receipt.dependency_tree_sha256,
    'Gemini dependency tree changed; run setup');
  assert.deepEqual(await validateHost(directory), receipt.validation);
  return receipt;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { values } = parseArgs({ options: {
      check: { type: 'boolean', default: false }, destination: { type: 'string' }, help: { type: 'boolean', default: false },
    } });
    if (values.help) console.log('Usage: node benches/native/gemini_native_setup.mjs [--check] [--destination <directory>]\nPrepares official core ' + VERSION + ' under target/native-hosts/gemini-' + VERSION + '.');
    else {
      const directory = path.resolve(values.destination ?? hostDirectory());
      const receipt = await (values.check ? verifyPrepared : prepare)({ directory });
      console.log(JSON.stringify({ status: 'available', directory, version: VERSION,
        tools: receipt.validation.tools, dependency_tree_sha256: receipt.dependency_tree_sha256 }));
    }
  } catch (error) { console.error(error.stack ?? String(error)); process.exitCode = 1; }
}
