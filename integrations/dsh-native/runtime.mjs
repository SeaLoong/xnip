import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { createXnipPlugin } from './index.mjs';
import compactToolsPlugin from './compact-tools.mjs';

/** Resolve only official, installed host modules; no editor or policy substitutes. */
export async function loadRuntime(hostRoot) {
  const require = createRequire(path.join(path.resolve(hostRoot), 'package.json'));
  const load = name => import(pathToFileURL(require.resolve(name)).href);
  const names = ['@deepseek-ai/cordis', '@deepseek-ai/dsh-tools', '@deepseek-ai/dsh-system-prompt',
    '@deepseek-ai/dsh-fs-sandbox', '@deepseek-ai/dsh-fs', '@deepseek-ai/dsh-sandbox',
    '@deepseek-ai/dsh-session-projection', '@deepseek-ai/dsh-sandbox-policy',
    '@deepseek-ai/dsh-working-directory', '@deepseek-ai/dsh-fs-observation-policy',
    '@deepseek-ai/dsh-tool-fs', '@deepseek-ai/dsh-session'];
  const modules = await Promise.all(names.map(load));
  return { ...Object.fromEntries(names.map((name, i) => [name.replace('@deepseek-ai/', ''), modules[i]])), load };
}

/** Both official dispatch APIs retain the host's policy and event pipeline. */
export async function callTool(ctx, name, args, { signal = new AbortController().signal, callId, agent } = {}) {
  if (typeof ctx.tools.setExecutor === 'function') {
    const batch = await ctx.tools.execute({ signal, agent,
      calls: [{ type: 'tool-call', id: callId, name, arguments: JSON.stringify(args) }],
      ...(agent ? { log: { turn: 1, step: 1 } } : {}),
    });
    return batch.results[0].result;
  }
  return ctx.tools.execute({ signal, callId, name, arguments: args, agent });
}

export async function createFixture({ hostRoot, cwd, executable, native = true, mode = 'workspace-write', plugin, providerEntries, compact = false, compactPlugin = compactToolsPlugin, toolConfig = {} }) {
  const host = await loadRuntime(hostRoot);
  const ctx = new host.cordis.Context();
  try {
    await ctx.plugin(host['dsh-system-prompt'].default);
    const batch = typeof host['dsh-tools'].default.prototype.setExecutor === 'function';
    await ctx.plugin(host['dsh-tools'].default, batch ? {} : { mode: 'native' });
    if (batch) await ctx.plugin(await host.load('@deepseek-ai/dsh-tool-executor-native'));
    await ctx.plugin(host['dsh-session-projection'].default);
    await ctx.plugin(host['dsh-sandbox-policy'].default, { mode, workspaceRoot: cwd });
    const nativePlugin = (plugin?.createXnipPlugin ?? createXnipPlugin)({ executable, loadHost: async () => ({
        SandboxedFileSystem: host['dsh-fs-sandbox'].default,
        FsError: host['dsh-fs'].FsError,
        writableRoots: host['dsh-sandbox'].writableRoots,
      }) });
    if (providerEntries) {
      const Loader = (await host.load('@deepseek-ai/cordis-plugin-loader')).default;
      await ctx.plugin(Loader);
      ctx.loader.builtins['native-provider'] = nativePlugin;
      ctx.loader.builtins['original-provider'] = host['dsh-fs-sandbox'].default;
      await ctx.loader.root.update(providerEntries.map(entry => ({ ...entry,
        name: entry.id === 'xnip-native-fs' ? 'cordis:native-provider' : 'cordis:original-provider',
        config: { ...entry.config, cwd },
      })));
      await ctx.loader.await();
      if (!ctx.fs || Boolean(ctx.fs.editor) !== native) throw new Error('Official Loader did not activate the composed filesystem provider');
    } else if (native) await ctx.plugin(nativePlugin, { cwd });
    else await ctx.plugin(host['dsh-fs-sandbox'].default, { cwd });
    await ctx.plugin(host['dsh-fs-observation-policy']);
    await ctx.plugin(host['dsh-working-directory'].default, { defaultDirectory: cwd });
    if (compact) ctx.provide('profileContext', { installAnchor: path.join(hostRoot, 'package.json') });
    // Older bundles expected the profile resolver to supply this host peer.
    const adapter = compactPlugin.createCompactToolsPlugin && !compactPlugin.compactBuiltin
      ? compactPlugin.createCompactToolsPlugin(() => host['dsh-tool-fs']) : compactPlugin;
    await ctx.plugin(compact ? adapter : host['dsh-tool-fs'], { ...toolConfig, readImage: false });
    const { Session, SESSION_FORMAT_VERSION } = host['dsh-session'];
    const id = 'xnip-native-validation';
    const session = Session.create(id, [], { version: SESSION_FORMAT_VERSION, id, createdAt: 0, cwd, isSeeded: false });
    const agent = { id, session, options: {}, ctx };
    let nextCall = 0;
    return {
      ctx, agent, host,
      async describe() {
        const assembly = await ctx.systemPrompt.assemble({ agent, scope: agent });
        return { tools: ctx.tools.schemas(agent), prompt: host['dsh-system-prompt'].renderPrompt(assembly) };
      },
      call(name, args, signal = new AbortController().signal) {
        return callTool(ctx, name, args, { signal, callId: `native-${++nextCall}`, agent });
      },
      close: () => ctx.fiber.dispose(),
    };
  } catch (error) { await ctx.fiber.dispose(); throw error; }
}
