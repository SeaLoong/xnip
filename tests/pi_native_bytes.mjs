import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createFixture } from './pi_native_runtime.mjs';

const root = path.resolve(import.meta.dirname, '..');
const executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip');
const { stdout } = await promisify(execFile)(process.env.XNIP_TEST_PYTHON ?? 'python3', ['-c', `import json,sys
sys.path.insert(0, 'benches')
from edit_cost import scenarios
print(json.dumps([dict(name=s.name,files=s.files,edits=s.edits,expected=s.expected()) for s in scenarios()]))
`], { cwd: root });
const scenarios = JSON.parse(stdout);
assert.equal(scenarios.length, 13);

for (const scenario of scenarios) {
  test(`Pi native byte fixture: ${scenario.name}`, async t => {
    const cwd = await fs.mkdtemp(path.join(root, 'target/pi-native-bytes-'));
    t.after(() => fs.rm(cwd, { recursive: true, force: true }));
    const runtime = await createFixture({ cwd, executable });
    t.after(() => runtime.close());
    assert.deepEqual(runtime.session.getActiveToolNames().sort(), ['edit', 'read', 'write']);
    for (const [file, content] of Object.entries(scenario.files)) {
      const created = await runtime.call('write', { path: file, content });
      assert.equal(created.isError, false, JSON.stringify(created.result));
      const read = await runtime.call('read', { path: file });
      assert.equal(read.isError, false, JSON.stringify(read.result));
    }
    const groups = new Map();
    for (const operation of scenario.edits) {
      const edits = groups.get(operation.file) ?? [];
      // Pi's public edit schema represents global replacement as one unique
      // full-file block. The original fixtures and their byte oracle stay intact.
      if (operation.all) {
        const oldText = await fs.readFile(path.join(cwd, operation.file), 'utf8');
        edits.push({ oldText, newText: oldText.replaceAll(operation.old, operation.text) });
      } else edits.push({ oldText: operation.old, newText: operation.text });
      groups.set(operation.file, edits);
    }
    for (const [file, edits] of groups) {
      const outcome = await runtime.call('edit', { path: file, edits });
      assert.equal(outcome.isError, false, JSON.stringify(outcome.result));
      assert.ok(outcome.result.details.patch);
    }
    for (const [file, expected] of Object.entries(scenario.expected)) {
      assert.deepEqual(await fs.readFile(path.join(cwd, file)), Buffer.from(expected));
    }
    assert.equal(runtime.session.isIdle, true);
    assert.equal(runtime.session.agent.state.messages.length, 0);
    assert.equal(runtime.modelCalls, 0);
  });
}
