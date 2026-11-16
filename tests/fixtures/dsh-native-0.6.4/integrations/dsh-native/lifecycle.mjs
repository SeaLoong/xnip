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
const bundleDirectory = '.xnip-native-package';
const bundleEntry = './' + bundleDirectory;
const bundleRoot = path.join(profile, bundleDirectory);
const bundleFiles = ['package.json', 'index.mjs', 'lifecycle.mjs', 'runtime.mjs',
  'rewind-validation.mjs', 'editor-client.mjs', 'cordis.patch.yml'];
const stage = option('--stage');
const installed = args.includes('--installed');
const execute = promisify(execFile);
function selectedEntries(entries) {
  return entries.flatMap(entry => entry.disabled === true ? [] : [entry,
    ...selectedEntries(Array.isArray(entry.config) ? entry.config : entry.config?.plugins ?? []),
  ]);
}

async function writeProfileManifest(manifest) {
  // The temporary file belongs to the managed bundle directory. A crash leaves
  // both the old manifest and any temporary bytes inside the snapshot boundary.
  const temporary = await fs.mkdtemp(path.join(bundleRoot, '.manifest-'));
  try {
    const filename = path.join(temporary, 'package.json');
    await fs.writeFile(filename, JSON.stringify(manifest, null, 2) + '\n', {
      mode: (await fs.stat(path.join(profile, 'package.json'))).mode,
    });
    await fs.rename(filename, path.join(profile, 'package.json'));
  } finally { await fs.rm(temporary, { recursive: true, force: true }); }
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
  const active = bundles.includes(name) || bundles.includes(bundleEntry);
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
      const packageRoot = bundles.includes(bundleEntry) ? bundleRoot : path.join(profile, 'node_modules', name);
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
    await fs.mkdir(packaged, { recursive: true });
    await Promise.all(bundleFiles.filter(file => !['editor-client.mjs', 'cordis.patch.yml'].includes(file))
      .map(file => fs.copyFile(path.join(pluginDir, file), path.join(packaged, file))));
    await fs.copyFile(clientSource, path.join(packaged, 'editor-client.mjs'));
    // This configuration belongs to the installed package. The profile user's
    // patch remains untouched, and runtime and verification use the same binary.
    await fs.writeFile(path.join(packaged, 'cordis.patch.yml'),
      `- id: xnip-mcp\n  disabled: true\n- id: fs-sandbox\n  disabled: true\n- insert:\n    - id: xnip-native-fs\n      name: ./index.mjs\n      config:\n        executable: ${JSON.stringify(executable)}\n`);
    const files = await Promise.all(bundleFiles.map(async file => ({
      path: file,
      sha256: createHash('sha256').update(await fs.readFile(path.join(packaged, file))).digest('hex'),
    })));
    await fs.writeFile(path.join(stage, 'package.json'), JSON.stringify({ files }));
    return {};
  }
  if (verb === 'apply') {
    if (!stage) throw new Error('apply requires --stage');
    const metadata = JSON.parse(await fs.readFile(path.join(stage, 'package.json'), 'utf8'));
    const packaged = path.resolve(stage, 'package');
    const files = await Promise.all(metadata.files.map(async file => {
      const source = path.resolve(packaged, file.path);
      const destination = path.resolve(bundleRoot, file.path);
      if (!source.startsWith(packaged + path.sep) || !destination.startsWith(bundleRoot + path.sep)) {
        throw new Error('Invalid staged bundle file path');
      }
      const content = await fs.readFile(source);
      if (createHash('sha256').update(content).digest('hex') !== file.sha256) throw new Error('Staged bundle file integrity mismatch');
      return { destination, content };
    }));
    await fs.rm(bundleRoot, { recursive: true, force: true });
    for (const file of files) {
      await fs.mkdir(path.dirname(file.destination), { recursive: true });
      await fs.writeFile(file.destination, file.content);
    }
    const next = { ...manifest, dsh: { ...manifest.dsh, profile: { ...manifest.dsh?.profile,
      bundles: [...bundles.filter(entry => entry !== bundleEntry), bundleEntry],
    } } };
    await writeProfileManifest(next);
    return {};
  }
  if (verb === 'remove') {
    if (bundles.includes(bundleEntry)) {
      const next = { ...manifest, dsh: { ...manifest.dsh, profile: { ...manifest.dsh.profile,
        bundles: bundles.filter(entry => entry !== bundleEntry),
      } } };
      await writeProfileManifest(next);
      await fs.rm(bundleRoot, { recursive: true });
    } else {
      // Retained manifests from package-manager installations keep their
      // original removal lifecycle and dependency snapshots.
      await run(['plugin', '--profile', path.basename(profile), 'remove', name]);
    }
    return {};
  }
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
