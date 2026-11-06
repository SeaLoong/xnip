#!/usr/bin/env node
/** Fixed official Cline SDK used by native tools, E2E and provider benchmarks. */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { resolveNpmCli } from './node_runtime.mjs';

export const VERSION = '0.2.0';
const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
export const HOST = path.join(ROOT, 'target/native-hosts/cline-' + VERSION);
const INPUTS = path.join(HERE, 'cline_native');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');

export async function loadCline(directory = HOST) {
  const base = path.join(directory, 'node_modules/@cline/sdk');
  const manifest = JSON.parse(await fs.readFile(path.join(base, 'package.json'), 'utf8'));
  return import(pathToFileURL(path.join(base, manifest.exports['.'].import)).href);
}

export async function verifyCline(directory = HOST) {
  const provenance = JSON.parse(await fs.readFile(path.join(INPUTS, 'provenance.json'), 'utf8'));
  for (const [name, expected] of Object.entries(provenance.inputs)) {
    assert.equal(hash(await fs.readFile(path.join(INPUTS, name))), expected, 'Committed Cline inputs');
    assert.equal(hash(await fs.readFile(path.join(directory, name))), expected, 'Run cline_native_setup.mjs');
  }
  const files = {};
  for (const name of ['sdk', 'core', 'agents', 'shared', 'llms']) {
    const base = path.join(directory, 'node_modules/@cline', name);
    const manifest = JSON.parse(await fs.readFile(path.join(base, 'package.json'), 'utf8'));
    assert.equal(manifest.version, VERSION, '@cline/' + name);
    files['@cline/' + name] = {
      version: manifest.version,
      package_sha256: hash(await fs.readFile(path.join(base, 'package.json'))),
      entry_sha256: hash(await fs.readFile(path.join(base, 'dist/index.js'))),
    };
  }
  assert.deepEqual(files, provenance.packages, 'Official Cline SDK package bytes');
  const sdk = await loadCline(directory);
  assert.equal(typeof sdk.computePatchChanges, 'function');
  assert.equal(typeof sdk.Agent, 'function');
  const tools = sdk.createBuiltinTools({ cwd: directory, enableEditor: false, enableApplyPatch: true,
    enableBash: false, enableSearch: false, enableWebFetch: false, enableSkills: false, enableAskQuestion: false });
  assert.deepEqual(tools.map(tool => tool.name), ['read_files', 'apply_patch']);
  const receipt = { version: VERSION, packages: files, tools: tools.map(tool => tool.name),
    lock_sha256: provenance.inputs['package-lock.json'], node: process.version };
  const recorded = path.join(directory, 'native-host.json');
  try {
    assert.deepEqual(JSON.parse(await fs.readFile(recorded, 'utf8')).packages, files, 'Prepared Cline package bytes changed');
  } catch (error) { if (error.code !== 'ENOENT') throw error; }
  return receipt;
}

export async function prepareCline(directory = HOST) {
  await fs.mkdir(directory, { recursive: true });
  for (const name of ['package.json', 'package-lock.json']) {
    await fs.copyFile(path.join(INPUTS, name), path.join(directory, name));
  }
  await fs.rm(path.join(directory, 'native-host.json'), { force: true });
  execFileSync(process.execPath, [resolveNpmCli(), 'ci', '--prefix', directory,
    '--ignore-scripts', '--no-audit', '--no-fund', '--prefer-offline',
    '--registry=https://registry.npmjs.org', '--strict-ssl=true'], {
    cwd: ROOT, stdio: ['ignore', 'pipe', 'pipe'], timeout: 300000,
  });
  const receipt = await verifyCline(directory);
  await fs.writeFile(path.join(directory, 'native-host.json'), JSON.stringify(receipt, null, 2) + '\n');
  return receipt;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { values } = parseArgs({ options: {
      check: { type: 'boolean', default: false }, destination: { type: 'string' }, help: { type: 'boolean', default: false },
    } });
    if (values.help) console.log('Usage: node benches/native/cline_native_setup.mjs [--check] [--destination <directory>]\nFixed @cline/sdk 0.2.0 under target/native-hosts/cline-0.2.0.');
    else console.log(JSON.stringify(await (values.check ? verifyCline : prepareCline)(path.resolve(values.destination ?? HOST))));
  } catch (error) { console.error(error.stack ?? String(error)); process.exitCode = 1; }
}
