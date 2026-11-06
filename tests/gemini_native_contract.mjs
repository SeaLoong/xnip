import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { prepareFixture, VERSION } from './gemini_native_runtime.mjs';
import { EditorClient } from '../integrations/native/editor-client.mjs';
import { installXnipFileSystem } from '../integrations/gemini-native/index.mjs';

const root = path.resolve(import.meta.dirname, '..');
const executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip');
async function fixture(t, options = {}) {
  const cwd = options.cwd ?? await fs.mkdtemp(path.join(root, 'target/gemini-native-contract-'));
  if (!options.cwd) t.after(() => fs.rm(cwd, { recursive: true, force: true }));
  const runtime = await prepareFixture({ cwd, executable, ...options });
  t.after(() => runtime.close());
  runtime.cwd = cwd;
  return runtime;
}
const editArgs = (runtime, old_string = 'old', new_string = 'new', extra = {}) => ({
  file_path: path.join(runtime.cwd, 'a.txt'), old_string, new_string,
  instruction: 'Apply the supplied literal replacement.', ...extra,
});
const success = result => assert.equal(result.error, undefined, JSON.stringify(result));

test('real Gemini registry keeps original tools and schemas while IO installs and restores', async t => {
  let client;
  const runtime = await fixture(t, { createClient: (binary, cwd) => client = new EditorClient(binary, cwd) });
  const original = await fixture(t, { cwd: runtime.cwd, native: false });
  assert.equal(VERSION, '0.27.2');
  assert.deepEqual(runtime.tools.map(tool => tool.name).sort(), ['read_file', 'replace', 'write_file']);
  assert.deepEqual(runtime.describe().tools, original.describe().tools);
  const file = path.join(runtime.cwd, 'a.txt');
  success(await runtime.call('write_file', { file_path: file, content: 'old\r\n尾巴' }));
  success(await runtime.call('read_file', { file_path: file }));
  success(await runtime.call('replace', editArgs(runtime)));
  assert.deepEqual(await fs.readFile(file), Buffer.from('new\r\n尾巴'));
  const tools = runtime.registry.getAllTools();
  runtime.installation.restore();
  for (const tool of tools) assert.equal(runtime.registry.getTool(tool.name), tool);
  success(await runtime.call('replace', editArgs(runtime, 'new', 'restored')));
  assert.equal(await fs.readFile(file, 'utf8'), 'restored\r\n尾巴');
  await runtime.installation.close();
  assert.ok(client.child.exitCode !== null);
  assert.equal(runtime.modelCalls, 0);
});

test('official ASK_USER confirmation and DENY policy own mutation decisions', async t => {
  const runtime = await fixture(t, { policyDecision: 'ask_user' });
  const file = path.join(runtime.cwd, 'a.txt');
  await fs.writeFile(file, 'old\n');
  const preview = await runtime.call('replace', editArgs(runtime), { confirm: false });
  assert.equal(preview.confirmation.type, 'edit');
  assert.match(preview.confirmation.fileDiff, /\+new/);
  assert.equal(await fs.readFile(file, 'utf8'), 'old\n');
  const cancelled = await runtime.call('replace', editArgs(runtime), { confirm: runtime.sdk.ToolConfirmationOutcome.Cancel });
  assert.equal(cancelled.cancelled, true);
  assert.equal(await fs.readFile(file, 'utf8'), 'old\n');
  success(await runtime.call('replace', editArgs(runtime)));
  assert.equal(await fs.readFile(file, 'utf8'), 'new\n');
  assert.ok(runtime.events.some(event => event.type === 'confirmation-request'));
  const denied = await fixture(t, { policyDecision: 'deny' });
  await fs.writeFile(path.join(denied.cwd, 'a.txt'), 'old\n');
  await assert.rejects(denied.call('replace', editArgs(denied)), /denied|policy|rejected/i);
  assert.equal(await fs.readFile(path.join(denied.cwd, 'a.txt'), 'utf8'), 'old\n');
  assert.equal(denied.modelCalls, 0);
});

test('Gemini workspace validation rejects outside paths before native IO', async t => {
  const runtime = await fixture(t);
  const outside = path.join(root, 'target', 'outside-gemini.txt');
  await assert.rejects(runtime.call('write_file', { file_path: outside, content: 'outside' }), /workspace|allowed|outside/i);
  await assert.rejects(runtime.call('read_file', { file_path: outside }), /workspace|allowed|outside/i);
  assert.equal(runtime.modelCalls, 0);
});

test('matching, error results, no-ops and CRLF/BOM rendering match original SDK tools', async t => {
  const runtime = await fixture(t), original = await fixture(t, { cwd: runtime.cwd, native: false });
  const file = path.join(runtime.cwd, 'a.txt');
  for (const [source, oldText, newText, extra] of [
    ['\ufeffold\r\n尾巴', 'old', 'new', {}],
    ['one\r\nold\nend\r\n', 'old', 'new', {}],
    ['old old', 'old', 'new', {}],
    ['old old', 'old', 'new', { expected_replacements: 2 }],
    ['old', 'missing', 'new', {}],
    ['old', 'old', 'old', {}],
    ['abcde', 'bc', 'XY', {}],
  ]) {
    const args = editArgs(runtime, oldText, newText, extra);
    await fs.writeFile(file, source);
    const expected = await original.call('replace', args);
    const expectedBytes = await fs.readFile(file);
    await fs.writeFile(file, source);
    const actual = await runtime.call('replace', args);
    assert.deepEqual(actual, expected, JSON.stringify(args));
    assert.deepEqual(await fs.readFile(file), expectedBytes);
  }
  assert.equal(runtime.modelCalls, 0);
});

test('native UTF-8 and binary reads retain Node decoding and complete write bytes', async t => {
  const runtime = await fixture(t);
  const file = path.join(runtime.cwd, 'a.txt');
  for (const content of ['\ufeff零\r\n尾巴\0', 'lone=\ud800\n', 'EOF']) {
    await fs.rm(file, { force: true });
    success(await runtime.call('write_file', { file_path: file, content }));
    assert.deepEqual(await fs.readFile(file), Buffer.from(content));
  }
  const raw = Buffer.from([0xef, 0xbb, 0xbf, 0xff, 0, 13, 10]);
  await fs.writeFile(file, raw);
  assert.equal(await runtime.config.getFileSystemService().readTextFile(file), raw.toString('utf8'));
  await assert.rejects(runtime.config.getFileSystemService().readTextFile(path.join(runtime.cwd, 'missing')), { code: 'ENOENT' });
  assert.equal(runtime.modelCalls, 0);
});

test('concurrent real replace invocations retain independent raw snapshots', { timeout: 30000 }, async t => {
  let reads = 0, release;
  const gate = new Promise(resolve => release = resolve);
  const runtime = await fixture(t, { createClient(binary, cwd) {
    const client = new EditorClient(binary, cwd), request = client.request.bind(client);
    client.request = async (action, fields) => {
      const result = await request(action, fields);
      if (action === 'read' && fields.path.endsWith('a.txt') && reads < 2) {
        if (++reads === 2) release();
        await gate;
      }
      return result;
    };
    return client;
  } });
  await fs.writeFile(path.join(runtime.cwd, 'a.txt'), '\ufeffold\r\n');
  const results = await Promise.all([
    runtime.call('replace', editArgs(runtime, 'old', 'first')),
    runtime.call('replace', editArgs(runtime, 'old', 'second')),
  ]);
  assert.equal(reads, 2);
  assert.equal(results.filter(result => !result.error).length, 1);
  assert.equal(results.filter(result => result.error).length, 1);
  const result = await fs.readFile(path.join(runtime.cwd, 'a.txt'), 'utf8');
  assert.ok(['\ufefffirst\r\n', '\ufeffsecond\r\n'].includes(result));
  assert.equal(runtime.modelCalls, 0);
});

test('external change after native read survives the invocation old-byte guard', async t => {
  let changed = false;
  const runtime = await fixture(t, { createClient(binary, cwd) {
    const client = new EditorClient(binary, cwd), request = client.request.bind(client);
    client.request = async (action, fields) => {
      const result = await request(action, fields);
      if (action === 'read' && fields.path.endsWith('a.txt') && !changed) {
        changed = true;
        await fs.writeFile(fields.path, 'external\r\n');
      }
      return result;
    };
    return client;
  } });
  await fs.writeFile(path.join(runtime.cwd, 'a.txt'), '\ufeffold\r\n');
  const result = await runtime.call('replace', editArgs(runtime));
  assert.ok(result.error, JSON.stringify(result));
  assert.match(result.error.message, /precondition|changed|stale|guard/i);
  assert.equal(await fs.readFile(path.join(runtime.cwd, 'a.txt'), 'utf8'), 'external\r\n');
});

test('a new file appearing before plan retains its external contents', async t => {
  let changed = false;
  const runtime = await fixture(t, { createClient(binary, cwd) {
    const client = new EditorClient(binary, cwd), request = client.request.bind(client);
    client.request = async (action, fields) => {
      if (action === 'plan' && !changed) {
        changed = true;
        assert.equal(fields.operations[0].op, 'create');
        await fs.writeFile(fields.operations[0].path, 'external');
      }
      return request(action, fields);
    };
    return client;
  } });
  const file = path.join(runtime.cwd, 'a.txt');
  const result = await runtime.call('write_file', { file_path: file, content: 'requested' });
  assert.ok(result.error, JSON.stringify(result));
  assert.equal(await fs.readFile(file, 'utf8'), 'external');
});

test('existing symlinks update their regular target and ordinary permissions remain', async t => {
  const runtime = await fixture(t), target = path.join(runtime.cwd, 'a.txt');
  const link = path.join(runtime.cwd, 'alias.txt');
  await fs.writeFile(target, 'old\n', { mode: 0o600 });
  await fs.symlink(target, link);
  success(await runtime.call('replace', editArgs(runtime, 'old', 'new', { file_path: link })));
  assert.equal(await fs.readFile(target, 'utf8'), 'new\n');
  assert.equal((await fs.lstat(link)).isSymbolicLink(), true);
  assert.equal((await fs.stat(target)).mode & 0o777, 0o600);
});

test('SDK scope, restoration ownership and supported byte-array fallback are explicit', async t => {
  const runtime = await fixture(t, { createClient(binary, cwd) {
    const client = new EditorClient(binary, cwd), request = client.request.bind(client);
    client.request = async (action, fields) => {
      const result = await request(action, fields);
      if (action === 'capabilities') delete result.content_encodings;
      return result;
    };
    return client;
  } });
  const file = path.join(runtime.cwd, 'a.txt');
  await assert.rejects(runtime.installation.service.writeTextFile(file, 'outside scope'), /native.run/);
  success(await runtime.call('write_file', { file_path: file, content: '\ufeffold\r\n' }));
  success(await runtime.call('replace', editArgs(runtime)));
  assert.equal(await fs.readFile(file, 'utf8'), '\ufeffnew\r\n');
  const owned = runtime.config.getFileSystemService();
  const later = new runtime.sdk.StandardFileSystemService();
  runtime.config.setFileSystemService(later);
  runtime.installation.restore();
  assert.equal(runtime.config.getFileSystemService(), later);
  runtime.config.setFileSystemService(owned);
  const standard = await fixture(t, { native: false });
  const original = standard.config.getFileSystemService();
  const sandbox = { command: 'docker', image: 'host-selected-image' };
  await assert.rejects(prepareFixture({ cwd: standard.cwd, executable, sandbox }), /local SDK configurations/);
  assert.equal(standard.config.getFileSystemService(), original);
});
