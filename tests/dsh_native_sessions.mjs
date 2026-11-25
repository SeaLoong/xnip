/** Real profile boot, preset activation, and durable Session create/resume. */
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import test from 'node:test';
import { callTool } from '../integrations/dsh-native/runtime.mjs';
import { VERSIONS } from '../benches/native/dsh_native_setup.mjs';

const file = fileURLToPath(import.meta.url);
const root = path.resolve(path.dirname(file), '..');
const execute = promisify(execFile);
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const legacy = path.join(root, 'tests/fixtures/dsh-native-0.6.7');

async function loadHost(host) {
  const require = createRequire(path.join(host, 'package.json'));
  return name => import(pathToFileURL(require.resolve(name)).href);
}

async function sessionProbe(options) {
  // The Web app normally prints a temporary browser credential. Keep the
  // isolated child receipt limited to assertions, without that launch URL.
  console.log = () => {};
  const load = await loadHost(options.host);
  const { runProfile } = await load('@deepseek-ai/dsh/profile-boot');
  const { createLaunchEnvironmentSnapshot } = await load('@deepseek-ai/dsh-launch-environment');
  const { createUserMessage } = await load('@deepseek-ai/dsh-llm');
  const { ctx } = await runProfile({ profile: 'web', patchFiles: [],
    args: ['--no-open', '--port', '0'],
    environment: createLaunchEnvironmentSnapshot([{ source: 'process', values: { ...process.env } }]),
  });
  const result = { mode: options.mode, model_calls: 0, presets: await ctx.agentPresets.list() };
  assert.ok(result.presets.length > 0, 'The official profile must publish its preset roster');
  ctx.on('llm/stream', () => { result.model_calls++; throw new Error('Session regression must not generate model output'); });
  const presetFor = agent => typeof ctx.agentPresets.presetForSession === 'function'
    ? ctx.agentPresets.presetForSession(agent.session)
    : ctx.sessionProjections.stateOf(agent.session, 'agentPreset');
  const errorValue = error => ({ code: error.code, message: error.message });
  const resume = async id => {
    const resolved = await ctx.sessionController.resolveAgent(id);
    if (resolved.error) throw resolved.error;
    assert.ok(resolved.agent, 'The real Session controller must publish the resumed Agent');
    await resolved.agent.whenIdle();
    return resolved.agent;
  };
  const create = async id => {
    const created = await ctx.sessionController.create({ sessionId: id, cwd: options.project });
    const agent = ctx.agents.get(created.sessionId);
    assert.ok(agent, 'The real Session controller must publish the new Agent');
    await agent.whenIdle();
    return agent;
  };
  try {
    if (options.mode === 'seed') {
      const agent = await create(options.existingId ?? 'xnip-existing-session');
      agent.session.append('user/message', createUserMessage({
        content: [{ type: 'text', text: 'Preserve the Session created before compact installation.' }],
        source: { kind: 'user' },
      }), { surfaceOp: 'append' });
      result.events = agent.session.snapshotEvents();
      result.preset = presetFor(agent);
      assert.equal(typeof result.preset, 'string');
      assert.ok(result.preset.trim().length > 0, 'The seeded Session must record an actual preset');
      assert.equal(ctx.agentPresets.composedPreset(agent.ctx), result.preset);
      assert.ok(result.events.length > 0);
    } else if (options.mode === 'legacy') {
      for (const [name, operation] of [['resume', () => resume('xnip-existing-session')],
        ['create', () => create('xnip-legacy-new-session')]]) {
        try { await operation(); result[name] = { passed: true }; }
        catch (error) { result[name] = { passed: false, ...errorValue(error) }; }
      }
    } else {
      assert.ok(result.presets.every(preset => !preset.broken), JSON.stringify(result.presets));
      const restored = await resume(options.existingId ?? 'xnip-existing-session');
      assert.deepEqual(restored.session.snapshotEvents().slice(0, options.events.length), options.events);
      const created = await create(options.newId ?? 'xnip-repaired-new-session');
      let calls = 0;
      for (const [label, agent] of [['restored', restored], ['new', created]]) {
        assert.equal(presetFor(agent), options.preset);
        assert.equal(ctx.agentPresets.composedPreset(agent.ctx), options.preset);
        const modules = ctx.agentPresets.inspectCompositions(agent.ctx).flatMap(composition => composition.modules);
        const activeCompact = [...modules.map(module => module.moduleName),
          ...[...ctx.loader.entries()].filter(entry => entry.fiber && !entry.disabled).map(entry => entry.options.name)]
          .filter(name => name === 'cordis:xnip-compact-tools' || name.endsWith('/compact-tools.mjs'));
        const installedCompact = pathToFileURL(path.join(process.env.DSH_HOME,
          'profiles/web/.xnip-native-package/compact-tools.mjs')).href;
        assert.ok(activeCompact.length > 0, 'The real Loader must activate the installed compact module');
        assert.ok(activeCompact.every(name => name === 'cordis:xnip-compact-tools' || name === installedCompact), JSON.stringify(activeCompact));
        const schemas = ctx.tools.schemas(agent);
        const edit = schemas.find(tool => tool.name === 'edit');
        assert.equal(edit.parameters.properties.new_string.description, 'Replacement; empty deletes.');
        assert.equal(schemas.find(tool => tool.name === 'read').description,
          'Read UTF-8 text with line numbers. Read before editing.');
        const target = path.join(options.project, `${label}-session-proof.txt`);
        await fs.writeFile(target, '\ufeffkeep\r\nold\n尾巴');
        const call = (name, args) => callTool(ctx, name, args,
          { agent, callId: `profile-session-${++calls}` });
        assert.equal((await call('read', { file_path: target })).isError, false);
        const edited = await call('edit', { file_path: target, old_string: 'old', new_string: 'new' });
        assert.equal(edited.isError, false, JSON.stringify(edited));
        assert.equal(edited.content[0].text, 'edited');
        assert.deepEqual(await fs.readFile(target), Buffer.from('\ufeffkeep\r\nnew\n尾巴'));
      }
      result.new_session = created.id;
      result.restored_session = restored.id;
      result.history_preserved = true;
      result.byte_exact_edits = 2;
    }
    await ctx.sessionPersistence.flush();
    assert.equal(result.model_calls, 0);
    return result;
  } finally { await ctx.fiber.dispose(); }
}

async function treeBytes(directory) {
  const files = {};
  const visit = async (current, relative) => {
    for (const entry of await fs.readdir(current, { withFileTypes: true })) {
      const next = path.join(current, entry.name);
      const key = path.posix.join(relative, entry.name);
      if (entry.isDirectory()) await visit(next, key);
      else if (entry.isSymbolicLink()) files[key] = { link: await fs.readlink(next) };
      else files[key] = digest(await fs.readFile(next));
    }
  };
  await visit(directory, '');
  return files;
}

function nestedCompact(entries, selectedPreset) {
  const preset = entries.find(entry => entry.disabled !== true && entry.config?.id === selectedPreset
    && Array.isArray(entry.config.plugins));
  const selected = rows => rows.flatMap(entry => entry.disabled === true ? [] : [entry,
    ...selected(Array.isArray(entry.config) ? entry.config : entry.config?.plugins ?? [])]);
  return selected(preset?.config.plugins ?? []).some(entry => entry.name === './compact-tools.mjs');
}

if (process.argv[2] === '--session-probe') {
  const receipt = await sessionProbe(JSON.parse(process.argv[3]));
  process.stdout.write(JSON.stringify(receipt) + '\n');
} else {
  const selected = process.env.XNIP_DSH_NATIVE_HOST;
  const hosts = selected ? [path.resolve(selected)] : VERSIONS.map(version =>
    path.join(process.env.XNIP_DSH_NATIVE_HOSTS_DIR ?? path.join(root, 'target/native-hosts'), `dsh-${version}`));
  for (const host of hosts) {
    const version = JSON.parse(await fs.readFile(path.join(host, 'node_modules/@deepseek-ai/dsh/package.json'))).version;
    test(`real ${version} profile creates and restores compact Sessions after repairing the 0.6.7 bundle`, { timeout: 300000 }, async t => {
      const provenance = JSON.parse(await fs.readFile(path.join(legacy, 'provenance.json')));
      assert.equal(provenance.commit, '45978217a9482da16f447399adb2840179d98ec0');
      assert.equal(provenance.npm_tarball_sha256, 'a968a1cf55fbcc19573acea6098d69ec728a3e7d8712f0b828d7760454154e96');
      for (const [name, expected] of Object.entries(provenance.files_sha256)) {
        assert.equal(digest(await fs.readFile(path.join(legacy, name))), expected, name);
      }
      const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'xnip native 会话-'));
      t.after(() => fs.rm(directory, { recursive: true, force: true }));
      const home = path.join(directory, 'original');
      const project = path.join(directory, 'project');
      let profile = path.join(home, 'profiles/web');
      await fs.mkdir(project);
      const executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip');
      const dsh = path.join(host, 'node_modules/.bin/dsh');
      const env = { ...process.env, DSH_HOME: home, XNIP_AUTO_UPDATE: '0', DSH_TELEMETRY_DISABLED: '1' };
      const native = async (verb, plugin, extra = []) => JSON.parse((await execute(executable,
        ['native', verb, '--profile', profile, '--plugin', plugin, '--dsh', dsh, ...extra],
        { env, cwd: project, timeout: 120000, maxBuffer: 8 * 1024 * 1024 })).stdout);
      const probe = async (mode, extra = {}) => JSON.parse((await execute(process.execPath,
        [file, '--session-probe', JSON.stringify({ host, project, mode, ...extra })],
        { env, cwd: project, timeout: 120000, maxBuffer: 8 * 1024 * 1024 })).stdout.trim());
      await execute(dsh, ['--profile', 'web', '--dump-config'], { env, cwd: project });
      const seed = await probe('seed');
      const patchBefore = await fs.readFile(path.join(profile, 'cordis.patch.yml'));
      assert.equal((await native('install', path.join(legacy, 'integrations/dsh-native'), ['--compact-tools'])).phase, 'active');
      assert.equal(JSON.parse(await fs.readFile(path.join(profile, '.xnip-native-package/package.json'))).version, '0.6.7');
      const load = await loadHost(host);
      const boot = await load('@deepseek-ai/dsh-app-boot');
      const loaded = boot.loadProfileDirectory('xnip-session-regression', profile,
        path.join(host, 'node_modules/@deepseek-ai/dsh/package.json'));
      const patches = boot.readProfilePatches('xnip-session-regression', {
        name: 'web', dir: profile, patchPath: path.join(profile, 'cordis.patch.yml'),
        installAnchor: path.join(host, 'node_modules/@deepseek-ai/dsh/package.json'),
        cwd: project, home, startedBundles: loaded.layers.map(layer => layer.packageName), overlays: [],
        telemetryDisabledEnv: '1',
      }, loaded);
      const hasNestedRelativeTool = nestedCompact(boot.composeEntries([patches]), seed.preset);
      const broken = await probe('legacy');
      if (hasNestedRelativeTool) {
        for (const action of ['create', 'resume']) {
          assert.equal(broken[action].passed, false, JSON.stringify(broken));
          assert.match(broken[action].message, /tool-fs \(\.\/compact-tools\.mjs\): never started/);
        }
      } else {
        assert.equal(broken.create.passed, true, JSON.stringify(broken));
        assert.equal(broken.resume.passed, true, JSON.stringify(broken));
      }
      const current = path.resolve(process.env.XNIP_TEST_CURRENT_DSH_PLUGIN
        ?? path.join(root, 'integrations/dsh-native'));
      assert.equal((await native('uninstall', current)).phase, 'restored');
      const retained = await treeBytes(path.join(profile, '.xnip-native'));
      const nextState = path.join(profile, '.xnip-native-0.6.8');
      assert.equal((await native('install', current, ['--compact-tools', '--state-dir', nextState])).phase, 'active');
      const repaired = await probe('repaired', { events: seed.events, preset: seed.preset });
      assert.equal(repaired.history_preserved, true);
      assert.equal(repaired.byte_exact_edits, 2);
      assert.equal(repaired.model_calls, 0);
      assert.deepEqual(await treeBytes(path.join(profile, '.xnip-native')), retained);
      assert.deepEqual(await fs.readFile(path.join(profile, 'cordis.patch.yml')), patchBefore);
      await assert.rejects(fs.access(path.join(profile, 'node_modules')), { code: 'ENOENT' });
      const patch = await fs.readFile(path.join(profile, '.xnip-native-package/cordis.patch.yml'), 'utf8');
      assert.ok(!patch.includes(pathToFileURL(profile).href));
      const state = await fs.readFile(path.join(nextState, 'install-manifest.json'), 'utf8');
      assert.ok(!state.includes(profile));
      // A directly dispatched test tool call is outside an LLM turn. Persist a
      // fresh, ordinary conversation to exercise replay across the move.
      const existingId = 'xnip-before-move';
      const beforeMove = await probe('seed', { existingId });
      env.DSH_HOME = path.join(directory, 'moved #会话');
      await fs.rename(home, env.DSH_HOME);
      profile = path.join(env.DSH_HOME, 'profiles/web');
      const movedState = ['--state-dir', path.join(profile, path.basename(nextState))];
      assert.equal((await native('verify', current, movedState)).phase, 'active');
      const moved = await probe('repaired', { existingId, events: beforeMove.events, preset: beforeMove.preset, newId: 'xnip-moved-new-session' });
      assert.equal(moved.byte_exact_edits, 2);
      assert.equal((await native('uninstall', current, movedState)).phase, 'restored');
      assert.equal((await native('verify', current, ['--restored', ...movedState])).phase, 'restored');
      assert.deepEqual(await fs.readFile(path.join(profile, 'cordis.patch.yml')), patchBefore);
    });
  }
}
