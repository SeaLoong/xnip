import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { createXnipPlugin } from './index.mjs';

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

export async function createFixture({ hostRoot, cwd, executable, native = true, mode = 'workspace-write', plugin, providerEntries }) {
  const host = await loadRuntime(hostRoot);
  const ctx = new host.cordis.Context();
  try {
    await ctx.plugin(host['dsh-system-prompt'].default);
    await ctx.plugin(host['dsh-tools'].default, { mode: 'native' });
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
    await ctx.plugin(host['dsh-tool-fs'], { readImage: false });
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
        return ctx.tools.execute({ signal, callId: `native-${++nextCall}`, name, arguments: args, agent });
      },
      close: () => ctx.fiber.dispose(),
    };
  } catch (error) { await ctx.fiber.dispose(); throw error; }
}
