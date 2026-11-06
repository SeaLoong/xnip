import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { EditorClient } from '../integrations/native/editor-client.mjs';
import { createClineFixture, patch } from './cline_native_runtime.mjs';
import { loadCline } from '../benches/native/cline_native_setup.mjs';

const root = path.resolve(import.meta.dirname, '..');
const update = (file = 'a', old = 'old', text = 'new') => ({ input: patch(`*** Update File: ${file}`, '@@', '-' + old, '+' + text) });
const value = reply => reply.output;
async function workspace(t, files = { a: 'old\n' }) {
  const cwd = await fs.mkdtemp(path.join(root, 'target/cline-native-contract-'));
  t.after(() => fs.rm(cwd, { recursive: true, force: true }));
  for (const [file, content] of Object.entries(files)) {
    await fs.mkdir(path.dirname(path.join(cwd, file)), { recursive: true });
    await fs.writeFile(path.join(cwd, file), content);
  }
  return cwd;
}
async function fixture(t, options) {
  const runtime = await createClineFixture(options);
  t.after(() => runtime.close());
  return runtime;
}

test('official registry, schema, read and result hooks wrap the native apply_patch executor', async t => {
  const cwd = await workspace(t);
  const before = [], after = [];
  const native = await fixture(t, { cwd, hooks: {
    beforeTool(ctx) { before.push(ctx.tool.name); },
    afterTool(ctx) { after.push(ctx.tool.name); },
  } });
  const original = await fixture(t, { cwd, native: false });
  assert.deepEqual(native.describe(), original.describe());
  assert.deepEqual(native.describe().tools.map(tool => tool.name), ['read_files', 'apply_patch']);
  assert.equal(value(await native.call('read_files', { files: [{ path: path.join(cwd, 'a') }] }))[0].success, true);
  assert.equal(value(await native.call('apply_patch', update())).success, true);
  assert.equal(await fs.readFile(path.join(cwd, 'a'), 'utf8'), 'new\n');
  assert.deepEqual(before, ['read_files', 'apply_patch']);
  assert.deepEqual(after, before);
  assert.deepEqual(native.events.map(event => event.type), ['tool-started', 'tool-finished', 'tool-started', 'tool-finished']);
  assert.equal(native.agent.snapshot().status, 'idle');
  assert.equal(native.modelCalls, 0);
});

test('tool policies and actual approval callbacks run before backend persistence', async t => {
  const cwd = await workspace(t);
  let children = 0, approvals = 0;
  const createClient = (...args) => { children++; return new EditorClient(...args); };
  const disabled = await fixture(t, { cwd, createClient, toolPolicies: { apply_patch: { enabled: false } } });
  assert.equal((await disabled.call('apply_patch', update())).isError, true);
  const denied = await fixture(t, { cwd, createClient, toolPolicies: { apply_patch: { autoApprove: false } },
    requestToolApproval() { approvals++; return { approved: false, reason: 'Contract rejection' }; } });
  assert.equal((await denied.call('apply_patch', update())).isError, true);
  assert.equal(children, 0);
  assert.equal(approvals, 1);
  const approved = await fixture(t, { cwd, createClient, toolPolicies: { apply_patch: { autoApprove: false } },
    requestToolApproval(request) { approvals++; assert.equal(request.toolName, 'apply_patch'); return { approved: true }; } });
  assert.equal(value(await approved.call('apply_patch', update())).success, true);
  assert.equal(approvals, 2);
  assert.equal(children, 1);
});

test('beforeTool input changes, skipped calls and afterTool result transformations retain SDK semantics', async t => {
  const cwd = await workspace(t);
  let skip = true;
  const runtime = await fixture(t, { cwd, hooks: {
    beforeTool() { return skip ? { skip: true, reason: 'Contract hook' } : { input: update('a', 'old', 'hooked') }; },
    afterTool({ result }) { return { result: { ...result, output: { ...result.output, hook: true } } }; },
  } });
  const blocked = await runtime.call('apply_patch', update());
  assert.equal(blocked.isError, true);
  assert.equal(blocked.output.hook, true);
  assert.equal(await fs.readFile(path.join(cwd, 'a'), 'utf8'), 'old\n');
  skip = false;
  const applied = await runtime.call('apply_patch', update());
  assert.equal(applied.output.hook, true);
  assert.equal(await fs.readFile(path.join(cwd, 'a'), 'utf8'), 'hooked\n');
});

test('official matching and native transactions agree on create, update, delete and move-overwrite', async t => {
  const inputs = [
    { input: patch('*** Add File: nested/new', '+created') },
    update(),
    { input: patch('*** Update File: a', '*** Move to: destination', '@@', '-new', '+moved') },
    { input: patch('*** Delete File: nested/new') },
  ];
  const outputs = [];
  for (const native of [false, true]) {
    const cwd = await workspace(t, { a: 'old\n', destination: 'overwritten\n' });
    const runtime = await fixture(t, { cwd, native });
    const results = [];
    for (const input of inputs) { const result = value(await runtime.call('apply_patch', input)); assert.equal(result.success, true, JSON.stringify(result)); results.push(result); }
    assert.equal(await fs.readFile(path.join(cwd, 'destination'), 'utf8'), 'moved\n');
    await assert.rejects(fs.access(path.join(cwd, 'a')), { code: 'ENOENT' });
    await assert.rejects(fs.access(path.join(cwd, 'nested/new')), { code: 'ENOENT' });
    outputs.push(results);
  }
  assert.deepEqual(outputs[0], outputs[1]);
});

test('BOM, CRLF, Unicode and fuzzy patch results match the original executor', async t => {
  const outputs = [];
  for (const native of [false, true]) {
    const cwd = await workspace(t, { a: '\ufeffheading\r\n  old  \r\n尾巴' });
    const runtime = await fixture(t, { cwd, native });
    const result = value(await runtime.call('apply_patch', update('a', 'old', '新内容')));
    assert.equal(result.success, true, JSON.stringify(result));
    outputs.push({ result, bytes: await fs.readFile(path.join(cwd, 'a')) });
  }
  assert.deepEqual(outputs[0], outputs[1]);
  assert.deepEqual(outputs[1].bytes, Buffer.from('\ufeffheading\r\n新内容\r\n尾巴'));
});

test('parser rejections and a later invalid target preserve every earlier file', async t => {
  const cwd = await workspace(t);
  const runtime = await fixture(t, { cwd });
  for (const input of [
    { input: patch('*** Add File: a', '+overwrite') },
    update('a', 'missing', 'new'),
    { input: patch('*** Update File: a', '@@', '-old', '+new', '*** Update File: missing', '@@', '-absent', '+new') },
  ]) {
    assert.equal(value(await runtime.call('apply_patch', input)).success, false);
    assert.equal(await fs.readFile(path.join(cwd, 'a'), 'utf8'), 'old\n');
  }
  await fs.mkdir(path.join(cwd, 'directory'));
  const failed = await runtime.call('apply_patch', { input: patch('*** Update File: a', '@@', '-old', '+new',
    '*** Add File: directory', '+invalid') });
  assert.equal(value(failed).success, false);
  assert.equal(await fs.readFile(path.join(cwd, 'a'), 'utf8'), 'old\n');
  const wrapped = await runtime.call('apply_patch', {
    input: '*** Update File: ../ignored-header\n' + update().input + '\n*** Delete File: ../ignored-footer',
  });
  assert.equal(value(wrapped).success, true, JSON.stringify(wrapped));
  const nested = path.join(cwd, 'nested');
  await fs.mkdir(nested);
  for (const native of [false, true]) {
    await fs.writeFile(path.join(cwd, 'a'), 'old\n');
    const unrestricted = await fixture(t, { cwd: nested, native,
      executorOptions: { applyPatch: { restrictToCwd: false } } });
    assert.equal(value(await unrestricted.call('apply_patch', update('../a'))).success, true);
    assert.equal(await fs.readFile(path.join(cwd, 'a'), 'utf8'), 'new\n');
  }
});

test('exact raw snapshot guards preserve a concurrent writer between host parsing and core planning', async t => {
  const cwd = await workspace(t);
  const runtime = await fixture(t, { cwd, createClient(...args) {
    const client = new EditorClient(...args), request = client.request.bind(client);
    client.request = async (action, fields) => {
      if (action === 'plan') await fs.writeFile(path.join(cwd, 'a'), 'concurrent\n');
      return request(action, fields);
    };
    return client;
  } });
  const result = value(await runtime.call('apply_patch', update()));
  assert.equal(result.success, false);
  assert.equal(await fs.readFile(path.join(cwd, 'a'), 'utf8'), 'concurrent\n');
  await fs.writeFile(path.join(cwd, 'a'), 'old\r\n');
  const duringParsing = await fixture(t, { cwd, loadHost: async () => {
    const host = await loadCline();
    return { ...host, async computePatchChanges(...args) {
      await fs.writeFile(path.join(cwd, 'a'), 'old\n');
      return host.computePatchChanges(...args);
    } };
  } });
  assert.equal(value(await duringParsing.call('apply_patch', update())).success, false);
  assert.equal(await fs.readFile(path.join(cwd, 'a'), 'utf8'), 'old\n');
});

test('an abort after native planning closes its plan and preserves disk bytes', async t => {
  const cwd = await workspace(t), controller = new AbortController();
  const actions = [];
  const runtime = await fixture(t, { cwd, createClient(...args) {
    const client = new EditorClient(...args), request = client.request.bind(client);
    client.request = async (action, fields) => {
      actions.push(action);
      const reply = await request(action, fields);
      if (action === 'plan') controller.abort(new Error('Contract cancellation'));
      return reply;
    };
    return client;
  } });
  assert.equal(value(await runtime.call('apply_patch', update(), { signal: controller.signal })).success, false);
  assert.equal(await fs.readFile(path.join(cwd, 'a'), 'utf8'), 'old\n');
  assert.ok(actions.includes('abort'));
  assert.ok(!actions.includes('commit'));
});

test('same-path move, normalized aliases and add-existing follow the actual official parser/executor', async t => {
  const outcomes = [];
  for (const native of [false, true]) {
    const cwd = await workspace(t);
    const runtime = await fixture(t, { cwd, native });
    const existing = value(await runtime.call('apply_patch', { input: patch('*** Add File: a', '+overwrite') }));
    const aliases = value(await runtime.call('apply_patch', { input: patch('*** Update File: a', '@@', '-old', '+first',
      '*** Update File: ./a', '@@', '-old', '+second') }));
    assert.equal(await fs.readFile(path.join(cwd, 'a'), 'utf8'), 'second\n');
    const addedAliases = value(await runtime.call('apply_patch', { input: patch('*** Add File: fresh', '+first',
      '*** Add File: ./fresh', '+second') }));
    assert.equal(await fs.readFile(path.join(cwd, 'fresh'), 'utf8'), 'second');
    const deletedAliases = value(await runtime.call('apply_patch', { input: patch('*** Delete File: fresh', '*** Delete File: ./fresh') }));
    await assert.rejects(fs.access(path.join(cwd, 'fresh')), { code: 'ENOENT' });
    await fs.writeFile(path.join(cwd, 'a'), 'old\n');
    await fs.writeFile(path.join(cwd, 'destination'), 'destination\n');
    const movedAliases = value(await runtime.call('apply_patch', { input: patch('*** Update File: a', '*** Move to: destination',
      '@@', '-old', '+moved', '*** Update File: ./destination', '@@', '-destination', '+updated') }));
    assert.equal(await fs.readFile(path.join(cwd, 'destination'), 'utf8'), 'updated\n');
    await fs.writeFile(path.join(cwd, 'a'), 'old\n');
    const moved = value(await runtime.call('apply_patch', { input: patch('*** Update File: a', '*** Move to: ./a', '@@', '-old', '+gone') }));
    await assert.rejects(fs.access(path.join(cwd, 'a')), { code: 'ENOENT' });
    outcomes.push({ existing, aliases, addedAliases, deletedAliases, movedAliases, moved });
  }
  assert.deepEqual(outcomes[0], outcomes[1]);
  assert.equal(outcomes[1].existing.success, false);
  assert.equal(outcomes[1].aliases.success, true);
  assert.equal(outcomes[1].moved.success, true);
});

test('closing the backend restores official executor selection and executes a real edit', async t => {
  const cwd = await workspace(t);
  let client;
  const native = await fixture(t, { cwd, createClient(...args) { client = new EditorClient(...args); return client; } });
  const baseline = native.describe();
  assert.equal(value(await native.call('apply_patch', update())).success, true);
  await native.close();
  assert.equal(client.closed, true);
  assert.equal(client.child.exitCode, 0);
  const restored = await fixture(t, { cwd, native: false });
  assert.deepEqual(restored.describe(), baseline);
  assert.equal(value(await restored.call('apply_patch', update('a', 'new', 'restored'))).success, true);
  assert.equal(await fs.readFile(path.join(cwd, 'a'), 'utf8'), 'restored\n');
  assert.equal(restored.modelCalls, 0);
});
