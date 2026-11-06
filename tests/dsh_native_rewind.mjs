import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { validateRewind } from '../integrations/dsh-native/rewind-validation.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
test('real DSH rewind restores provider edits through official Loader, AgentLoop and commands', async t => {
  const cwd = await fs.mkdtemp(path.join(root, 'target/native-rewind-'));
  t.after(() => fs.rm(cwd, { recursive: true, force: true }));
  await validateRewind({
    hostRoot: process.env.XNIP_DSH_NATIVE_HOST ?? path.join(root, 'target/native-hosts/dsh-0.2.1-rc.5'), cwd,
    executable: process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip'),
    providerEntries: [
      { id: 'fs-sandbox', name: '@deepseek-ai/dsh-fs-sandbox', disabled: true },
      { id: 'xnip-native-fs', name: './index.mjs', config: {} },
    ],
  });
});

test('vanilla DSH validates real native approval and diff without enabling optional rewind', async t => {
  const cwd = await fs.mkdtemp(path.join(root, 'target/native-governance-'));
  t.after(() => fs.rm(cwd, { recursive: true, force: true }));
  const result = await validateRewind({
    hostRoot: process.env.XNIP_DSH_NATIVE_HOST ?? path.join(root, 'target/native-hosts/dsh-0.2.1-rc.5'), cwd,
    executable: process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip'), rewind: false,
    providerEntries: [
      { id: 'fs-sandbox', name: '@deepseek-ai/dsh-fs-sandbox', disabled: true },
      { id: 'xnip-native-fs', name: './index.mjs', config: {} },
    ],
  });
  assert.equal(result.passed, true);
  assert.equal(result.plugin, null);
  assert.deepEqual(result.rewind, { enabled: false, validated: false });
  assert.deepEqual(result.assertions, ['real AgentLoop idle agent', 'real approval rejection and audit pair',
    'official diff presentation', 'zero model calls']);
  assert.equal(await fs.access(path.join(cwd, 'rewind-snapshots')).then(() => true, () => false), false);
});
