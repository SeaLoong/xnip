import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { prepareFixture } from './gemini_native_runtime.mjs';

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
  test(`Gemini native byte fixture: ${scenario.name}`, async t => {
    const cwd = await fs.mkdtemp(path.join(root, 'target/gemini-native-bytes-'));
    t.after(() => fs.rm(cwd, { recursive: true, force: true }));
    const runtime = await prepareFixture({ cwd, executable });
    t.after(() => runtime.close());
    assert.deepEqual(runtime.tools.map(tool => tool.name).sort(), ['read_file', 'replace', 'write_file']);
    for (const [file, content] of Object.entries(scenario.files)) {
      const file_path = path.join(cwd, file);
      const created = await runtime.call('write_file', { file_path, content });
      assert.equal(created.error, undefined, JSON.stringify(created));
      const read = await runtime.call('read_file', { file_path });
      assert.equal(read.error, undefined, JSON.stringify(read));
    }
    for (const operation of scenario.edits) {
      const file_path = path.join(cwd, operation.file);
      const expected_replacements = operation.all
        ? (await fs.readFile(file_path, 'utf8')).split(operation.old).length - 1 : 1;
      const outcome = await runtime.call('replace', {
        file_path, old_string: operation.old, new_string: operation.text,
        instruction: 'Apply the supplied literal replacement.', expected_replacements,
      });
      assert.equal(outcome.error, undefined, JSON.stringify(outcome));
    }
    for (const [file, expected] of Object.entries(scenario.expected)) {
      assert.deepEqual(await fs.readFile(path.join(cwd, file)), Buffer.from(expected));
    }
    assert.equal(runtime.modelCalls, 0);
  });
}
