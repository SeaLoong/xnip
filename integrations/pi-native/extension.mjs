import { access, mkdir, realpath } from 'node:fs/promises';
import { constants } from 'node:fs';
import { isUtf8 } from 'node:buffer';
import { createRequire } from 'node:module';
import { EditorClient } from '../native/editor-client.mjs';

const require = createRequire(import.meta.url);
const nativeExecutable = () => require('xnip-cli/native').resolveNativeExecutable();
const bytes = text => [...Buffer.from(text)];

/** Pi owns schemas, matching, rendering, hooks and its file mutation queue. */
export function createXnipNativeExtension({
  resolveExecutable = nativeExecutable,
  loadHost = () => import('@earendil-works/pi-coding-agent'),
  createClient = (executable, cwd) => new EditorClient(executable, cwd),
} = {}) {
  return async function xnipNativeExtension(pi) {
    const host = await loadHost();
    const cwd = process.cwd();
    let client;
    let utf8 = false;
    const editor = () => client ??= createClient(resolveExecutable(), cwd);
    const readFile = async absolutePath => {
      const result = await editor().request('read', { path: absolutePath,
        ...(utf8 ? { content_encoding: 'utf8' } : {}) });
      return Buffer.from(result.text ?? result.bytes);
    };

    // Resolve an existing final symlink just as Pi's local writeFile does. The
    // core transaction then guards and updates the resolved regular file.
    const writePath = async absolutePath => {
      try { return await realpath(absolutePath); }
      catch (error) {
        if (error.code === 'ENOENT') return absolutePath;
        throw error;
      }
    };
    async function writeFile(absolutePath, content, signal, original) {
      signal?.throwIfAborted();
      const target = await writePath(absolutePath);
      const plan = await editor().request('plan', {
        ...(utf8 ? { content_encoding: 'utf8' } : {}),
        operations: [{
          op: 'write', path: target, text: utf8 ? content : bytes(content),
          ...(original === undefined ? {} : { old: utf8 && isUtf8(original)
            ? original.toString() : [...original] }),
        }],
      });
      const abort = () => { void editor().request('abort', { plan_id: plan.plan_id }).catch(() => {}); };
      signal?.addEventListener('abort', abort, { once: true });
      try {
        signal?.throwIfAborted();
        await editor().request('commit', {
          plan_id: plan.plan_id, policy: { read_only: false, writable_roots: null },
        });
      } catch (error) {
        await editor().request('abort', { plan_id: plan.plan_id }).catch(() => {});
        throw error;
      } finally {
        signal?.removeEventListener('abort', abort);
      }
    }

    pi.registerTool(host.createReadToolDefinition(cwd, { operations: {
      readFile, access: p => access(p, constants.R_OK),
      detectImageMimeType: host.detectSupportedImageMimeTypeFromFile,
    } }));
    const edit = host.createEditToolDefinition(cwd);
    pi.registerTool({ ...edit, async execute(id, params, signal, onUpdate, ctx) {
      let original;
      const tool = host.createEditToolDefinition(cwd, { operations: {
        access: p => access(p, constants.R_OK | constants.W_OK),
        readFile: async p => original = await readFile(p),
        writeFile: (p, content) => writeFile(p, content, signal, original),
      } });
      return tool.execute(id, params, signal, onUpdate, ctx);
    } });
    const write = host.createWriteToolDefinition(cwd);
    pi.registerTool({ ...write, async execute(id, params, signal, onUpdate, ctx) {
      const tool = host.createWriteToolDefinition(cwd, { operations: {
        mkdir: dir => mkdir(dir, { recursive: true }).then(() => {}),
        writeFile: (p, content) => writeFile(p, content, signal),
      } });
      return tool.execute(id, params, signal, onUpdate, ctx);
    } });
    pi.on('session_start', async () => {
      const capabilities = await editor().request('capabilities');
      utf8 = capabilities.content_encodings?.includes('utf8') === true;
    });
    pi.on('session_shutdown', async () => {
      if (client) await client.close();
      client = undefined;
      utf8 = false;
    });
  };
}
