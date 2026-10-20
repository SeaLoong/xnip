/** Installed private tarball -> default xnip/native resolution -> official DSH peer. */
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { createRequire } = require('node:module');
const { pathToFileURL } = require('node:url');
const test = require('node:test');

const install = process.env.XNIP_DSH_TEST_INSTALL;
const runtimePath = process.env.XNIP_DSH_TEST_RUNTIME;
const hash = data => crypto.createHash('sha256').update(data).digest('hex');
const optIn = !install || !runtimePath ? 'Set XNIP_DSH_TEST_INSTALL and XNIP_DSH_TEST_RUNTIME for the installed-package integration' : false;

test('installed private package loads its default native resolver and official bridge peer', {
  skip: optIn,
  timeout: 45000,
}, async () => {
  assert.ok(path.isAbsolute(install) && path.isAbsolute(runtimePath));
  const requireInstalled = createRequire(path.join(install, 'package.json'));
  const packageFile = requireInstalled.resolve('xnip/package.json');
  const packageDir = path.dirname(packageFile);
  const manifest = JSON.parse(await fs.readFile(packageFile, 'utf8'));
  const nativeManifest = JSON.parse(await fs.readFile(path.join(packageDir, 'native-manifest.json'), 'utf8'));
  assert.equal(manifest.private, true, 'this test accepts the prepared private package only');
  assert.equal(nativeManifest.localOnly, true, 'a real host-only native payload is required');
  const peer = JSON.parse(await fs.readFile(requireInstalled.resolve('@deepseek-ai/dsh-mcp-client/package.json'), 'utf8'));
  assert.equal(peer.version, '0.2.1-rc.2');
  const runtime = await import(pathToFileURL(runtimePath).href);
  assert.equal(typeof runtime.loadOptionalPatches, 'function');
  const patches = runtime.loadOptionalPatches('xnip-installed-test', path.join(packageDir, manifest.dsh.bundle.patch));
  const inserted = patches.flatMap(patch => patch.insert ?? []);
  assert.equal(inserted.length, 1);
  assert.equal(inserted[0].name, pathToFileURL(requireInstalled.resolve('xnip/dsh')).href);
  const plugin = await import(inserted[0].name);
  const { Context } = await import(pathToFileURL(requireInstalled.resolve('@deepseek-ai/cordis')).href);
  const { default: ToolRuntime } = await import(pathToFileURL(requireInstalled.resolve('@deepseek-ai/dsh-tools')).href);
  const { default: SystemPrompt } = await import(pathToFileURL(requireInstalled.resolve('@deepseek-ai/dsh-system-prompt')).href);
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'xnip dsh installed-'));
  const cache = path.join(root, 'immutable-cache');
  const previousCache = process.env.XNIP_NPM_CACHE_DIR;
  const previousUpdate = process.env.XNIP_UPDATE_STATE_DIR;
  process.env.XNIP_NPM_CACHE_DIR = cache;
  process.env.XNIP_UPDATE_STATE_DIR = path.join(root, 'update-state');
  const ctx = new Context();
  try {
    const native = requireInstalled('xnip/native');
    const executable = native.resolveNativeExecutable();
    assert.ok(path.relative(cache, executable) && !path.relative(cache, executable).startsWith('..'));
    assert.ok(!executable.startsWith(packageDir + path.sep), 'the package-manager payload is never mapped');
    const entry = nativeManifest.platforms[process.platform + '-' + process.arch];
    const expectedHash = hash(await fs.readFile(executable));
    assert.equal(expectedHash, entry.sha256);
    assert.equal(plugin.createMcpConfig({ cwd: root }).command, executable);
    await fs.writeFile(path.join(root, 'note.txt'), Buffer.from('标题\r\nvalue=old', 'utf8'));
    await ctx.plugin(SystemPrompt);
    await ctx.plugin(ToolRuntime);
    // No resolver injection and no loadMcpClient injection: both installed defaults.
    await ctx.plugin(plugin, { ...inserted[0].config, cwd: root });
    const names = ctx.tools.schemas().map(item => item.name).filter(name => name.startsWith('mcp__xnip__'));
    assert.equal(names.length, 5);
    const call = (id, args) => ctx.tools.execute({
      signal: new AbortController().signal, callId: id, name: 'mcp__xnip__xnip_edit', arguments: args,
    });
    const result = await call('installed-edit', { file: 'note.txt', old: 'value=old', text: 'value=new' });
    assert.notEqual(result.isError, true);
    assert.match(result.content.filter(item => item.type === 'text').map(item => item.text).join(''), /written/);
    const expected = Buffer.from('标题\r\nvalue=new', 'utf8');
    assert.deepEqual(await fs.readFile(path.join(root, 'note.txt')), expected);
    const preview = await call('installed-preview', { file: 'note.txt', old: 'value=new', text: 'value=preview', dry_run: true });
    assert.notEqual(preview.isError, true);
    assert.deepEqual(await fs.readFile(path.join(root, 'note.txt')), expected);
    assert.equal(hash(await fs.readFile(executable)), expectedHash);
    assert.equal(await fs.access(path.join(root, 'update-state')).then(() => true, () => false), false);
  } finally {
    await ctx.fiber.dispose();
    if (previousCache === undefined) delete process.env.XNIP_NPM_CACHE_DIR;
    else process.env.XNIP_NPM_CACHE_DIR = previousCache;
    if (previousUpdate === undefined) delete process.env.XNIP_UPDATE_STATE_DIR;
    else process.env.XNIP_UPDATE_STATE_DIR = previousUpdate;
    const resolved = await fs.realpath(root);
    assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
    assert.ok(path.basename(resolved).startsWith('xnip dsh installed-'));
    await fs.rm(resolved, { recursive: true, force: true });
  }
});

test('official profile resolver supplies the host peer to a separate profile package', {
  skip: optIn,
  timeout: 45000,
}, async () => {
  const requireHost = createRequire(path.join(install, 'package.json'));
  const installedPackage = path.dirname(requireHost.resolve('xnip/package.json'));
  const runtime = await import(pathToFileURL(runtimePath).href);
  assert.equal(typeof runtime.createRuntimeResolution, 'function');
  assert.equal(typeof runtime.installRuntimeInterception, 'function');
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'xnip dsh profile-'));
  const home = path.join(root, 'home');
  const profileDir = path.join(home, 'profiles', 'local');
  const packageDir = path.join(profileDir, 'node_modules', 'xnip');
  const cwd = path.join(root, 'project');
  await fs.mkdir(cwd, { recursive: true });
  await fs.mkdir(path.dirname(packageDir), { recursive: true });
  await fs.cp(installedPackage, packageDir, { recursive: true });
  await fs.writeFile(path.join(profileDir, 'package.json'), JSON.stringify({
    name: 'xnip-dsh-local-profile-test', private: true, dependencies: { xnip: '0.5.0' },
  }));
  const profile = { name: 'local', dir: profileDir, layers: [], patches: [], skippedBundles: [],
    patchPath: path.join(profileDir, 'cordis.patch.yml') };
  const resolution = await runtime.createRuntimeResolution({
    installAnchor: path.join(install, 'package.json'), profile, home,
  });
  const peer = resolution.entries.find(entry => entry.name === '@deepseek-ai/dsh-mcp-client');
  assert.equal(peer?.scope, 'installation');
  const peerInProfile = path.join(profileDir, 'node_modules', '@deepseek-ai', 'dsh-mcp-client');
  assert.equal(await fs.access(peerInProfile).then(() => true, () => false), false);
  const { Context } = await import(pathToFileURL(requireHost.resolve('@deepseek-ai/cordis')).href);
  const { default: ToolRuntime } = await import(pathToFileURL(requireHost.resolve('@deepseek-ai/dsh-tools')).href);
  const { default: SystemPrompt } = await import(pathToFileURL(requireHost.resolve('@deepseek-ai/dsh-system-prompt')).href);
  const registration = runtime.installRuntimeInterception(resolution);
  const previousCache = process.env.XNIP_NPM_CACHE_DIR;
  process.env.XNIP_NPM_CACHE_DIR = path.join(root, 'immutable-cache');
  const ctx = new Context();
  try {
    const patches = runtime.loadOptionalPatches('xnip-profile-test', path.join(packageDir, 'integrations/dsh/cordis.patch.yml'));
    const inserted = patches.flatMap(patch => patch.insert ?? []);
    assert.equal(inserted.length, 1);
    const plugin = await import(inserted[0].name);
    await ctx.plugin(SystemPrompt);
    await ctx.plugin(ToolRuntime);
    // The official resolver routes the plugin's own bare peer import to the host.
    await ctx.plugin(plugin, { ...inserted[0].config, cwd });
    const tools = ctx.tools.schemas().filter(item => item.name.startsWith('mcp__xnip__'));
    assert.equal(tools.length, 5);
    const result = await ctx.tools.execute({
      signal: new AbortController().signal, callId: 'profile-default-write',
      name: 'mcp__xnip__xnip_edit',
      arguments: { file: 'from-profile.txt', operation: 'write', text: '真实\r\nEOF' },
    });
    assert.notEqual(result.isError, true);
    assert.deepEqual(await fs.readFile(path.join(cwd, 'from-profile.txt')), Buffer.from('真实\r\nEOF', 'utf8'));
    assert.equal(await fs.access(peerInProfile).then(() => true, () => false), false);
  } finally {
    await ctx.fiber.dispose();
    registration.dispose();
    if (previousCache === undefined) delete process.env.XNIP_NPM_CACHE_DIR;
    else process.env.XNIP_NPM_CACHE_DIR = previousCache;
    const resolved = await fs.realpath(root);
    assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
    assert.ok(path.basename(resolved).startsWith('xnip dsh profile-'));
    await fs.rm(resolved, { recursive: true, force: true });
  }
});
