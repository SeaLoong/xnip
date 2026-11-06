#!/usr/bin/env node
/** JSON lifecycle bridge: real official profile composition and local tool execution. */
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { setEnvironmentData } from 'node:worker_threads';
import { createFixture } from './runtime.mjs';
import { clientSource } from './editor-client.mjs';
import { validateRewind } from './rewind-validation.mjs';

const [verb, ...args] = process.argv.slice(2);
const option = name => { const i = args.indexOf(name); return i < 0 ? undefined : args[i + 1]; };
const profile = path.resolve(option('--profile') ?? '.');
const dsh = option('--dsh');
const pluginDir = option('--plugin');
const executable = option('--executable') ?? process.env.XNIP_EDITOR_EXECUTABLE ?? 'xnip';
const name = 'xnip-dsh-native';
const stage = option('--stage');
const installed = args.includes('--installed');
const execute = promisify(execFile);
function selectedEntries(entries) {
  return entries.flatMap(entry => entry.disabled === true ? [] : [entry,
    ...selectedEntries(Array.isArray(entry.config) ? entry.config : entry.config?.plugins ?? []),
  ]);
}

async function main() {
  if (!dsh || !path.isAbsolute(dsh)) throw new Error('--dsh must name the absolute official DSH executable');
  if (path.basename(path.dirname(profile)) !== 'profiles') throw new Error('--profile must be <DSH_HOME>/profiles/<name>');
  const home = path.dirname(path.dirname(profile));
  const env = { ...process.env, DSH_HOME: home, XNIP_AUTO_UPDATE: '0', npm_config_ignore_scripts: 'true' };
  const run = async argv => (await execute(dsh, argv, { cwd: profile, env, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, timeout: 120000 })).stdout;
  const binary = await fs.realpath(dsh);
  const hostRoot = path.dirname(path.dirname(binary));
  const host = JSON.parse(await fs.readFile(path.join(hostRoot, 'package.json'), 'utf8'));
  if (host.name !== '@deepseek-ai/dsh') throw new Error('--dsh does not point to the official DSH package');
  const manifest = JSON.parse(await fs.readFile(path.join(profile, 'package.json'), 'utf8'));
  await fs.access(path.join(profile, 'cordis.patch.yml'));
  const bundles = manifest.dsh?.profile?.bundles ?? [];
  const active = bundles.includes(name);
  const dump = () => run(['--profile', path.basename(profile), '--dump-config']);
  const fixtureOptions = async cwd => {
    const require = createRequire(path.join(hostRoot, 'package.json'));
    const boot = await import(pathToFileURL(require.resolve('@deepseek-ai/dsh-app-boot')).href);
    const loaded = boot.loadProfileDirectory('xnip-native-verify', profile, path.join(hostRoot, 'package.json'));
    if (loaded.skippedBundles.length) throw new Error('Profile has skipped bundles: ' + JSON.stringify(loaded.skippedBundles));
    const patches = boot.readProfilePatches('xnip-native-verify', {
      name: path.basename(profile), dir: profile, patchPath: path.join(profile, 'cordis.patch.yml'),
      installAnchor: path.join(hostRoot, 'package.json'), cwd, home, startedBundles: bundles,
      overlays: [], telemetryDisabledEnv: process.env.DSH_TELEMETRY_DISABLED,
    }, loaded);
    const entries = boot.composeEntries([patches]);
    if (active && entries.some(entry => entry.id === 'xnip-mcp' && !entry.disabled)) {
      throw new Error('Profile override keeps the xnip MCP bridge active alongside the native provider; select one active editing backend');
    }
    const providerEntries = entries.filter(entry => ['fs-sandbox', 'xnip-native-fs'].includes(entry.id));
    if (providerEntries.filter(entry => !entry.disabled).length !== 1) throw new Error('Composed profile must activate exactly one known filesystem provider');
    if (active !== providerEntries.some(entry => entry.id === 'xnip-native-fs' && !entry.disabled)) throw new Error('Native provider patch did not compose as intended');
    let plugin;
    if (active) {
      const packageRoot = path.join(profile, 'node_modules', name);
      const metadata = JSON.parse(await fs.readFile(path.join(packageRoot, 'package.json'), 'utf8'));
      if (metadata.name !== name) throw new Error('Installed native package identity mismatch');
      plugin = await import(pathToFileURL(path.join(packageRoot, 'index.mjs')).href);
    }
    const rewind = selectedEntries(entries).some(entry => entry.name === 'dsh-rewind-plugin' || entry.id === 'dsh-rewind-plugin');
    return { hostRoot, cwd, executable, native: active, plugin, providerEntries, rewind };
  };
  const fixture = async cwd => createFixture(await fixtureOptions(cwd));
  if (verb === 'preflight') {
    const require = createRequire(path.join(hostRoot, 'package.json'));
    const sandbox = await import(pathToFileURL(require.resolve('@deepseek-ai/dsh-fs-sandbox')).href);
    const proto = sandbox.default?.prototype;
    const supported = typeof proto?.checkedTarget === 'function' && typeof proto?.writeText === 'function' && typeof proto?.editText === 'function';
    const config = await dump();
    if (!config.includes('id: fs-sandbox')) throw new Error('Profile has no known fs-sandbox provider to replace');
    return { host: 'dsh', version: host.version, level: supported ? 'L3' : 'L0', mechanism: 'filesystem-provider', supported,
      gaps: ['native operation requests are limited to 32 MiB', 'existing host sessions need restart to load a changed provider'] };
  }
  if (verb === 'observe') {
    const config = await dump();
    const temp = await fs.mkdtemp(path.join(os.tmpdir(), 'xnip-dsh-observe-'));
    let runtime;
    try {
      runtime = await fixture(temp);
      const description = await runtime.describe();
      return { catalog: JSON.stringify({ tools: description.tools, composed_profile: config }), prompt: description.prompt };
    } finally { if (runtime) await runtime.close(); await fs.rm(temp, { recursive: true, force: true }); }
  }
  if (verb === 'stage') {
    if (!stage || !pluginDir) throw new Error('stage requires --stage and --plugin');
    await fs.mkdir(stage, { recursive: true });
    const packaged = path.join(stage, 'package');
    await fs.cp(path.resolve(pluginDir), packaged, { recursive: true });
    await fs.copyFile(clientSource, path.join(packaged, 'editor-client.mjs'));
    // This configuration belongs to the installed package. The profile user's
    // patch remains untouched, and runtime and verification use the same binary.
    await fs.writeFile(path.join(packaged, 'cordis.patch.yml'),
      `- id: xnip-mcp\n  disabled: true\n- id: fs-sandbox\n  disabled: true\n- insert:\n    - id: xnip-native-fs\n      name: ./index.mjs\n      config:\n        executable: ${JSON.stringify(executable)}\n`);
    const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
    const result = JSON.parse((await execute(npm, ['pack', packaged, '--pack-destination', path.resolve(stage), '--ignore-scripts', '--json'], { encoding: 'utf8', env, timeout: 30000 })).stdout);
    await fs.writeFile(path.join(stage, 'package.json'), JSON.stringify({ tarball: result[0].filename, integrity: result[0].integrity }));
    return {};
  }
  if (verb === 'apply') {
    if (!stage) throw new Error('apply requires --stage');
    const metadata = JSON.parse(await fs.readFile(path.join(stage, 'package.json'), 'utf8'));
    const tarball = path.resolve(stage, metadata.tarball);
    if (path.dirname(tarball) !== path.resolve(stage)) throw new Error('Invalid staged package path');
    const digest = 'sha512-' + createHash('sha512').update(await fs.readFile(tarball)).digest('base64');
    if (digest !== metadata.integrity) throw new Error('Staged package integrity mismatch');
    await run(['plugin', '--profile', path.basename(profile), 'add', '--ignore-scripts', tarball]);
    return {};
  }
  if (verb === 'remove') { await run(['plugin', '--profile', path.basename(profile), 'remove', name]); return {}; }
  if (verb === 'activate') { await dump(); return {}; }
  if (verb === 'verify') {
    if (active !== installed) throw new Error('Profile native bundle membership does not match requested state');
    const config = await dump();
    if (installed && (!config.includes('id: xnip-native-fs') || !config.includes('id: fs-sandbox'))) throw new Error('Native provider patch did not compose');
    const temp = await fs.mkdtemp(path.join(os.tmpdir(), 'xnip-dsh-verify-'));
    let runtime;
    try {
      runtime = await fixture(temp);
      const target = path.join(temp, 'proof.txt');
      await fs.writeFile(target, 'keep\r\nold\r\n尾巴');
      const read = await runtime.call('read', { file_path: target });
      if (read.isError) throw new Error('Real host read failed: ' + JSON.stringify(read));
      const edit = await runtime.call('edit', { file_path: target, old_string: 'old', new_string: 'new' });
      if (edit.isError) throw new Error('Real host edit failed: ' + JSON.stringify(edit));
      const actual = await fs.readFile(target, 'utf8');
      if (!actual.includes('new') || actual.includes('old')) throw new Error('Host edit did not mutate expected bytes');
      if (installed && actual !== 'keep\r\nnew\r\n尾巴') throw new Error('Native edit did not preserve byte contract');
      await runtime.close();
      runtime = undefined;
      let governance;
      if (installed) {
        const options = await fixtureOptions(temp);
        if (options.rewind) {
          const profileRequire = createRequire(path.join(profile, 'package.json'));
          const hostRequire = createRequire(path.join(hostRoot, 'package.json'));
          const boot = await import(pathToFileURL(hostRequire.resolve('@deepseek-ai/dsh-app-boot')).href);
          const loaded = boot.loadProfileDirectory('xnip-native-verify', profile, path.join(hostRoot, 'package.json'));
          const resolution = await boot.createRuntimeResolution({ installAnchor: path.join(hostRoot, 'package.json'), profile: loaded, home });
          // Use the host's exported inherited-resolution bootstrap so a profile
          // outside the host installation resolves its declared official peers.
          setEnvironmentData('@deepseek-ai/dsh-app-boot/profile-resolution', { resolution });
          await import(pathToFileURL(hostRequire.resolve('@deepseek-ai/dsh-app-boot/worker/profile-resolution-bootstrap')).href);
          const resolve = name => { try { return profileRequire.resolve(name); } catch { return hostRequire.resolve(name); } };
          options.rewindPlugin = await import(pathToFileURL(resolve('dsh-rewind-plugin')).href);
          options.rewindVersion = JSON.parse(await fs.readFile(resolve('dsh-rewind-plugin/package.json'), 'utf8')).version;
        }
        governance = await validateRewind(options);
      }
      return { functional_edit: true, checks: [
        { name: 'official-profile-composition', status: 'passed', detail: 'Official dsh --dump-config completed' },
        { name: 'real-native-edit', status: 'passed', detail: installed ? 'Installed provider edited bytes through official tools' : 'Original provider edited bytes through official tools' },
        ...(installed ? [{ name: 'profile-governance', status: 'passed', detail:
          `${governance.rewind.enabled ? `${governance.plugin}@${governance.version}` : 'rewind inactive in selected profile'}: ${governance.assertions.join('; ')}` }] : []),
      ] };
    } finally { if (runtime) await runtime.close(); await fs.rm(temp, { recursive: true, force: true }); }
  }
  throw new Error('Unknown lifecycle verb: ' + verb);
}

try { process.stdout.write(JSON.stringify(await main()) + '\n'); }
catch (error) { process.stderr.write((error.stack ?? String(error)) + '\n'); process.exitCode = 1; }
