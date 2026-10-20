/** Real official DSH bridge -> real xnip MCP; opt in with the isolated runtime. */
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const test = require('node:test');

const runtimePath = process.env.XNIP_DSH_TEST_RUNTIME;
const executable = process.env.XNIP_DSH_TEST_BINARY;

test('the official DSH bundle loader anchors the package patch to its own entry', {
  skip: !runtimePath ? 'Set XNIP_DSH_TEST_RUNTIME for the official bundle-loader check' : false,
}, async (t) => {
  const runtime = await import(pathToFileURL(runtimePath).href);
  if (typeof runtime.loadOptionalPatches !== 'function') {
    return t.skip('the isolated runtime has not prepared its official app-boot loader export');
  }
  const integrationDir = path.resolve(__dirname, '../integrations/dsh');
  const patches = runtime.loadOptionalPatches('xnip-dsh-test', path.join(integrationDir, 'cordis.patch.yml'));
  const inserted = patches.flatMap(patch => patch.insert ?? []);
  assert.equal(inserted.length, 1);
  assert.equal(inserted[0].name, pathToFileURL(path.join(integrationDir, 'index.mjs')).href);
  assert.deepEqual(inserted[0].config, {});
  assert.equal(typeof (await import(inserted[0].name)).apply, 'function');
});

test('official DSH MCP bridge discovers xnip and performs byte-exact edits in its configured cwd', {
  skip: !runtimePath && !executable ? 'Set XNIP_DSH_TEST_RUNTIME and XNIP_DSH_TEST_BINARY for the real integration' : false,
  timeout: 45000,
}, async () => {
  assert.ok(runtimePath && executable, 'both real integration paths are required');
  assert.ok(path.isAbsolute(runtimePath) && path.isAbsolute(executable), 'integration paths must be absolute');
  const runtime = await import(pathToFileURL(runtimePath).href);
  const { createXnipPlugin } = await import(pathToFileURL(path.resolve(__dirname, '../integrations/dsh/index.mjs')).href);
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'xnip dsh integration-'));
  const ctx = new runtime.Context();
  let sequence = 0;
  const text = result => result.content.filter(item => item.type === 'text').map(item => item.text).join('\n');
  const call = (name, arguments_) => ctx.tools.execute({
    signal: new AbortController().signal,
    callId: `xnip-dsh-${++sequence}`,
    name: `mcp__xnip__${name}`,
    arguments: arguments_,
  });
  try {
    await fs.writeFile(path.join(root, 'a.txt'), Buffer.from('标题\r\nvalue=old', 'utf8'));
    await fs.writeFile(path.join(root, 'b.txt'), 'one=1\n');
    await ctx.plugin(runtime.SystemPrompt);
    await ctx.plugin(runtime.ToolRuntime);
    await ctx.plugin(createXnipPlugin({
      resolveNativeExecutable: () => executable,
      loadMcpClient: async () => runtime.McpClient,
    }), { cwd: root });
    const names = ctx.tools.schemas().map(item => item.name).filter(name => name.startsWith('mcp__xnip__')).sort();
    assert.deepEqual(names, ['xnip_apply', 'xnip_edit', 'xnip_find', 'xnip_peek', 'xnip_recover']
      .map(name => 'mcp__xnip__' + name).sort());
    const edited = await call('xnip_edit', { file: 'a.txt', old: 'value=old', text: 'value=new' });
    assert.notEqual(edited.isError, true);
    assert.match(text(edited), /written/);
    const expected = Buffer.from('标题\r\nvalue=new', 'utf8');
    assert.deepEqual(await fs.readFile(path.join(root, 'a.txt')), expected);
    const preview = await call('xnip_edit', { file: 'a.txt', old: 'value=new', text: 'value=preview', dry_run: true });
    assert.notEqual(preview.isError, true);
    assert.match(text(preview), /written=false/);
    assert.deepEqual(await fs.readFile(path.join(root, 'a.txt')), expected);
    const checked = await call('xnip_edit', { file: 'a.txt', old: 'value=new', text: 'value=checked', check: true });
    assert.notEqual(checked.isError, true);
    assert.match(text(checked), /written=false/);
    assert.deepEqual(await fs.readFile(path.join(root, 'a.txt')), expected);
    const batch = await call('xnip_apply', { files: {
      'a.txt': [['value=new', 'value=final']],
      'b.txt': [['one=1', 'one=2']],
    } });
    assert.notEqual(batch.isError, true);
    assert.deepEqual(await fs.readFile(path.join(root, 'a.txt')), Buffer.from('标题\r\nvalue=final', 'utf8'));
    assert.deepEqual(await fs.readFile(path.join(root, 'b.txt')), Buffer.from('one=2\n', 'utf8'));
  } finally {
    await ctx.fiber.dispose();
    const resolvedRoot = await fs.realpath(root);
    assert.equal(path.dirname(resolvedRoot), await fs.realpath(os.tmpdir()));
    assert.ok(path.basename(resolvedRoot).startsWith('xnip dsh integration-'));
    await fs.rm(resolvedRoot, { recursive: true, force: true });
  }
});
