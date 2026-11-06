import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import { createFixture } from './opencode_native_runtime.mjs';
import { createXnipPlugin } from '../integrations/opencode/plugin.mjs';
import { EditorClient } from '../integrations/native/editor-client.mjs';
import { HOST } from '../benches/native/opencode_native_setup.mjs';

const root = path.resolve(import.meta.dirname, '..');
async function fixture(t, options = {}) {
  const f = await createFixture(options);
  t.after(() => f.close());
  return f;
}
const success = (result) => {
  assert.equal(result.ok, true, result.error);
  return result.value.result;
};
const edit = { filePath: 'a.txt', oldString: 'old', newString: 'new' };

test('actual OpenCode registry loads four native plugin tools and closes the owned process', async (t) => {
  const f = await fixture(t);
  const catalog = await f.describe();
  for (const name of ['xnip_edit', 'xnip_write', 'xnip_apply', 'xnip_read'])
    assert.equal(catalog.tools[name], true);
  success(await f.call('xnip_write', { filePath: 'a.txt', content: 'old\r\n尾巴' }));
  assert.equal(success(await f.call('xnip_read', { filePath: 'a.txt' })).output, 'old\r\n尾巴');
  assert.equal(success(await f.call('xnip_edit', edit)).output, 'written');
  assert.deepEqual(await fs.readFile(path.join(f.cwd, 'a.txt')), Buffer.from('new\r\n尾巴'));
  const events = await f.events();
  assert.ok(events.some((e) => e.event === 'request' && e.action === 'commit'));
  const spawns = events.filter((e) => e.event === 'spawn').map((e) => e.pid),
    exits = events.filter((e) => e.event === 'exit').map((e) => e.pid);
  assert.deepEqual(spawns, exits);
  assert.equal(f.modelCalls, 0);
});
test('malformed edit input fails before persistence in the actual CLI tool path', async (t) => {
  const f = await fixture(t);
  await fs.writeFile(path.join(f.cwd, 'a.txt'), 'old');
  const result = await f.call('xnip_edit', { filePath: 'a.txt', newString: 'new' });
  assert.equal(result.ok, false);
  assert.equal(await fs.readFile(path.join(f.cwd, 'a.txt'), 'utf8'), 'old');
});
test('actual edit path permission denial aborts the native plan before commit', async (t) => {
  const f = await fixture(t, { permission: { edit: 'deny' } });
  await fs.writeFile(path.join(f.cwd, 'a.txt'), 'old');
  assert.equal((await f.call('xnip_edit', edit)).ok, false);
  assert.equal(await fs.readFile(path.join(f.cwd, 'a.txt'), 'utf8'), 'old');
  const events = await f.events();
  assert.ok(events.some((e) => e.action === 'plan'));
  assert.ok(events.some((e) => e.action === 'abort'));
  assert.ok(!events.some((e) => e.action === 'commit'));

  const outside = await fs.mkdtemp(path.join(os.tmpdir(), 'xnip-opencode-boundary-'));
  t.after(() => fs.rm(outside, { recursive: true, force: true }));
  const target = path.join(outside, 'file.txt');
  await fs.writeFile(target, 'private\n');
  const denied = await fixture(t, { permission: { external_directory: 'deny' } });
  await fs.symlink(outside, path.join(denied.cwd, 'alias'), 'dir');
  assert.equal((await denied.call('xnip_read', { filePath: target })).ok, false);
  assert.equal((await denied.call('xnip_read', { filePath: 'alias/file.txt' })).ok, false);
  assert.equal(
    (await denied.call('xnip_write', { filePath: 'alias/file.txt', content: 'changed' })).ok,
    false,
  );
  assert.equal(
    (await denied.call('xnip_write', { filePath: 'alias/new/file.txt', content: 'created' })).ok,
    false,
  );
  assert.equal(
    (
      await denied.call('xnip_edit', {
        filePath: 'alias/file.txt',
        oldString: 'private',
        newString: 'changed',
      })
    ).ok,
    false,
  );
  const aliasPatch =
    '*** Begin Patch\n*** Update File: alias/file.txt\n@@\n-private\n+changed\n*** End Patch';
  assert.equal((await denied.call('xnip_apply', { patch: aliasPatch })).ok, false);
  const newAliasPatch = '*** Begin Patch\n*** Add File: alias/created.txt\n+created\n*** End Patch';
  assert.equal((await denied.call('xnip_apply', { patch: newAliasPatch })).ok, false);
  assert.equal(await fs.readFile(target, 'utf8'), 'private\n');
  await assert.rejects(fs.access(path.join(outside, 'new')), { code: 'ENOENT' });
  await assert.rejects(fs.access(path.join(outside, 'created.txt')), { code: 'ENOENT' });
  assert.ok(!(await denied.events()).some((event) => event.action === 'commit'));
  assert.equal(denied.modelCalls, 0);

  // Starting a real non-git workspace through its own symlink keeps internal IO
  // authorized while OpenCode's '/' worktree sentinel grants no outside access.
  const actualRoot = path.join(outside, 'workspace');
  const linkedRoot = path.join(outside, 'workspace-alias');
  await fs.mkdir(actualRoot);
  await fs.symlink(actualRoot, linkedRoot, 'dir');
  const linked = await fixture(t, { cwd: linkedRoot, permission: { external_directory: 'deny' } });
  success(await linked.call('xnip_write', { filePath: 'inside.txt', content: 'inside' }));
  assert.equal(
    success(await linked.call('xnip_read', { filePath: 'inside.txt' })).output,
    'inside',
  );
  assert.equal((await linked.call('xnip_read', { filePath: target })).ok, false);
  assert.equal(await fs.readFile(path.join(actualRoot, 'inside.txt'), 'utf8'), 'inside');
  assert.equal(linked.modelCalls, 0);
});
test('OpenCode tool policy disables a plugin tool in its actual catalog', async (t) => {
  const f = await fixture(t, { permission: { xnip_edit: 'deny' } });
  assert.equal((await f.describe()).tools.xnip_edit, false);
  assert.equal((await f.call('xnip_edit', edit)).ok, false);
  assert.ok(!(await f.events()).some((e) => e.event === 'spawn'));
});
test('real read permission denial reaches the host before starting native IO', async (t) => {
  const f = await fixture(t, { permission: { read: 'deny' } });
  await fs.writeFile(path.join(f.cwd, 'a.txt'), 'private');
  assert.equal((await f.call('xnip_read', { filePath: 'a.txt' })).ok, false);
  assert.ok(!(await f.events()).some((e) => e.event === 'spawn'));
});
test('multi-file patch commits through core and rejects a later invalid hunk atomically', async (t) => {
  const f = await fixture(t);
  await fs.writeFile(path.join(f.cwd, 'a.txt'), 'old\n');
  const patch =
    '*** Begin Patch\n*** Update File: a.txt\n@@\n-old\n+new\n*** Add File: b.txt\n+created\n*** End Patch';
  success(await f.call('xnip_apply', { patch }));
  assert.equal(await fs.readFile(path.join(f.cwd, 'a.txt'), 'utf8'), 'new\n');
  assert.equal(await fs.readFile(path.join(f.cwd, 'b.txt'), 'utf8'), 'created\n');
  const invalid =
    '*** Begin Patch\n*** Update File: a.txt\n@@\n-new\n+changed\n*** Update File: missing.txt\n@@\n-absent\n+created\n*** End Patch';
  assert.equal((await f.call('xnip_apply', { patch: invalid })).ok, false);
  assert.equal(await fs.readFile(path.join(f.cwd, 'a.txt'), 'utf8'), 'new\n');
  const lifecycle =
    '*** Begin Patch\n*** Delete File: b.txt\n*** Update File: a.txt\n*** Move to: moved.txt\n@@\n-new\n+moved\n*** End Patch';
  success(await f.call('xnip_apply', { patch: lifecycle }));
  await assert.rejects(fs.access(path.join(f.cwd, 'a.txt')), { code: 'ENOENT' });
  await assert.rejects(fs.access(path.join(f.cwd, 'b.txt')), { code: 'ENOENT' });
  assert.equal(await fs.readFile(path.join(f.cwd, 'moved.txt'), 'utf8'), 'moved\n');
});
test('exact matching, missing and ambiguous guards preserve byte contracts', async (t) => {
  const f = await fixture(t),
    file = path.join(f.cwd, 'a.txt');
  await fs.writeFile(file, 'old old\r\n');
  assert.equal((await f.call('xnip_edit', edit)).ok, false);
  assert.equal((await f.call('xnip_edit', { ...edit, oldString: 'missing' })).ok, false);
  assert.equal(await fs.readFile(file, 'utf8'), 'old old\r\n');
  success(await f.call('xnip_edit', { ...edit, replaceAll: true }));
  assert.equal(await fs.readFile(file, 'utf8'), 'new new\r\n');
  assert.equal(
    success(
      await f.call('xnip_edit', { ...edit, oldString: 'new', newString: 'new', replaceAll: true }),
    ).output,
    'unchanged',
  );
});
test('external changes after preview survive the committed full-file guard', async (t) => {
  const f = await fixture(t, { mode: 'drift' });
  await fs.writeFile(path.join(f.cwd, 'a.txt'), 'old\r\n');
  assert.equal((await f.call('xnip_edit', edit)).ok, false);
  assert.equal(await fs.readFile(path.join(f.cwd, 'a.txt'), 'utf8'), 'external\r\n');
});
test('binary reads are decoded as UTF-8 and byte budgets disclose truncation', async (t) => {
  const f = await fixture(t);
  const file = path.join(f.cwd, 'a.txt');
  const raw = Buffer.from([65, 0, 255, 66, 67]);
  await fs.writeFile(file, raw);
  const result = success(await f.call('xnip_read', { filePath: 'a.txt', maxBytes: 3 }));
  assert.equal(
    result.output,
    raw.subarray(0, 3).toString('utf8') + '\n[read truncated; next_offset=3; total_bytes=5]',
  );
  assert.equal(result.metadata.total_bytes, 5);
  assert.equal(result.metadata.file_truncated, true);
  assert.equal(result.metadata.truncated, false);
  success(await f.call('xnip_write', { filePath: 'a.txt', content: '\ufeff尾巴\r\n\ud800' }));
  assert.deepEqual(await fs.readFile(file), Buffer.from('\ufeff尾巴\r\n\ud800'));
});
test('plugin removal restores the actual builtin registry and a real builtin write', async (t) => {
  const f = await fixture(t);
  assert.equal((await f.describe()).tools.xnip_write, true);
  await f.setNative(false);
  assert.equal((await f.describe()).tools.xnip_write, undefined);
  success(
    await f.call('write', { filePath: path.join(f.cwd, 'restored.txt'), content: 'restored\r\n' }),
  );
  assert.equal(await fs.readFile(path.join(f.cwd, 'restored.txt'), 'utf8'), 'restored\r\n');
  await f.setMcp(true);
  const connected = await f.management(['mcp', 'list']);
  assert.match(connected.stdout + connected.stderr, /xnip[\s\S]*connected/i);
  await f.setMcp(false);
  const restored = await f.management(['mcp', 'list']);
  assert.doesNotMatch(restored.stdout + restored.stderr, /xnip[\s\S]*connected/i);
});
test('public plugin abort callback releases an actual pending core plan', async (t) => {
  const f = await fixture(t);
  const file = path.join(f.cwd, 'a.txt');
  await fs.writeFile(file, 'old');
  const controller = new AbortController(),
    actions = [];
  const sdk = await import(
    pathToFileURL(path.join(HOST, 'node_modules/@opencode-ai/plugin/dist/tool.js')).href
  );
  const plugin = await createXnipPlugin({
    loadTool: async () => sdk,
    resolveExecutable: () => process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip'),
    createClient: (binary, cwd) => {
      const client = new EditorClient(binary, cwd),
        request = client.request.bind(client);
      client.request = async (action, fields) => {
        actions.push(action);
        const value = await request(action, fields);
        if (action === 'plan') controller.abort();
        return value;
      };
      return client;
    },
  })({ directory: f.cwd, worktree: f.cwd });
  t.after(() => plugin.dispose());
  await assert.rejects(
    plugin.tool.xnip_edit.execute(edit, {
      directory: f.cwd,
      worktree: f.cwd,
      abort: controller.signal,
      async ask() {},
      metadata() {},
    }),
  );
  assert.equal(await fs.readFile(file, 'utf8'), 'old');
  assert.ok(actions.includes('abort'));
  assert.ok(!actions.includes('commit'));
});
