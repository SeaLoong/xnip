import path from 'node:path';
import { realpath } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { EditorClient } from '../native/editor-client.mjs';

const require = createRequire(import.meta.url);
const nativeExecutable = () => require('xnip-cli/native').resolveNativeExecutable();

/** Public OpenCode tools, permission requests and plugin-owned process lifetime. */
export function createXnipPlugin({
  resolveExecutable = nativeExecutable,
  loadTool = () => import('@opencode-ai/plugin/tool'),
  createClient = (executable, cwd) => new EditorClient(executable, cwd),
} = {}) {
  return async function xnipPlugin({ directory, worktree }) {
    const { tool } = await loadTool();
    const z = tool.schema;
    let connection;
    let client;
    const editor = () =>
      (connection ??= (async () => {
        client = createClient(resolveExecutable(), directory);
        try {
          const capabilities = await client.request('capabilities');
          return { client, utf8: capabilities.content_encodings?.includes('utf8') === true };
        } catch (error) {
          await client.close();
          throw error;
        }
      })());
    const absolute = (name, context) => path.resolve(context.directory ?? directory, name);
    const inside = (root, file) => {
      const name = path.relative(root, file);
      return name !== '..' && !name.startsWith('..' + path.sep) && !path.isAbsolute(name);
    };
    async function canonical(file) {
      try {
        return await realpath(file);
      } catch (error) {
        if (error.code !== 'ENOENT') throw error;
        // A new file still inherits the actual location of its existing parent.
        const parent = path.dirname(file);
        if (parent === file) throw error;
        return path.join(await canonical(parent), path.basename(file));
      }
    }
    async function external(file, context, approved = new Set()) {
      const originalWorktree = context.worktree ?? worktree;
      const [target, actualDirectory, actualWorktree] = await Promise.all([
        canonical(file),
        realpath(context.directory ?? directory),
        realpath(originalWorktree),
      ]);
      // Match OpenCode containsPath: non-git projects use '/' as a sentinel,
      // which grants no additional worktree authority beyond their directory.
      const internal =
        inside(actualDirectory, target) ||
        (originalWorktree !== '/' && inside(actualWorktree, target));
      if (!internal && !approved.has(target)) {
        const pattern = path.join(path.dirname(target), '*').replaceAll('\\', '/');
        await context.ask({
          permission: 'external_directory',
          patterns: [pattern],
          always: [pattern],
          metadata: { filepath: target },
        });
        approved.add(target);
      }
      return { path: target, relative: path.relative(actualWorktree, target) };
    }
    async function mutate(operation, context, files = []) {
      context.abort.throwIfAborted();
      const approved = new Set();
      for (const file of files) await external(absolute(file, context), context, approved);
      const { client: session, utf8 } = await editor();
      const plan = await session.request('plan', {
        operations: [operation(utf8)],
        ...(utf8 ? { content_encoding: 'utf8' } : {}),
      });
      const abort = () => {
        void session.request('abort', { plan_id: plan.plan_id }).catch(() => {});
      };
      context.abort.addEventListener('abort', abort, { once: true });
      try {
        context.abort.throwIfAborted();
        const targets = [];
        for (const change of plan.changes) {
          targets.push(await external(change.path, context, approved));
        }
        await context.ask({
          permission: 'edit',
          patterns: targets.map((target) => target.relative),
          always: ['*'],
          metadata: { diff: plan.diff, files: plan.changes.map((change) => change.path) },
        });
        context.abort.throwIfAborted();
        const result = await session.request('commit', {
          plan_id: plan.plan_id,
          policy: { read_only: false, writable_roots: null },
        });
        return {
          title: 'xnip',
          output: result.written ? 'written' : 'unchanged',
          metadata: {
            written: result.written,
            files: result.changes.map((change) => change.path),
            journal: result.journal,
          },
        };
      } catch (error) {
        await session.request('abort', { plan_id: plan.plan_id }).catch(() => {});
        throw error;
      } finally {
        context.abort.removeEventListener('abort', abort);
      }
    }
    const payload = (value, utf8) => (utf8 ? value : [...Buffer.from(value)]);
    return {
      tool: {
        xnip_edit: tool({
          description:
            'Replace a unique exact block, preserving untouched file bytes. Use replaceAll for all non-overlapping matches.',
          args: {
            filePath: z.string(),
            oldString: z.string().min(1),
            newString: z.string(),
            replaceAll: z.boolean().optional(),
          },
          execute: (args, context) =>
            mutate(
              (utf8) => ({
                op: 'exact_replace',
                path: absolute(args.filePath, context),
                old: payload(args.oldString, utf8),
                text: payload(args.newString, utf8),
                all: args.replaceAll ?? false,
              }),
              context,
              [args.filePath],
            ),
        }),
        xnip_write: tool({
          description:
            'Create or replace a complete UTF-8 text file. Supply oldContent to guard an existing file.',
          args: { filePath: z.string(), content: z.string(), oldContent: z.string().optional() },
          execute: (args, context) =>
            mutate(
              (utf8) => ({
                op: 'write',
                path: absolute(args.filePath, context),
                text: payload(args.content, utf8),
                ...(args.oldContent === undefined ? {} : { old: payload(args.oldContent, utf8) }),
              }),
              context,
              [args.filePath],
            ),
        }),
        xnip_apply: tool({
          description:
            'Apply a unified diff or *** Begin Patch program through a guarded multi-file xnip transaction.',
          args: { patch: z.string().min(1) },
          execute: (args, context) =>
            mutate(
              () => ({ op: 'patch', text: args.patch, root: context.directory ?? directory }),
              context,
            ),
        }),
        xnip_read: tool({
          description:
            'Read a bounded UTF-8 byte range; output reports truncation and the next byte offset.',
          args: {
            filePath: z.string(),
            offset: z.number().int().nonnegative().optional(),
            maxBytes: z.number().int().nonnegative().optional(),
          },
          async execute(args, context) {
            context.abort.throwIfAborted();
            const target = await external(absolute(args.filePath, context), context);
            await context.ask({
              permission: 'read',
              patterns: [target.relative],
              always: ['*'],
              metadata: { filepath: target.path },
            });
            context.abort.throwIfAborted();
            const { client: session, utf8 } = await editor();
            const result = await session.request('read', {
              path: target.path,
              offset: args.offset ?? 0,
              max_bytes: args.maxBytes ?? 12000,
              ...(utf8 ? { content_encoding: 'utf8' } : {}),
            });
            const selected = Buffer.from(result.text ?? result.bytes);
            const next = (args.offset ?? 0) + selected.length;
            return {
              title: target.relative,
              output:
                selected.toString('utf8') +
                (result.truncated
                  ? `\n[read truncated; next_offset=${next}; total_bytes=${result.total_bytes}]`
                  : ''),
              metadata: {
                total_bytes: result.total_bytes,
                file_truncated: result.truncated,
                next_offset: next,
              },
            };
          },
        }),
      },
      async dispose() {
        if (client) await client.close();
      },
    };
  };
}
