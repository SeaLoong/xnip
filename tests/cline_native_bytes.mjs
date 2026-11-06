import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createClineFixture, patch } from './cline_native_runtime.mjs';

const root = path.resolve(import.meta.dirname, '..');
const { stdout } = await promisify(execFile)(process.env.XNIP_TEST_PYTHON ?? 'python3', ['-c', `import json,sys
sys.path.insert(0, 'benches')
from edit_cost import scenarios
print(json.dumps([dict(name=s.name,files=s.files,expected=s.expected()) for s in scenarios()]))
`], { cwd: root });
const scenarios = JSON.parse(stdout);
assert.equal(scenarios.length, 13);

for (const scenario of scenarios) {
  test(`Cline apply_patch native byte fixture: ${scenario.name}`, async t => {
    const cwd = await fs.mkdtemp(path.join(root, 'target/cline-native-bytes-'));
    t.after(() => fs.rm(cwd, { recursive: true, force: true }));
    const runtime = await createClineFixture({ cwd });
    t.after(() => runtime.close());
    assert.deepEqual(runtime.describe().tools.map(tool => tool.name), ['read_files', 'apply_patch']);
    for (const [file, content] of Object.entries(scenario.files)) {
      await fs.mkdir(path.dirname(path.join(cwd, file)), { recursive: true });
      await fs.writeFile(path.join(cwd, file), content);
    }
    assert.ok((await runtime.call('read_files', { files: Object.keys(scenario.files).map(file => ({ path: path.join(cwd, file) })) }))
      .output.every(result => result.success));
    const lines = [];
    for (const [file, expected] of Object.entries(scenario.expected)) {
      // The whole original file supplies literal patch context. Global edits
      // and substring edits share the same independently computed byte oracle.
      const original = scenario.files[file];
      lines.push(`*** Update File: ${file}`, '@@',
        ...original.replace(/\r\n/g, '\n').split('\n').map(line => '-' + line),
        ...expected.replace(/\r\n/g, '\n').split('\n').map(line => '+' + line));
    }
    const result = await runtime.call('apply_patch', { input: patch(...lines) });
    assert.equal(result.output.success, true, JSON.stringify(result));
    for (const [file, expected] of Object.entries(scenario.expected)) {
      assert.deepEqual(await fs.readFile(path.join(cwd, file)), Buffer.from(expected));
    }
    assert.equal(runtime.agent.snapshot().status, 'idle');
    assert.equal(runtime.modelCalls, 0);
  });
}
