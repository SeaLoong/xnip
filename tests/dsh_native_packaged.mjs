import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import test from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const hostRoot = process.env.XNIP_DSH_NATIVE_HOST ?? path.join(root, 'target/native-hosts/dsh-0.2.1-rc.5');
const executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip');
const execute = promisify(execFile);

async function validatePackagedProfile({ mcp = false } = {}) {
  const requireHost = createRequire(path.join(hostRoot, 'package.json'));
  const load = name => import(pathToFileURL(requireHost.resolve(name)).href);
  const [Boot, ProfileBoot, Environment] = await Promise.all([
    '@deepseek-ai/dsh-app-boot', '@deepseek-ai/dsh/profile-boot', '@deepseek-ai/dsh-launch-environment',
  ].map(load));
  const directory = await fs.mkdtemp(path.join(root, 'target/native-packaged-'));
  const home = path.join(directory, 'home');
  const profileDir = path.join(home, 'profiles', 'validation');
  const project = path.join(directory, 'project');
  const installedPackage = path.join(profileDir, '.xnip-native-package');
  const basePackage = path.join(directory, 'base-host');
  const mcpArtifacts = path.join(directory, 'mcp-artifacts');
  const stage = path.join(directory, 'stage');
  const previousHome = process.env.DSH_HOME;
  const previousPath = process.env.PATH;
  const previousExecutable = process.env.XNIP_EDITOR_EXECUTABLE;
  const previousCache = process.env.XNIP_NPM_CACHE_DIR;
  const previousUpdate = process.env.XNIP_UPDATE_STATE_DIR;
  let running;
  let handle;
  try {
    process.env.XNIP_NPM_CACHE_DIR = path.join(directory, 'mcp-native-cache');
    process.env.XNIP_UPDATE_STATE_DIR = path.join(directory, 'update-state');
    await fs.mkdir(project, { recursive: true });
    Boot.initProfile(profileDir, []);
    await fs.mkdir(basePackage, { recursive: true });
    await fs.writeFile(path.join(basePackage, 'package.json'), JSON.stringify({
      name: 'xnip-native-validation-host', version: '1.0.0', type: 'module',
      dsh: { bundle: { patch: './cordis.patch.yml' } },
    }));
    // The minimal host layer contains actual official services. Native's own
    // package layer disables its original fs provider and inserts its default.
    const plugins = [
      ['system-prompt', 'dsh-system-prompt'], ['tools', 'dsh-tools', { mode: 'native' }],
      ['session-projections', 'dsh-session-projection'], ['sessions', 'dsh-session'],
      ['llm', 'dsh-llm'], ['agents', 'dsh-agent'], ['agent-loop', 'dsh-agent-loop'],
      ['sandbox-policy', 'dsh-sandbox-policy', { mode: 'workspace-write', workspaceRoot: project }],
      ['fs-sandbox', 'dsh-fs-sandbox', { cwd: project }],
      ['fs-observation-policy', 'dsh-fs-observation-policy'],
      ['working-directory', 'dsh-working-directory', { defaultDirectory: project }],
      ['tool-fs', 'dsh-tool-fs', { readImage: false }],
    ];
    await fs.writeFile(path.join(basePackage, 'cordis.patch.yml'), JSON.stringify([{ insert: plugins.map(([id, name, config]) => ({
      id, name: '@deepseek-ai/' + name, ...(config ? { config } : {}),
    })) }]));
    let mcpTarball;
    if (mcp) {
      // Prepare the actual private package with its bundled native payload, so
      // removing native can also prove the original MCP default resolver works.
      const packagedBinary = path.join(directory, 'mcp-host-xnip');
      await fs.copyFile(executable, packagedBinary);
      // Developer builds can exceed the artifact size bound due to DWARF.
      // Strip only this private fixture copy; retain the real build identity.
      if (process.platform === 'linux') await execute('strip', ['--strip-debug', packagedBinary]);
      await execute('python3', [path.join(root, 'npm/prepare.py'), '--host-binary', packagedBinary,
        '--out', mcpArtifacts, '--pack'], { cwd: root, encoding: 'utf8', timeout: 120000 });
      const report = JSON.parse(await fs.readFile(path.join(mcpArtifacts, 'preparation.json'), 'utf8'));
      mcpTarball = path.join(mcpArtifacts, report.tarballs.find(item => item.name === 'xnip-cli').filename);
    }
    await fs.writeFile(path.join(profileDir, 'package.json'), JSON.stringify({
      name: 'xnip-profile-default-resolution', private: true,
      dependencies: { 'xnip-native-validation-host': 'file:' + basePackage, ...(mcp ? { 'xnip-cli': 'file:' + mcpTarball } : {}) },
      dsh: { profile: { bundles: ['xnip-native-validation-host', ...(mcp ? ['xnip-cli'] : [])] } },
    }));
    const userPatch = '# The installer must preserve this user-owned patch.\n[]\n';
    await fs.writeFile(path.join(profileDir, 'cordis.patch.yml'), userPatch);
    // Establish the user's existing bundles through the official package
    // manager before native installs its separate profile-owned directory.
    await execute(path.join(hostRoot, 'node_modules/.bin/dsh'),
      ['plugin', '--profile', 'validation', 'install', '--ignore-scripts', '--offline'],
      { cwd: profileDir, env: { ...process.env, DSH_HOME: home, XNIP_AUTO_UPDATE: '0' },
        encoding: 'utf8', timeout: 120000, maxBuffer: 32 * 1024 * 1024 });
    const lifecycle = verb => execute(process.execPath, [path.join(root, 'integrations/dsh-native/lifecycle.mjs'), verb,
      '--profile', profileDir, '--dsh', path.join(hostRoot, 'node_modules/.bin/dsh'),
      '--plugin', path.join(root, 'integrations/dsh-native'), '--executable', executable, '--stage', stage],
    { cwd: root, env: { ...process.env, XNIP_AUTO_UPDATE: '0' }, encoding: 'utf8', timeout: 120000, maxBuffer: 32 * 1024 * 1024 });
    await lifecycle('stage');
    await lifecycle('apply');
    assert.equal(await fs.readFile(path.join(profileDir, 'cordis.patch.yml'), 'utf8'), userPatch);
    const installed = JSON.parse(await fs.readFile(path.join(installedPackage, 'package.json'), 'utf8'));
    assert.equal(installed.name, 'xnip-dsh-native');
    assert.ok((await fs.readFile(path.join(installedPackage, 'cordis.patch.yml'), 'utf8')).includes(JSON.stringify(executable)));
    // Only the package-owned executable setting may locate the native child.
    // Preserve real node for the official boot and omit every PATH entry with xnip.
    const cleanPath = [];
    for (const item of (process.env.PATH ?? '').split(path.delimiter)) {
      if (!item) continue;
      if (!await fs.access(path.join(item, 'xnip')).then(() => true, () => false)) cleanPath.push(item);
    }
    process.env.PATH = cleanPath.join(path.delimiter);
    delete process.env.XNIP_EDITOR_EXECUTABLE;
    for (const item of cleanPath) assert.equal(await fs.access(path.join(item, 'xnip')).then(() => true, () => false), false);
    const installAnchor = requireHost.resolve('@deepseek-ai/dsh/package.json');
    const profile = Boot.loadProfileDirectory('xnip-packaged-test', profileDir, installAnchor);
    assert.deepEqual(profile.skippedBundles, []);
    const entries = Boot.composeEntries([Boot.readProfilePatches('xnip-packaged-test', {
      name: 'validation', dir: profileDir, patchPath: path.join(profileDir, 'cordis.patch.yml'),
      installAnchor, cwd: project, home, startedBundles: profile.bundles, overlays: [],
    }, profile)]);
    assert.equal(entries.find(entry => entry.id === 'fs-sandbox')?.disabled, true);
    assert.equal(entries.filter(entry => entry.id === 'xnip-native-fs' && !entry.disabled).length, 1);
    if (mcp) assert.equal(entries.find(entry => entry.id === 'xnip-mcp')?.disabled, true);
    assert.equal(await fs.access(path.join(profileDir, 'node_modules/@deepseek-ai/dsh-fs')).then(() => true, () => false), false);
    process.env.DSH_HOME = home;
    running = await ProfileBoot.runProfile({
      profile: 'validation', resolvedProfile: { profile, installAnchor }, patchFiles: [], args: [],
      environment: Environment.createLaunchEnvironmentSnapshot([{ source: 'process', values: process.env }]),
    });
    const { ctx } = running;
    let modelCalls = 0;
    ctx.on('llm/stream', () => { modelCalls++; throw new Error('Native E2E validation executes tools without model generation'); });
    // No createXnipPlugin(), dependency injection, synthetic FS or ToolRuntime.
    assert.ok(ctx.fs.editor, 'The composed default plugin must own the native process');
    assert.deepEqual(ctx.tools.schemas().map(tool => tool.name), ['read', 'write', 'edit']);
    const target = path.join(project, 'actual.txt');
    await fs.writeFile(target, 'unchanged\r\nold\r\n尾巴');
    handle = await ctx.agents.create({ sessionId: 'installed-default-proof', meta: { cwd: project } });
    const agent = handle.agent;
    await agent.whenIdle();
    const call = (name, args) => ctx.tools.execute({ signal: new AbortController().signal,
      callId: 'installed-' + name, name, arguments: args, agent });
    assert.equal((await call('read', { file_path: target })).isError, false);
    const result = await call('edit', { file_path: target, old_string: 'old', new_string: 'new' });
    assert.equal(result.isError, false, JSON.stringify(result));
    assert.equal(await fs.readFile(target, 'utf8'), 'unchanged\r\nnew\r\n尾巴');
    assert.equal(modelCalls, 0);
    const editor = ctx.fs.editor;
    await handle.dispose(); handle = undefined;
    await running.shutdown.shutdown(0); running = undefined;
    assert.equal(editor.closed, true);
    assert.notEqual(editor.child.exitCode, null, 'Owned native process must exit on official profile shutdown');
    if (mcp) {
      await lifecycle('remove');
      assert.equal(await fs.readFile(path.join(profileDir, 'cordis.patch.yml'), 'utf8'), userPatch);
      const restored = Boot.loadProfileDirectory('xnip-packaged-restored', profileDir, installAnchor);
      assert.deepEqual(restored.skippedBundles, []);
      running = await ProfileBoot.runProfile({
        profile: 'validation', resolvedProfile: { profile: restored, installAnchor }, patchFiles: [], args: [],
        environment: Environment.createLaunchEnvironmentSnapshot([{ source: 'process', values: process.env }]),
      });
      let restoredModelCalls = 0;
      running.ctx.on('llm/stream', () => { restoredModelCalls++; throw new Error('Restored MCP E2E validation executes tools without model generation'); });
      assert.equal(Boolean(running.ctx.fs.editor), false);
      assert.deepEqual(running.ctx.tools.schemas().map(tool => tool.name).sort(), ['read', 'write', 'edit',
        ...['xnip_apply', 'xnip_edit', 'xnip_find', 'xnip_peek', 'xnip_recover'].map(name => 'mcp__xnip__' + name)].sort());
      const result = await running.ctx.tools.execute({ signal: new AbortController().signal,
        callId: 'restored-mcp', name: 'mcp__xnip__xnip_edit',
        arguments: { file: target, old: 'new', text: 'restored' } });
      assert.notEqual(result.isError, true, JSON.stringify(result));
      assert.equal(await fs.readFile(target, 'utf8'), 'unchanged\r\nrestored\r\n尾巴');
      assert.equal(restoredModelCalls, 0);
      await running.shutdown.shutdown(0); running = undefined;
    }
  } finally {
    if (handle) await handle.dispose();
    if (running) await running.shutdown.shutdown(0);
    if (previousHome === undefined) delete process.env.DSH_HOME;
    else process.env.DSH_HOME = previousHome;
    if (previousPath === undefined) delete process.env.PATH;
    else process.env.PATH = previousPath;
    if (previousExecutable === undefined) delete process.env.XNIP_EDITOR_EXECUTABLE;
    else process.env.XNIP_EDITOR_EXECUTABLE = previousExecutable;
    if (previousCache === undefined) delete process.env.XNIP_NPM_CACHE_DIR;
    else process.env.XNIP_NPM_CACHE_DIR = previousCache;
    if (previousUpdate === undefined) delete process.env.XNIP_UPDATE_STATE_DIR;
    else process.env.XNIP_UPDATE_STATE_DIR = previousUpdate;
    await fs.rm(directory, { recursive: true, force: true });
  }
}

test('official profile boot loads staged native package defaults without xnip on PATH',
  { timeout: 120000 }, () => validatePackagedProfile());

test('native package disables existing xnip MCP bundle through official profile composition and boot',
  { timeout: 120000 }, () => validatePackagedProfile({ mcp: true }));
