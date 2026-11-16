import { EditorClient } from './editor-client.mjs';

async function loadOfficialHost() {
  const [sandbox, fs, policy] = await Promise.all([
    import('@deepseek-ai/dsh-fs-sandbox'),
    import('@deepseek-ai/dsh-fs'),
    import('@deepseek-ai/dsh-sandbox'),
  ]);
  return { SandboxedFileSystem: sandbox.default, FsError: fs.FsError, writableRoots: policy.writableRoots };
}
const bytes = text => [...Buffer.from(text, 'utf8')];
const normalize = text => text.replaceAll('\r\n', '\n');
const diffText = text => normalize(text.replace(/^\ufeff/, ''));
const decode = data => data === null || typeof data === 'string' ? data : new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(Uint8Array.from(data));

// DSH reads normalize CRLF. Map that host view back to exact storage spans;
// editor planning still checks the full original bytes before committing.
function translateEdit(raw, text, edit, FsError, encode) {
  const old = normalize(edit.oldString);
  if (!old.length) throw new FsError('old_string must be non-empty', 'FS_EDIT_NOT_FOUND');
  let view = '';
  const offsets = [];
  for (let i = 0; i < text.length; i++) {
    offsets.push(i);
    if (text[i] === '\r' && text[i + 1] === '\n') { view += '\n'; i++; }
    else view += text[i];
  }
  offsets.push(text.length);
  const matches = [];
  for (let from = 0; from <= view.length;) {
    const at = view.indexOf(old, from);
    if (at < 0) break;
    matches.push([offsets[at], offsets[at + old.length]]);
    from = at + (edit.replaceAll ? old.length : 1);
  }
  if (!matches.length) throw new FsError('old_string was not found', 'FS_EDIT_NOT_FOUND');
  if (!edit.replaceAll && matches.length > 1) throw new FsError('old_string is ambiguous', 'FS_AMBIGUOUS_EDIT');
  const exactOld = text.slice(...matches[0]);
  if (matches.every(([start, end]) => text.slice(start, end) === exactOld)) {
    return { op: 'exact_replace', old: encode(exactOld), text: encode(edit.newString), all: edit.replaceAll };
  }
  let after = text;
  for (const [start, end] of matches.toReversed()) after = after.slice(0, start) + edit.newString + after.slice(end);
  return { op: 'write', old: raw, text: encode(after) };
}

/** Official provider seam; the host retains tools, observations, presentation and approval. */
export function createXnipPlugin({ loadHost = loadOfficialHost, executable } = {}) {
  return {
    name: 'xnip-native-fs',
    inject: ['sandboxPolicy'],
    async apply(ctx, config = {}) {
      const { SandboxedFileSystem, FsError, writableRoots } = await loadHost();
      const proto = SandboxedFileSystem?.prototype;
      if (typeof proto?.checkedTarget !== 'function' || typeof proto.writeText !== 'function'
          || typeof proto.editText !== 'function' || typeof writableRoots !== 'function') {
        throw new Error('Unsupported DSH filesystem contract; retain the native provider or use xnip MCP.');
      }
      class XnipFileSystem extends SandboxedFileSystem {
        constructor() {
          super(ctx, { cwd: config.cwd ?? process.cwd(), diffBasisMaxBytes: config.diffBasisMaxBytes ?? 10 * 1024 * 1024 });
          if (typeof this.withLock !== 'function') throw new Error('DSH provider has no compatible per-target lock');
          this.editor = new EditorClient(executable ?? config.executable ?? process.env.XNIP_EDITOR_EXECUTABLE ?? 'xnip', this.config.cwd);
          ctx.effect(() => () => this.editor.close());
        }
        async mutation(target, expected, signal, sandboxPolicy, makeOperation, write) {
          const policy = sandboxPolicy ?? ctx.sandboxPolicy.resolve();
          const checked = await this.checkedTarget(target, policy);
          return this.withLock(checked.targetKey, async () => {
            if (signal?.aborted) throw new FsError('Operation aborted', 'FS_ABORTED');
            const before = await this.stat(checked, signal);
            if (before && before.type !== 'file') throw new FsError('Target is not a regular file', 'FS_NOT_REGULAR_FILE');
            if (expected?.kind === 'createIfAbsent' && before) throw new FsError('Read the existing file before writing', 'FS_NOT_OBSERVED');
            if ((expected?.version !== undefined && before?.version !== expected.version) || (!write && !before)) {
              throw new FsError('File changed since it was read', 'FS_STALE_VERSION');
            }
            let plan;
            let onAbort;
            try {
              plan = await this.editor.request('plan', {
                operations: [await makeOperation(checked, before)],
                ...(this.utf8 ? { content_encoding: 'utf8' } : {}),
              });
              const change = plan.changes[0];
              let oldText = null;
              try { oldText = decode(change.before); }
              catch (error) { if (!write) throw new FsError('Target is not UTF-8 text', 'FS_NOT_TEXT', { cause: error }); }
              if (write && oldText?.includes('\0')) oldText = null;
              const newText = decode(change.after);
              onAbort = () => { void this.editor.request('abort', { plan_id: plan.plan_id }).catch(() => {}); };
              signal?.addEventListener('abort', onAbort, { once: true });
              if (signal?.aborted) throw new FsError('Operation aborted', 'FS_ABORTED');
              const current = await this.checkedTarget(checked, policy);
              if (current.targetKey !== checked.targetKey || (await this.stat(current, signal))?.version !== before?.version) {
                throw new FsError('File changed during edit planning', 'FS_STALE_VERSION');
              }
              if (signal?.aborted) throw new FsError('Operation aborted', 'FS_ABORTED');
              await this.editor.request('commit', {
                plan_id: plan.plan_id,
                policy: { read_only: policy.mode === 'read-only', writable_roots: policy.mode === 'danger-full-access' ? null : writableRoots(policy) },
              });
              const version = (await this.stat(checked))?.version;
              if (!version) throw new FsError('Written file disappeared before observation', 'FS_IO_ERROR');
              return { ...(write ? { operation: before ? 'update' : 'create' } : {}), version,
                before: write && oldText !== null && (Buffer.byteLength(oldText) >= this.config.diffBasisMaxBytes || Buffer.byteLength(newText) >= this.config.diffBasisMaxBytes) ? null : oldText === null ? null : diffText(oldText),
                after: !write && oldText?.startsWith('\ufeff') ? diffText(newText) : normalize(newText) };
            } catch (error) {
              if (plan) await this.editor.request('abort', { plan_id: plan.plan_id }).catch(() => {});
              if (error instanceof FsError) throw error;
              if (signal?.aborted) throw new FsError('Operation aborted', 'FS_ABORTED', { cause: error });
              const message = error.message ?? String(error);
              const code = /ambiguous|multiple/i.test(message) ? 'FS_AMBIGUOUS_EDIT'
                : /not found|empty|non-empty/i.test(message) ? 'FS_EDIT_NOT_FOUND'
                : /changed|stale|guard|precondition/i.test(message) ? 'FS_STALE_VERSION' : 'FS_IO_ERROR';
              throw new FsError(message, code, { cause: error });
            } finally {
              if (onAbort) signal?.removeEventListener('abort', onAbort);
            }
          });
        }
        writeText(target, content, expected, signal, policy) {
          return this.mutation(target, expected, signal, policy, (checked, before) => ({
            op: before ? 'write' : 'create', path: this.processPath(checked), text: this.utf8 ? content : bytes(content),
          }), true);
        }
        editText(target, edit, expected, signal, policy) {
          return this.mutation(target, expected, signal, policy, async checked => {
            const path = this.processPath(checked);
            const read = await this.editor.request('read', { path, ...(this.utf8 ? { content_encoding: 'utf8' } : {}) });
            const raw = read.text ?? read.bytes;
            let original;
            try { original = decode(raw); }
            catch (error) { throw new FsError('Target is not UTF-8 text', 'FS_NOT_TEXT', { cause: error }); }
            if (original.includes('\0')) throw new FsError('Target is not UTF-8 text', 'FS_NOT_TEXT');
            if (!original.includes('\r\n') && !edit.oldString.includes('\r\n')) {
              return { op: 'exact_replace', path, old: this.utf8 ? edit.oldString : bytes(edit.oldString), text: this.utf8 ? edit.newString : bytes(edit.newString), all: edit.replaceAll };
            }
            return { path, ...translateEdit(raw, original, edit, FsError, text => this.utf8 ? text : bytes(text)) };
          }, false);
        }
      }
      const fs = new XnipFileSystem();
      const capabilities = await fs.editor.request('capabilities');
      fs.utf8 = capabilities?.content_encodings?.includes('utf8') ?? false;
      if (capabilities?.api_version?.split('.')[0] !== '1'
          || !['exact_replace', 'write', 'create'].every(op => capabilities.ops?.includes(op))) {
        throw new Error('Unsupported xnip native protocol or editing capabilities');
      }
    },
  };
}
const plugin = createXnipPlugin();
export const { name, inject, apply } = plugin;
export default plugin;
