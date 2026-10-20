/** Configuration contracts only: no filesystem editor or MCP process is simulated. */
const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const test = require('node:test');

const entry = path.resolve(__dirname, '../integrations/dsh/index.mjs');
const imported = import(pathToFileURL(entry).href);
const native = () => process.execPath;

test('the entry is an explicit Cordis plugin and imports without resolving a binary', async () => {
  const mod = await imported;
  assert.equal(mod.name, 'xnip-mcp');
  assert.deepEqual(mod.inject, ['tools']);
  assert.equal(mod.apply, mod.default.apply);
  assert.equal(typeof mod.createXnipPlugin, 'function');
});

test('default configuration uses the native resolver and an explicit text-receipt stdio process', async () => {
  const { createMcpConfig } = await imported;
  let resolutions = 0;
  const config = createMcpConfig({}, () => { resolutions++; return process.execPath; });
  assert.equal(resolutions, 1);
  assert.equal(config.command, process.execPath);
  assert.equal(config.transport, 'stdio');
  assert.equal(config.serverName, 'xnip');
  assert.deepEqual(config.args, ['mcp', '--receipts', 'text']);
  assert.equal(config.cwd, process.cwd());
  assert.deepEqual(config.env, { XNIP_AUTO_UPDATE: '0' });
  assert.equal(config.failOnStartupError, true);
});

test('custom cwd and environment are forwarded without mutating caller or ambient values', async () => {
  const { createMcpConfig } = await imported;
  const env = Object.freeze({ SAMPLE_VALUE: 'value with spaces', XNIP_AUTO_UPDATE: '1' });
  const config = Object.freeze({ cwd: './workspace with spaces', env, serverName: 'project-xnip',
    receipts: 'structured', toolset: 'full' });
  const before = JSON.stringify({ ...process.env });
  const result = createMcpConfig(config, native);
  assert.equal(result.cwd, path.resolve('./workspace with spaces'));
  assert.equal(result.serverName, 'project-xnip');
  assert.deepEqual(result.args, ['mcp', '--receipts', 'structured', '--toolset', 'full']);
  assert.deepEqual(result.env, { SAMPLE_VALUE: 'value with spaces', XNIP_AUTO_UPDATE: '0' });
  assert.equal(env.XNIP_AUTO_UPDATE, '1');
  assert.ok(JSON.stringify({ ...process.env }) === before, 'ambient environment must remain unchanged');
});

test('invalid configuration fails before native resolution or any plugin registration', async () => {
  const { createMcpConfig } = await imported;
  const unused = () => { throw new Error('must not resolve'); };
  for (const config of [null, [], { cwd: '' }, { receipts: 'compact' }, { toolset: 'all' },
    { serverName: '' }, { env: { VALUE: 1 } }, { command: 'arbitrary-editor' }]) {
    assert.throws(() => createMcpConfig(config, unused), TypeError);
  }
  assert.throws(() => createMcpConfig({}, () => 'relative-xnip'), /absolute executable path/);
});

test('resolver and official bridge loading failures propagate without registering a replacement', async () => {
  const { createXnipPlugin } = await imported;
  const failure = new Error('native unavailable');
  const ctx = { plugin() { assert.fail('must not register before dependencies are ready'); } };
  await assert.rejects(createXnipPlugin({ resolveNativeExecutable() { throw failure; } }).apply(ctx), error => error === failure);
  await assert.rejects(createXnipPlugin({ resolveNativeExecutable: native,
    loadMcpClient() { throw failure; } }).apply(ctx), error => error === failure);
  await assert.rejects(createXnipPlugin({ resolveNativeExecutable: native,
    loadMcpClient: async () => ({}) }).apply(ctx), /official DSH MCP client/);
});
