import { access, realpath, stat } from 'node:fs/promises';
import { constants } from 'node:fs';
import { isUtf8 } from 'node:buffer';
import { createRequire } from 'node:module';
import { AsyncLocalStorage } from 'node:async_hooks';
import { EditorClient } from '../native/editor-client.mjs';

const require = createRequire(import.meta.url);
const nativeExecutable = () => require('xnip-cli/native').resolveNativeExecutable();

/** Install through Gemini Config's public FileSystemService extension point. */
export async function installXnipFileSystem(config, {
  executable = nativeExecutable(),
  createClient = (binary, cwd) => new EditorClient(binary, cwd),
} = {}) {
  if (config.getSandbox()) {
    throw new Error('Gemini native IO supports local SDK configurations; retain the configured filesystem service for sandbox sessions.');
  }
  const original = config.getFileSystemService();
  const invocations = new AsyncLocalStorage();
  const client = createClient(executable, config.getTargetDir());
  let utf8;
  try {
    const capabilities = await client.request('capabilities');
    utf8 = capabilities.content_encodings?.includes('utf8') === true;
  } catch (error) {
    await client.close();
    throw error;
  }

  const service = {
    async readTextFile(filePath) {
      // Retain the Node error codes consumed by Gemini's tool invocations.
      const snapshots = invocations.getStore();
      try { await access(filePath, constants.R_OK); }
      catch (error) {
        if (error.code === 'ENOENT') snapshots?.set(filePath, null);
        throw error;
      }
      if ((await stat(filePath)).isDirectory()) {
        throw Object.assign(new Error(`EISDIR: illegal operation on a directory, read '${filePath}'`), {
          code: 'EISDIR', path: filePath, syscall: 'read',
        });
      }
      const result = await client.request('read', { path: filePath,
        ...(utf8 ? { content_encoding: 'utf8' } : {}),
      });
      const raw = Buffer.from(result.text ?? result.bytes);
      snapshots?.set(filePath, raw);
      return raw.toString('utf8');
    },
    async writeTextFile(filePath, content) {
      const snapshots = invocations.getStore();
      if (!snapshots) throw new Error('Execute Gemini SDK tools through native.run(callback) to bind their file snapshots.');
      let target;
      try { target = await realpath(filePath); }
      catch (error) {
        if (error.code !== 'ENOENT') throw error;
        target = filePath;
      }
      try { await access(target, constants.W_OK); }
      catch (error) { if (error.code !== 'ENOENT') throw error; }
      const before = snapshots.get(filePath);
      const operation = before === null
        ? { op: 'create', path: target, text: utf8 ? content : [...Buffer.from(content)] }
        : { op: 'write', path: target, text: utf8 ? content : [...Buffer.from(content)],
          ...(before === undefined ? {} : { old: utf8 && isUtf8(before) ? before.toString() : [...before] }),
        };
      const plan = await client.request('plan', {
        ...(utf8 ? { content_encoding: 'utf8' } : {}),
        operations: [operation],
      });
      try {
        await client.request('commit', {
          plan_id: plan.plan_id, policy: { read_only: false, writable_roots: null },
        });
      } catch (error) {
        await client.request('abort', { plan_id: plan.plan_id }).catch(() => {});
        throw error;
      }
    },
  };
  config.setFileSystemService(service);
  function restore() {
    if (config.getFileSystemService() === service) config.setFileSystemService(original);
  }
  return {
    service, restore,
    run: callback => invocations.run(new Map(), callback),
    async close() { restore(); await client.close(); },
  };
}
