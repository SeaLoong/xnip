import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { createFixture } from '../integrations/dsh-native/runtime.mjs';
import { EditorClient } from '../integrations/dsh-native/editor-client.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const hostRoot = process.env.XNIP_DSH_NATIVE_HOST ?? path.join(root, 'target/native-hosts/dsh-0.2.1-rc.5');
const executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip');

test('official DSH provider preserves catalog, byte edits, observations, sandbox and disposal', async t => {
  const cwd = await fs.mkdtemp(path.join(root, 'target/dsh-native-'));
  t.after(() => fs.rm(cwd, { recursive: true, force: true }));
  const fixture = await createFixture({ hostRoot, cwd, executable });
  t.after(() => fixture.close());
  const original = await createFixture({ hostRoot, cwd, native: false });
  t.after(() => original.close());
  const observed = [];
  fixture.ctx.on('fs/observed', (target, observation) => observed.push({ target, observation }));
  const oldCatalog = await original.describe();
  const newCatalog = await fixture.describe();
  assert.deepEqual(newCatalog.tools, oldCatalog.tools);
  assert.equal(newCatalog.prompt, oldCatalog.prompt);
  assert.deepEqual(newCatalog.tools.map(tool => tool.name), ['read', 'write', 'edit']);
  const created = path.join(cwd, 'created.txt');
  const written = await fixture.call('write', { file_path: created, content: 'created\r\n尾巴' });
  assert.equal(written.isError, false, JSON.stringify(written));
  assert.equal(written.value.operation, 'create');
  assert.equal(written.value.before, null);
  assert.equal(await fs.readFile(created, 'utf8'), 'created\r\n尾巴');
  const refreshed = await fixture.call('edit', { file_path: created, old_string: 'created', new_string: 'updated' });
  assert.equal(refreshed.isError, false, JSON.stringify(refreshed));
  assert.equal(await fs.readFile(created, 'utf8'), 'updated\r\n尾巴');
  const large = path.join(cwd, 'large.txt');
  const prefix = Buffer.from('\ufeff' + 'x'.repeat(5 * 1024 * 1024) + '\r\n');
  await fs.writeFile(large, Buffer.concat([prefix, Buffer.from('old\r\nblock\r\nEOF')]));
  assert.equal((await fixture.call('read', { file_path: large })).isError, false);
  const largeEdit = await fixture.call('edit', { file_path: large, old_string: 'old\nblock', new_string: 'new\nblock' });
  assert.equal(largeEdit.isError, false, JSON.stringify(largeEdit));
  assert.deepEqual(await fs.readFile(large), Buffer.concat([prefix, Buffer.from('new\nblock\r\nEOF')]));
  const file = path.join(cwd, 'a.txt');
  await fs.writeFile(file, 'keep\r\nold\r\n尾巴');
  const edit = { file_path: file, old_string: 'old', new_string: 'new' };
  let result = await fixture.call('edit', edit);
  assert.equal(result.isError, true);
  assert.match(JSON.stringify(result), /FS_NOT_OBSERVED/);
  assert.equal((await fixture.call('read', { file_path: file })).isError, false);
  result = await fixture.call('edit', edit);
  assert.equal(result.isError, false, JSON.stringify(result));
  assert.equal(await fs.readFile(file, 'utf8'), 'keep\r\nnew\r\n尾巴');
  assert.ok(observed.length >= 2);
  const after = await fixture.ctx.fs.stat(await fixture.ctx.fs.resolve(file));
  assert.match(after.version, /^\d+:\d+:\d+:\d+:\d+$/);
  result = await fixture.call('edit', { ...edit, old_string: 'new', new_string: 'final' });
  assert.equal(result.isError, false, JSON.stringify(result));
  await fs.writeFile(file, 'external');
  result = await fixture.call('edit', { ...edit, old_string: 'external' });
  assert.equal(result.isError, true);
  assert.match(JSON.stringify(result), /FS_STALE_VERSION/);
  assert.equal(await fs.readFile(file, 'utf8'), 'external');
  // Host reads normalize CRLF; mixed storage still preserves untouched bytes.
  await fs.writeFile(file, 'A\r\nold\nA\nold\r\nEOF');
  await fixture.call('read', { file_path: file });
  result = await fixture.call('edit', { file_path: file, old_string: 'A\nold', new_string: 'B\nnew' });
  assert.equal(result.isError, true);
  assert.match(JSON.stringify(result), /FS_AMBIGUOUS_EDIT/);
  assert.equal(await fs.readFile(file, 'utf8'), 'A\r\nold\nA\nold\r\nEOF');
  result = await fixture.call('edit', { file_path: file, old_string: 'A\nold', new_string: 'B\nnew', replace_all: true });
  assert.equal(result.isError, false, JSON.stringify(result));
  assert.equal(await fs.readFile(file, 'utf8'), 'B\nnew\nB\nnew\r\nEOF');
  await fs.writeFile(file, 'aaaa\r\n');
  await fixture.call('read', { file_path: file });
  result = await fixture.call('edit', { file_path: file, old_string: 'aaa', new_string: 'X' });
  assert.equal(result.isError, true);
  assert.match(JSON.stringify(result), /FS_AMBIGUOUS_EDIT/);
  assert.equal(await fs.readFile(file, 'utf8'), 'aaaa\r\n');
  const alias = path.join(cwd, 'alias.txt');
  await fs.symlink(file, alias);
  const resolved = await fixture.ctx.fs.resolve(file);
  const throughLink = await fixture.ctx.fs.resolve(alias);
  assert.equal(resolved.targetKey, throughLink.targetKey);
  const version = (await fixture.ctx.fs.stat(resolved)).version;
  const outcomes = await Promise.allSettled([
    fixture.ctx.fs.editText(resolved, { oldString: 'aaaa', newString: 'first', replaceAll: false }, { version }),
    fixture.ctx.fs.editText(throughLink, { oldString: 'aaaa', newString: 'second', replaceAll: false }, { version }),
  ]);
  assert.equal(outcomes.filter(outcome => outcome.status === 'fulfilled').length, 1);
  assert.equal(outcomes.find(outcome => outcome.status === 'rejected').reason.code, 'FS_STALE_VERSION');
  assert.equal((await fs.lstat(alias)).isSymbolicLink(), true);
  assert.equal(await fs.readFile(file, 'utf8'), 'first\r\n');
  // A real disk mutation between core planning and commit must be host-stale.
  await fixture.call('read', { file_path: file });
  const request = fixture.ctx.fs.editor.request.bind(fixture.ctx.fs.editor);
  fixture.ctx.fs.editor.request = async (action, fields) => {
    const value = await request(action, fields);
    if (action === 'plan') await fs.writeFile(file, 'race');
    return value;
  };
  result = await fixture.call('edit', { file_path: file, old_string: 'first', new_string: 'must not commit' });
  assert.equal(result.isError, true);
  assert.match(JSON.stringify(result), /FS_STALE_VERSION/);
  assert.equal(await fs.readFile(file, 'utf8'), 'race');
  fixture.ctx.fs.editor.request = request;
  await fs.writeFile(file, Buffer.from('\ufeffkeep\r\nold\r\n尾巴'));
  await fixture.call('read', { file_path: file });
  result = await fixture.call('edit', { file_path: file, old_string: 'old\n尾巴', new_string: 'new\n尾巴' });
  assert.equal(result.isError, false, JSON.stringify(result));
  assert.deepEqual(await fs.readFile(file), Buffer.from('\ufeffkeep\r\nnew\n尾巴'));
  assert.equal(result.value.before, 'keep\nold\n尾巴');
  assert.equal(result.value.after, 'keep\nnew\n尾巴');
  const cancel = new AbortController();
  fixture.ctx.fs.editor.request = async (action, fields) => {
    if (action === 'commit') cancel.abort();
    return request(action, fields);
  };
  const cancelTarget = await fixture.ctx.fs.resolve(file);
  const cancelBefore = await fs.readFile(file);
  await assert.rejects(fixture.ctx.fs.editText(cancelTarget,
    { oldString: 'new', newString: 'canceled', replaceAll: false }, undefined, cancel.signal),
    error => error.code === 'FS_ABORTED');
  assert.deepEqual(await fs.readFile(file), cancelBefore);
  fixture.ctx.fs.editor.request = request;
  const failures = path.join(cwd, 'lf-errors.txt');
  await fs.writeFile(failures, 'repeat repeat\n');
  for (const runtime of [original, fixture]) {
    await runtime.call('read', { file_path: failures });
    for (const [old_string, code] of [['missing', 'FS_EDIT_NOT_FOUND'], ['repeat', 'FS_AMBIGUOUS_EDIT']]) {
      const failure = await runtime.call('edit', { file_path: failures, old_string, new_string: 'fixed' });
      assert.equal(failure.isError, true);
      assert.ok(JSON.stringify(failure).includes(code), JSON.stringify(failure));
      assert.equal(await fs.readFile(failures, 'utf8'), 'repeat repeat\n');
    }
  }
  const surrogatePath = path.join(cwd, 'surrogate.txt');
  result = await fixture.call('write', { file_path: surrogatePath, content: 'old\r\n' });
  assert.equal(result.isError, false, JSON.stringify(result));
  result = await fixture.call('edit', { file_path: surrogatePath, old_string: 'old', new_string: 'new\ud800' });
  assert.equal(result.isError, false, JSON.stringify(result));
  assert.deepEqual(await fs.readFile(surrogatePath), Buffer.from('new\ud800\r\n'));
  await fs.writeFile(file, 'race');
  const target = await fixture.ctx.fs.resolve(file);
  await assert.rejects(fixture.ctx.fs.writeText(target, 'denied', undefined, undefined,
    { mode: 'read-only', workspaceRoot: cwd }), error => error.code === 'FS_SANDBOX_DENIED');
  assert.equal(await fs.readFile(file, 'utf8'), 'race');
  const outside = path.join(root, 'target', `outside-${path.basename(cwd)}.txt`);
  t.after(() => fs.rm(outside, { force: true }));
  await fs.writeFile(outside, 'outside');
  await fixture.call('read', { file_path: outside });
  result = await fixture.call('edit', { file_path: outside, old_string: 'outside', new_string: 'bad' });
  assert.equal(result.isError, true);
  assert.match(JSON.stringify(result), /FS_SANDBOX_DENIED/);
  assert.equal(await fs.readFile(outside, 'utf8'), 'outside');
  const deniedApproval = await fixture.call('edit', { ...edit, sandbox_permissions: 'danger-full-access', justification: 'Test fail-closed approval' });
  assert.equal(deniedApproval.isError, true);
  assert.equal(await fs.readFile(file, 'utf8'), 'race');
  const editor = fixture.ctx.fs.editor;
  await fixture.close();
  assert.equal(editor.closed, true);
});

test('native child pipe failure rejects requests without crashing the host', async () => {
  const client = new EditorClient(executable, root);
  try {
    await client.request('capabilities');
    client.child.kill('SIGKILL');
    await assert.rejects(client.request('plan', { operations: [{ op: 'write', path: path.join(root, 'target/unused-pipe-fixture'), text: new Array(1024 * 1024).fill(65) }] }));
  } finally { await client.close(); }
});
