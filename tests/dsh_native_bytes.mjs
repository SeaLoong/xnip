import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createFixture } from '../integrations/dsh-native/runtime.mjs';

const root = path.resolve(import.meta.dirname, '..');
const hostRoot = process.env.XNIP_DSH_NATIVE_HOST ?? path.join(root, 'target/native-hosts/dsh-0.2.1-rc.5');
const executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip');
const python = process.env.XNIP_TEST_PYTHON ?? 'python3';
const { stdout } = await promisify(execFile)(python, ['-c', `import json,sys
sys.path.insert(0, 'benches')
from edit_cost import scenarios
print(json.dumps([dict(name=s.name,files=s.files,edits=s.edits,expected=s.expected()) for s in scenarios()]))
`], { cwd: root });
const fixtures = JSON.parse(stdout);
assert.equal(fixtures.length, 13, 'Run every original benchmark byte fixture');

for (const scenario of fixtures) {
  test(`native provider byte fixture: ${scenario.name}`, async t => {
    const cwd = await fs.mkdtemp(path.join(root, 'target/dsh-native-bytes-'));
    t.after(() => fs.rm(cwd, { recursive: true, force: true }));
    const runtime = await createFixture({ hostRoot, cwd, executable });
    t.after(() => runtime.close());
    assert.deepEqual((await runtime.describe()).tools.map(tool => tool.name), ['read', 'write', 'edit']);
    for (const [file, text] of Object.entries(scenario.files)) {
      await fs.mkdir(path.dirname(path.join(cwd, file)), { recursive: true });
      await fs.writeFile(path.join(cwd, file), text);
      const read = await runtime.call('read', { file_path: path.join(cwd, file) });
      assert.equal(read.isError, false, JSON.stringify(read));
    }
    for (const edit of scenario.edits) {
      const result = await runtime.call('edit', {
        file_path: path.join(cwd, edit.file), old_string: edit.old, new_string: edit.text,
        ...(edit.all ? { replace_all: true } : {}),
      });
      assert.equal(result.isError, false, JSON.stringify(result));
    }
    for (const [file, expected] of Object.entries(scenario.expected)) {
      assert.deepEqual(await fs.readFile(path.join(cwd, file)), Buffer.from(expected));
    }
  });
}
