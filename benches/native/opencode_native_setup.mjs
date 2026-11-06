#!/usr/bin/env node
/** Frozen official OpenCode CLI and plugin SDK for real tool-only execution. */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { resolveNpmCli } from './node_runtime.mjs';

export const VERSION = '1.19.3';
export const COMMIT = 'e3c4eb242585dcd77be1a867391b7c25fd47ce52';
const root = path.resolve(import.meta.dirname, '../..');
const inputs = path.join(import.meta.dirname, 'opencode_native');
export const HOST = path.join(root, 'target/native-hosts/opencode-' + VERSION);
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
export const executablePath = (directory = HOST) =>
  path.join(directory, 'node_modules/opencode-linux-x64/bin/opencode');

export async function verifyOpenCode(directory = HOST) {
  const provenance = JSON.parse(await fs.readFile(path.join(inputs, 'provenance.json'), 'utf8'));
  for (const [name, expected] of Object.entries(provenance.inputs)) {
    assert.equal(
      hash(await fs.readFile(path.join(inputs, name))),
      expected,
      'Committed OpenCode input',
    );
    assert.equal(
      hash(await fs.readFile(path.join(directory, name))),
      expected,
      'Run opencode_native_setup.mjs',
    );
  }
  const packages = {};
  for (const name of [
    'opencode-ai',
    'opencode-linux-x64',
    '@opencode-ai/plugin',
    '@opencode-ai/sdk',
  ]) {
    const base = path.join(directory, 'node_modules', name);
    const manifest = await fs.readFile(path.join(base, 'package.json'));
    assert.equal(JSON.parse(manifest).version, VERSION, name);
    packages[name] = hash(manifest);
  }
  const binary = executablePath(directory);
  assert.equal(execFileSync(binary, ['--version'], { encoding: 'utf8' }).trim(), VERSION);
  const actual = {
    version: VERSION,
    commit: COMMIT,
    packages,
    binary_sha256: hash(await fs.readFile(binary)),
    plugin_tool_sha256: hash(
      await fs.readFile(path.join(directory, 'node_modules/@opencode-ai/plugin/dist/tool.js')),
    ),
  };
  assert.deepEqual(actual, provenance.host, 'Official OpenCode bytes');
  return actual;
}

export async function prepareOpenCode(directory = HOST) {
  assert.equal(
    process.platform,
    'linux',
    'The pinned CLI fixture targets Linux x64; the plugin uses OpenCode public APIs on its supported platforms',
  );
  assert.equal(process.arch, 'x64', 'The pinned CLI fixture targets Linux x64');
  await fs.mkdir(directory, { recursive: true });
  for (const name of ['package.json', 'package-lock.json'])
    await fs.copyFile(path.join(inputs, name), path.join(directory, name));
  execFileSync(
    process.execPath,
    [
      resolveNpmCli(),
      'ci',
      '--prefix',
      directory,
      '--ignore-scripts',
      '--omit=optional',
      '--no-audit',
      '--no-fund',
      '--prefer-offline',
      '--registry=https://registry.npmjs.org',
      '--strict-ssl=true',
    ],
    { cwd: root, stdio: ['ignore', 'pipe', 'pipe'], timeout: 300000 },
  );
  const receipt = await verifyOpenCode(directory);
  await fs.writeFile(
    path.join(directory, 'native-host.json'),
    JSON.stringify(receipt, null, 2) + '\n',
  );
  return receipt;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({
    options: {
      check: { type: 'boolean', default: false },
      destination: { type: 'string' },
      help: { type: 'boolean', default: false },
    },
  });
  if (values.help)
    console.log(
      'Usage: node benches/native/opencode_native_setup.mjs [--check] [--destination DIRECTORY]\nPinned OpenCode 1.19.3 Linux x64 fixture; product plugin uses the public cross-platform API.',
    );
  else
    console.log(
      JSON.stringify(
        await (values.check ? verifyOpenCode : prepareOpenCode)(
          path.resolve(values.destination ?? HOST),
        ),
      ),
    );
}
