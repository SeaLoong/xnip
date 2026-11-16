import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createFixture, callTool } from './runtime.mjs';

/** Exercise the official host's observation, sandbox, approval and diff contracts. */
export async function validateHostGovernance(options) {
  const { cwd } = options;
  const fixture = await createFixture(options);
  const { ctx } = fixture;
  let handle;
  try {
    const [Agents, Llm, Loop, Approval] = await Promise.all([
      '@deepseek-ai/dsh-agent', '@deepseek-ai/dsh-llm', '@deepseek-ai/dsh-agent-loop',
      '@deepseek-ai/dsh-user-approval',
    ].map(fixture.host.load));
    await ctx.plugin(fixture.host['dsh-session'].default);
    await ctx.plugin(Agents.default);
    await ctx.plugin(Llm.default);
    await ctx.plugin(Loop.default);
    await ctx.plugin(Approval.default, { policy: 'never' });
    let modelCalls = 0;
    ctx.on('llm/stream', () => { modelCalls++; throw new Error('Host validation executes tools with model generation idle'); });
    handle = await ctx.agents.create({ sessionId: 'xnip-host-governance', meta: { cwd } });
    const { agent } = handle;
    await agent.whenIdle();
    agent.session.append('turn/start', { turn: 1 });
    agent.session.append('user/message', Llm.createUserMessage({
      content: [{ type: 'text', text: 'Edit the validation file' }], source: { kind: 'user' },
    }), { surfaceOp: 'append' });
    const target = path.join(cwd, 'host-governance-proof.txt');
    const before = Buffer.from('keep\r\nold\r\n尾巴');
    await fs.writeFile(target, before);
    let callId = 0;
    const call = (name, args) => callTool(ctx, name, args, { callId: `governance-${++callId}`, agent });
    const editArgs = { file_path: target, old_string: 'old', new_string: 'new' };
    const unobserved = await call('edit', editArgs);
    assert.equal(unobserved.isError, true, JSON.stringify(unobserved));
    assert.match(JSON.stringify(unobserved), /FS_NOT_OBSERVED/);
    assert.deepEqual(await fs.readFile(target), before);
    assert.equal((await call('read', { file_path: target })).isError, false);
    const rejected = await call('edit', { ...editArgs, new_string: 'denied',
      sandbox_permissions: 'danger-full-access', justification: 'Validate denied escalation without mutation',
    });
    assert.equal(rejected.isError, true, JSON.stringify(rejected));
    assert.deepEqual(await fs.readFile(target), before);
    const approvalEvents = agent.session.snapshotEvents().filter(event => event.type.startsWith('approval/'));
    assert.equal(approvalEvents.filter(event => event.type === 'approval/asked').length, 1);
    assert.equal(approvalEvents.filter(event => event.type === 'approval/decided' && event.data.outcome === 'rejected').length, 1);
    const tool = ctx.tools.get('edit', agent);
    assert.equal(tool.presentCall(editArgs).card, 'diff');
    const edit = await call('edit', editArgs);
    assert.equal(edit.isError, false, JSON.stringify(edit));
    const presentation = tool.presentResult(editArgs, edit);
    assert.equal(presentation.card, 'diff');
    assert.equal(presentation.diffs[0].path, target);
    assert.ok(presentation.diffs[0].oldText.includes('old'));
    assert.ok(presentation.diffs[0].newText.includes('new'));
    assert.equal(await fs.readFile(target, 'utf8'), 'keep\r\nnew\r\n尾巴');
    const refreshed = await call('edit', { file_path: target, old_string: 'new', new_string: 'refreshed' });
    assert.equal(refreshed.isError, false, JSON.stringify(refreshed));
    assert.equal(await fs.readFile(target, 'utf8'), 'keep\r\nrefreshed\r\n尾巴');
    await fs.writeFile(target, 'external\r\n');
    const stale = await call('edit', { file_path: target, old_string: 'external', new_string: 'denied' });
    assert.equal(stale.isError, true, JSON.stringify(stale));
    assert.match(JSON.stringify(stale), /FS_STALE_VERSION/);
    assert.equal(await fs.readFile(target, 'utf8'), 'external\r\n');
    const resolved = await ctx.fs.resolve(target);
    await assert.rejects(ctx.fs.writeText(resolved, 'denied', undefined, undefined,
      { mode: 'read-only', workspaceRoot: cwd }), error => error.code === 'FS_SANDBOX_DENIED');
    assert.equal(await fs.readFile(target, 'utf8'), 'external\r\n');
    agent.session.append('turn/end', { turn: 1, reason: 'completed' });
    assert.equal(modelCalls, 0);
    return { passed: true, assertions: ['real AgentLoop idle agent', 'host observation guards',
      'real approval rejection and audit pair', 'official diff presentation', 'host observation refresh',
      'host sandbox write denial', 'zero model calls'] };
  } finally {
    if (handle) await handle.dispose();
    await fixture.close();
  }
}
