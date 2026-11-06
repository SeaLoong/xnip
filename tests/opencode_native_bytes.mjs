import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { after, before, test } from 'node:test';
import { createFixture } from './opencode_native_runtime.mjs';
const root = path.resolve(import.meta.dirname, '..');
const scenarios = JSON.parse(
  execFileSync(
    process.env.XNIP_TEST_PYTHON ?? 'python3',
    [
      '-c',
      "import json,sys;sys.path.insert(0,'benches');from edit_cost import scenarios;print(json.dumps([dict(name=s.name,files=s.files,edits=s.edits,expected=s.expected()) for s in scenarios()]))",
    ],
    { cwd: root, encoding: 'utf8' },
  ),
);
assert.equal(scenarios.length, 13);
let runtime;
before(async () => {
  runtime = await createFixture();
});
after(() => runtime.close());
for (const scenario of scenarios)
  test('actual OpenCode plugin byte fixture: ' + scenario.name, async () => {
    const directory = path.join(runtime.cwd, scenario.name);
    await fs.mkdir(directory, { recursive: true });
    for (const [name, text] of Object.entries(scenario.files)) {
      await fs.mkdir(path.dirname(path.join(directory, name)), { recursive: true });
      await fs.writeFile(path.join(directory, name), text);
    }
    for (const edit of scenario.edits) {
      const result = await runtime.call('xnip_edit', {
        filePath: path.join(directory, edit.file),
        oldString: edit.old,
        newString: edit.text,
        replaceAll: edit.all ?? false,
      });
      assert.equal(result.ok, true, result.error);
    }
    for (const [name, text] of Object.entries(scenario.expected))
      assert.deepEqual(await fs.readFile(path.join(directory, name)), Buffer.from(text));
    assert.equal(runtime.modelCalls, 0);
  });
