import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { validateHostGovernance } from '../integrations/dsh-native/host-governance.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
for (const compact of [false, true]) {
  test(`official DSH governance preserves observations, approval, diff and sandbox with ${compact ? 'compact' : 'ordinary'} tools`, async t => {
    const cwd = await fs.mkdtemp(path.join(root, 'target/native-host-governance-'));
    t.after(() => fs.rm(cwd, { recursive: true, force: true }));
    const result = await validateHostGovernance({
      hostRoot: process.env.XNIP_DSH_NATIVE_HOST ?? path.join(root, 'target/native-hosts/dsh-0.2.1-rc.5'), cwd,
      executable: process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip'), compact,
      providerEntries: [
        { id: 'fs-sandbox', name: '@deepseek-ai/dsh-fs-sandbox', disabled: true },
        { id: 'xnip-native-fs', name: './index.mjs', config: {} },
      ],
    });
    assert.equal(result.passed, true);
    for (const check of ['real AgentLoop idle agent', 'host observation guards', 'real approval rejection and audit pair',
      'official diff presentation', 'host observation refresh', 'host sandbox write denial', 'zero model calls']) {
      assert.ok(result.assertions.includes(check), check);
    }
  });
}
