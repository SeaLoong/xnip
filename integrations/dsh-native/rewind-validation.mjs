import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { createFixture } from './runtime.mjs';

/** Real official AgentLoop, approval, Session and tool presentation validation.
 * Optional rewind is validated only when requested by the composed profile.
 * No model calls or synthetic filesystem, snapshot or undo implementation.
 */
export async function validateRewind({ hostRoot, cwd, executable, plugin, native = true, providerEntries, rewind = true, rewindPlugin, rewindVersion }) {
  const require = createRequire(path.join(path.resolve(hostRoot), 'package.json'));
  const load = name => import(pathToFileURL(require.resolve(name)).href);
  const fixture = await createFixture({ hostRoot, cwd, executable, plugin, native, providerEntries });
  const { ctx } = fixture;
  let handle;
  try {
    const [Loader, Boot, ConfigEditor, Settings, Commands, Agents, Llm, Loop, Approval] = await Promise.all([
      '@deepseek-ai/cordis-plugin-loader', '@deepseek-ai/dsh-app-boot',
      '@deepseek-ai/dsh-config-editor', '@deepseek-ai/dsh-settings', '@deepseek-ai/dsh-commands',
      '@deepseek-ai/dsh-agent', '@deepseek-ai/dsh-llm', '@deepseek-ai/dsh-agent-loop',
      '@deepseek-ai/dsh-user-approval',
    ].map(load));
    const home = path.join(cwd, 'rewind-home');
    const profile = path.join(home, 'profiles', 'validation');
    Boot.initProfile(profile, []);
    ctx.provide('profileContext', {
      name: 'validation', dir: profile, patchPath: path.join(profile, 'cordis.patch.yml'),
      installAnchor: require.resolve('@deepseek-ai/dsh/package.json'), cwd, home,
      startedBundles: [], overlays: [], telemetryDisabledEnv: undefined,
    });
    if (!ctx.loader) await ctx.plugin(Loader.default, { baseUrl: pathToFileURL(profile + path.sep).href });
    await ctx.plugin(ConfigEditor.default);
    await ctx.plugin(Settings.default);
    await ctx.plugin(Commands.default);
    await ctx.plugin(fixture.host['dsh-session'].default);
    await ctx.plugin(Agents.default);
    await ctx.plugin(Llm.default);
    await ctx.plugin(Loop.default);
    await ctx.plugin(Approval.default, { policy: 'never' });
    let modelCalls = 0;
    ctx.on('llm/stream', () => { modelCalls++; throw new Error('Rewind validation must not call a model'); });
    const snapshots = path.join(cwd, 'rewind-snapshots');
    if (rewind) {
      // A selected plugin must resolve and run. A vanilla profile never needs
      // this third-party dependency merely to validate its native provider.
      await ctx.plugin(rewindPlugin ?? await load('dsh-rewind-plugin'), { snapshotDir: snapshots, dshHome: home });
    }
    handle = await ctx.agents.create({ sessionId: 'xnip-real-rewind', meta: { cwd } });
    const { agent } = handle;
    await agent.whenIdle();
    const source = { kind: 'user' };
    agent.session.append('turn/start', { turn: 1 });
    const anchor = agent.session.append('user/message', Llm.createUserMessage({
      content: [{ type: 'text', text: 'Edit the validation file' }], source,
    }), { surfaceOp: 'append' });
    const target = path.join(cwd, 'rewind-proof.txt');
    const before = Buffer.from('keep\r\nold\r\n尾巴');
    await fs.writeFile(target, before);
    let callId = 0;
    const call = (name, args) => ctx.tools.execute({
      signal: new AbortController().signal, callId: `rewind-${++callId}`, name, arguments: args, agent,
    });
    assert.equal((await call('read', { file_path: target })).isError, false);
    const rejected = await call('edit', {
      file_path: target, old_string: 'old', new_string: 'denied',
      sandbox_permissions: 'danger-full-access', justification: 'Validate denied escalation without mutation',
    });
    assert.equal(rejected.isError, true, JSON.stringify(rejected));
    assert.deepEqual(await fs.readFile(target), before);
    const approvalEvents = agent.session.snapshotEvents().filter(event => event.type.startsWith('approval/'));
    assert.equal(approvalEvents.filter(event => event.type === 'approval/asked').length, 1);
    assert.equal(approvalEvents.filter(event => event.type === 'approval/decided' && event.data.outcome === 'rejected').length, 1);
    const editArgs = { file_path: target, old_string: 'old', new_string: 'new' };
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
    const assertions = ['real AgentLoop idle agent', 'real approval rejection and audit pair', 'official diff presentation'];
    agent.session.append('turn/end', { turn: 1, reason: 'completed' });
    if (rewind) {
      const snapshotFiles = await fs.readdir(snapshots, { recursive: true });
      assert.ok(snapshotFiles.length > 0, 'Real rewind capture must write a snapshot');
      const result = await ctx.commands.execute(agent, `/rewind @${anchor.seq} both`, [], new AbortController().signal);
      assert.ok(result, 'Real rewind command must resolve');
      assert.equal(result.result?.kind, 'success', JSON.stringify(result));
      assert.deepEqual(await fs.readFile(target), before);
      // Rewind must refresh fs/observed: another native edit should work without read.
      const next = await call('edit', { file_path: target, old_string: 'old', new_string: 'after-undo' });
      assert.equal(next.isError, false, JSON.stringify(next));
      assert.equal(await fs.readFile(target, 'utf8'), 'keep\r\nafter-undo\r\n尾巴');
      assertions.push('real tool-event snapshot', 'real /rewind command', 'byte-exact restore', 'observation refresh');
    }
    assert.equal(modelCalls, 0);
    assertions.push('zero model calls');
    return { passed: true, plugin: rewind ? 'dsh-rewind-plugin' : null,
      version: rewind ? rewindVersion ?? require('dsh-rewind-plugin/package.json').version : null,
      rewind: { enabled: rewind, validated: rewind }, assertions };
  } finally {
    if (handle) await handle.dispose();
    await fixture.close();
  }
}
