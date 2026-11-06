import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { createFixture, host, COMMIT, VERSION } from './pi_native_runtime.mjs';

const root = path.resolve(import.meta.dirname, '..');
const executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip');
async function fixture(t, options = {}) {
  const cwd = await fs.mkdtemp(path.join(root, 'target/pi-native-contract-'));
  t.after(() => fs.rm(cwd, { recursive: true, force: true }));
  const runtime = await createFixture({ cwd, executable, ...options });
  t.after(() => runtime.close());
  return { cwd, ...runtime };
}
const text = outcome => outcome.result.content.filter(x => x.type === 'text').map(x => x.text).join('\n');
const edit = (runtime, edits, signal) => runtime.call('edit', { path: 'a', edits }, signal);

test('official Pi loader replaces the three names and retains host schema, prompts and renderers', async t => {
  const runtime = await fixture(t);
  assert.equal(VERSION, '1.3.0');
  assert.equal(COMMIT, 'fdce48eb404ec014adaf2ac10a3a53bc6b1740c2');
  assert.deepEqual(runtime.session.getActiveToolNames().sort(), ['edit', 'read', 'write']);
  assert.deepEqual(runtime.session.getAllTools().map(x => x.name).sort(), ['edit', 'read', 'write']);
  for (const name of ['read', 'edit', 'write']) {
    const actual = runtime.session.getToolDefinition(name);
    const expected = host[`create${name[0].toUpperCase()}${name.slice(1)}ToolDefinition`](runtime.cwd);
    for (const field of ['name', 'label', 'description', 'parameters', 'outputSchema',
      'constrainedSampling', 'promptSnippet', 'promptGuidelines', 'renderShell']) {
      assert.deepEqual(actual[field], expected[field], `${name}.${field}`);
    }
    for (const field of ['prepareArguments', 'renderCall', 'renderResult']) {
      assert.equal(actual[field], expected[field], `${name}.${field}`);
    }
    const entry = runtime.session.getAllTools().find(x => x.name === name);
    assert.match(JSON.stringify(entry.sourceInfo), /xnip-native\.mjs/);
  }
  assert.match(runtime.session.systemPrompt, /edits\[\]\.oldText/);
});

test('settings removal plus real session reload restores builtin registry and edit execution', async t => {
  const runtime = await fixture(t);
  await fs.writeFile(path.join(runtime.cwd, 'a'), 'old\n');
  const replacement = runtime.session.getToolDefinition('edit').execute;
  assert.equal((await edit(runtime, [{ oldText: 'old', newText: 'native' }])).isError, false);
  await runtime.setNative(false);
  assert.deepEqual(runtime.session.getActiveToolNames().sort(), ['edit', 'read', 'write']);
  const restored = runtime.session.getToolDefinition('edit');
  assert.notEqual(restored.execute, replacement);
  assert.match(JSON.stringify(runtime.session.getAllTools().find(x => x.name === 'edit').sourceInfo), /builtin/);
  assert.equal((await edit(runtime, [{ oldText: 'native', newText: 'builtin' }])).isError, false);
  await runtime.setNative(true);
  assert.match(JSON.stringify(runtime.session.getAllTools().find(x => x.name === 'edit').sourceInfo), /xnip-native/);
  assert.equal((await edit(runtime, [{ oldText: 'builtin', newText: 'restored' }])).isError, false);
  assert.deepEqual(await fs.readFile(path.join(runtime.cwd, 'a')), Buffer.from('restored\n'));
  assert.equal(runtime.session.isIdle, true);
  assert.equal(runtime.session.agent.state.messages.length, 0);
  assert.equal(runtime.modelCalls, 0);
  assert.deepEqual(runtime.errors, []);
});

test('actual tool_call permission hooks and tool_result transformations wrap native IO', async t => {
  const calls = [], results = [];
  const runtime = await fixture(t, { hooks(pi) {
    pi.on('tool_call', event => {
      calls.push(event.toolName);
      if (event.toolName === 'write') return { block: true, reason: 'Fixture approval required' };
    });
    pi.on('tool_result', event => {
      results.push(event.toolName);
      if (event.toolName === 'edit') return { content: [...event.content, { type: 'text', text: 'Reviewed by fixture' }] };
    });
  } });
  await fs.writeFile(path.join(runtime.cwd, 'a'), 'old\n');
  const blocked = await runtime.call('write', { path: 'a', content: 'blocked' });
  assert.equal(blocked.isError, true);
  assert.match(text(blocked), /Fixture approval required/);
  assert.deepEqual(await fs.readFile(path.join(runtime.cwd, 'a')), Buffer.from('old\n'));
  const success = await edit(runtime, [{ oldText: 'old', newText: 'new' }]);
  assert.equal(success.isError, false, text(success));
  assert.match(text(success), /Reviewed by fixture/);
  assert.deepEqual(calls, ['write', 'edit']);
  assert.ok(results.includes('edit'));
  assert.deepEqual(await fs.readFile(path.join(runtime.cwd, 'a')), Buffer.from('new\n'));
});

test('original-coordinate multi-edit, native fuzzy matching and UI diff exactly match Pi', async t => {
  const adapted = await fixture(t), original = await fixture(t, { native: false });
  for (const [source, edits] of [
    ['\ufeffone=1\r\ntwo=2', [{ oldText: 'one=1', newText: 'one=1000' }, { oldText: 'two=2', newText: 'two=20' }]],
    ['one=1\r\ntwo=2\nthree=3\r\n', [{ oldText: 'two=2', newText: 'two=20' }]],
    ['keep  \nconst title = “old”;\nend  \n', [{ oldText: 'const title = "old";', newText: 'const title = "new";' }]],
    ['ababa', [{ oldText: 'aba', newText: 'X' }]],
    ['a', [{ oldText: 'a', newText: 'b' }, { oldText: 'b', newText: 'c' }]],
    ['abcde', [{ oldText: 'abc', newText: 'X' }, { oldText: 'bcd', newText: 'Y' }]],
    ['same same', [{ oldText: 'same', newText: 'new' }]],
    ['old', [{ oldText: 'missing', newText: 'new' }]],
    ['old', [{ oldText: 'old', newText: 'old' }]],
    ['old', [{ oldText: '', newText: 'new' }]],
  ]) {
    for (const runtime of [adapted, original]) await fs.writeFile(path.join(runtime.cwd, 'a'), source);
    const actual = await edit(adapted, edits), expected = await edit(original, edits);
    assert.equal(actual.isError, expected.isError, JSON.stringify(edits));
    assert.deepEqual(actual.result, expected.result, JSON.stringify(edits));
    assert.deepEqual(await fs.readFile(path.join(adapted.cwd, 'a')), await fs.readFile(path.join(original.cwd, 'a')));
  }
});

test('actual mutation queue, argument preparation and cancellation preserve final bytes', async t => {
  const runtime = await fixture(t);
  await fs.writeFile(path.join(runtime.cwd, 'a'), 'start\n');
  const [first, second] = await Promise.all([
    edit(runtime, [{ oldText: 'start', newText: 'middle' }]),
    edit(runtime, [{ oldText: 'middle', newText: 'done' }]),
  ]);
  for (const result of [first, second]) assert.equal(result.isError, false, text(result));
  const prepared = await edit(runtime, JSON.stringify([{ oldText: 'done', newText: 'prepared' }]));
  assert.equal(prepared.isError, false, text(prepared));
  const controller = new AbortController(); controller.abort();
  for (const [name, args] of [
    ['edit', { path: 'a', edits: [{ oldText: 'prepared', newText: 'later' }] }],
    ['write', { path: 'a', content: 'later' }],
    ['read', { path: 'a' }],
  ]) {
    const result = await runtime.call(name, args, controller.signal);
    assert.equal(result.isError, true);
    assert.match(text(result), /abort/i);
  }
  assert.deepEqual(await fs.readFile(path.join(runtime.cwd, 'a')), Buffer.from('prepared\n'));
  const invalid = await edit(runtime, [{ oldText: 1, newText: 'bad' }]);
  assert.equal(invalid.isError, true);
  assert.deepEqual(await fs.readFile(path.join(runtime.cwd, 'a')), Buffer.from('prepared\n'));
});

test('read pagination, truncation, image output and write results retain builtin shape', async t => {
  const adapted = await fixture(t), original = await fixture(t, { native: false });
  const image = Buffer.from('R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7', 'base64');
  for (const runtime of [adapted, original]) {
    await fs.writeFile(path.join(runtime.cwd, 'a'), 'first\nsecond\nthird\n');
    await fs.writeFile(path.join(runtime.cwd, 'long'), 'x'.repeat(60_000));
    await fs.writeFile(path.join(runtime.cwd, 'pixel.gif'), image);
  }
  for (const args of [{ path: 'a' }, { path: 'a', offset: 2, limit: 1 },
    { path: 'long' }, { path: 'pixel.gif' }]) {
    const actual = await adapted.call('read', args), expected = await original.call('read', args);
    assert.equal(actual.isError, expected.isError);
    assert.deepEqual(actual.result, expected.result);
  }
  for (const content of ['中\r\nno-eof', '']) {
    const args = { path: 'new/deep/a', content };
    const actual = await adapted.call('write', args), expected = await original.call('write', args);
    assert.equal(actual.isError, false, text(actual));
    assert.deepEqual(actual.result, expected.result);
    assert.deepEqual(await fs.readFile(path.join(adapted.cwd, args.path)), Buffer.from(content));
  }
});

test('existing final symlinks retain the link and update the regular target', async t => {
  const runtime = await fixture(t);
  await fs.writeFile(path.join(runtime.cwd, 'target'), 'old\n');
  await fs.symlink('target', path.join(runtime.cwd, 'a'));
  const edited = await edit(runtime, [{ oldText: 'old', newText: 'new' }]);
  assert.equal(edited.isError, false, text(edited));
  const written = await runtime.call('write', { path: 'a', content: 'written\n' });
  assert.equal(written.isError, false, text(written));
  assert.equal(await fs.readlink(path.join(runtime.cwd, 'a')), 'target');
  assert.deepEqual(await fs.readFile(path.join(runtime.cwd, 'target')), Buffer.from('written\n'));
});

test('binary byte transport retains the builtin UTF-8 decoder and overwrite behavior', async t => {
  const adapted = await fixture(t), original = await fixture(t, { native: false });
  const source = Buffer.from([0xff, 0x0a, ...Buffer.from('old\n')]);
  for (const runtime of [adapted, original]) await fs.writeFile(path.join(runtime.cwd, 'a'), source);
  const read = await adapted.call('read', { path: 'a' });
  const expectedRead = await original.call('read', { path: 'a' });
  assert.deepEqual(read.result, expectedRead.result);
  const actual = await edit(adapted, [{ oldText: 'old', newText: 'new' }]);
  const expected = await edit(original, [{ oldText: 'old', newText: 'new' }]);
  assert.equal(actual.isError, false, text(actual));
  assert.deepEqual(actual.result, expected.result);
  assert.deepEqual(await fs.readFile(path.join(adapted.cwd, 'a')), await fs.readFile(path.join(original.cwd, 'a')));
  await fs.writeFile(path.join(adapted.cwd, 'a'), source);
  const overwritten = await adapted.call('write', { path: 'a', content: 'plain\n' });
  assert.equal(overwritten.isError, false, text(overwritten));
  assert.deepEqual(await fs.readFile(path.join(adapted.cwd, 'a')), Buffer.from('plain\n'));
});

test('Pi string arguments retain Node UTF-8 encoding for lone UTF-16 surrogates', async t => {
  const adapted = await fixture(t), original = await fixture(t, { native: false });
  const file = 'new-\ud800.txt';
  const args = { path: file, content: 'left\ud800right\n' };
  const actualWrite = await adapted.call('write', args), expectedWrite = await original.call('write', args);
  assert.equal(actualWrite.isError, false, text(actualWrite));
  assert.deepEqual(actualWrite.result, expectedWrite.result);
  for (const runtime of [adapted, original]) {
    assert.deepEqual(await fs.readFile(path.join(runtime.cwd, file)), Buffer.from(args.content));
  }
  const actualRead = await adapted.call('read', { path: file }), expectedRead = await original.call('read', { path: file });
  assert.deepEqual(actualRead.result, expectedRead.result);
  const edits = { path: file, edits: [{ oldText: 'right', newText: 'new-\udc00' }] };
  const actualEdit = await adapted.call('edit', edits), expectedEdit = await original.call('edit', edits);
  assert.equal(actualEdit.isError, false, text(actualEdit));
  assert.deepEqual(actualEdit.result, expectedEdit.result);
  assert.deepEqual(await fs.readFile(path.join(adapted.cwd, file)), await fs.readFile(path.join(original.cwd, file)));
});

test('snapshot and atomic-replacement permission boundaries preserve target bytes',
  { skip: process.platform === 'win32' || process.getuid?.() === 0 }, async t => {
  const adapted = await fixture(t), original = await fixture(t, { native: false });
  for (const runtime of [adapted, original]) {
    await fs.writeFile(path.join(runtime.cwd, 'a'), 'old\n', { mode: 0o200 });
  }
  const adaptedWriteOnly = await adapted.call('write', { path: 'a', content: 'new\n' });
  const originalWriteOnly = await original.call('write', { path: 'a', content: 'new\n' });
  assert.equal(adaptedWriteOnly.isError, true);
  assert.match(text(adaptedWriteOnly), /permission denied/i);
  assert.equal(originalWriteOnly.isError, false, text(originalWriteOnly));
  for (const runtime of [adapted, original]) await fs.chmod(path.join(runtime.cwd, 'a'), 0o600);
  assert.deepEqual(await fs.readFile(path.join(adapted.cwd, 'a')), Buffer.from('old\n'));
  assert.deepEqual(await fs.readFile(path.join(original.cwd, 'a')), Buffer.from('new\n'));
  for (const runtime of [adapted, original]) {
    await fs.mkdir(path.join(runtime.cwd, 'locked'));
    await fs.writeFile(path.join(runtime.cwd, 'locked/a'), 'old\n');
    await fs.chmod(path.join(runtime.cwd, 'locked'), 0o500);
    t.after(() => fs.chmod(path.join(runtime.cwd, 'locked'), 0o700).catch(() => {}));
  }
  const adaptedDirectory = await adapted.call('write', { path: 'locked/a', content: 'new\n' });
  const originalDirectory = await original.call('write', { path: 'locked/a', content: 'new\n' });
  assert.equal(adaptedDirectory.isError, true);
  assert.match(text(adaptedDirectory), /permission denied/i);
  assert.equal(originalDirectory.isError, false, text(originalDirectory));
  for (const runtime of [adapted, original]) await fs.chmod(path.join(runtime.cwd, 'locked'), 0o700);
  assert.deepEqual(await fs.readFile(path.join(adapted.cwd, 'locked/a')), Buffer.from('old\n'));
  assert.deepEqual(await fs.readFile(path.join(original.cwd, 'locked/a')), Buffer.from('new\n'));
});
