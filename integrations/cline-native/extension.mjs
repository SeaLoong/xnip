import { readFile } from 'node:fs/promises';
import { isUtf8 } from 'node:buffer';
import path from 'node:path';
import { createRequire } from 'node:module';
import { EditorClient } from '../native/editor-client.mjs';

const require = createRequire(import.meta.url);
const nativeExecutable = () => require('xnip-cli/native').resolveNativeExecutable();
const lf = text => text.replace(/\r\n/g, '\n');

function resolvePath(cwd, file, restrictToCwd) {
  const absolute = path.isAbsolute(file) ? path.normalize(file) : path.resolve(cwd, file);
  const relative = path.relative(cwd, absolute);
  if (restrictToCwd && !path.isAbsolute(file) && (relative.startsWith('..') || path.isAbsolute(relative))) {
    throw new Error(`Path must stay within cwd: ${file}`);
  }
  return absolute;
}

/** Official Cline parsing and tool definitions, with core-owned persistence. */
export async function createXnipClineTools({
  cwd = process.cwd(), resolveExecutable = nativeExecutable,
  loadHost = () => import('@cline/sdk'),
  createClient = (executable, directory) => new EditorClient(executable, directory),
  ...options
} = {}) {
  const host = await loadHost();
  const patchOptions = options.executorOptions?.applyPatch ?? {};
  const { restrictToCwd = true } = patchOptions;
  if (patchOptions.encoding && !['utf-8', 'utf8'].includes(patchOptions.encoding)) {
    throw new Error('The xnip Cline backend uses UTF-8 text patches');
  }
  let client;
  let session;
  const editor = async () => {
    session ??= (async () => {
      client = createClient(resolveExecutable(), cwd);
      const capabilities = await client.request('capabilities');
      return { client, utf8: capabilities.content_encodings?.includes('utf8') === true };
    })();
    return session;
  };

  const applyPatch = async (input, directory, context) => {
    context.signal?.throwIfAborted();
    const originals = new Map();
    const inputLines = input.input.split('\n').map(value => value.replace(/\r$/, ''));
    const begin = inputLines.findIndex(line => line.startsWith(host.PATCH_MARKERS.BEGIN));
    const end = inputLines.findLastIndex(line => line.startsWith(host.PATCH_MARKERS.END));
    const sources = begin < 0 && end < 0 ? inputLines
      : begin >= 0 && end >= begin ? inputLines.slice(begin, end + 1) : [];
    for (const line of sources) {
      const marker = [host.PATCH_MARKERS.UPDATE, host.PATCH_MARKERS.DELETE].find(value => line.startsWith(value));
      if (!marker) continue;
      const source = resolvePath(directory, line.slice(marker.length).trim(), restrictToCwd);
      if (originals.has(source)) continue;
      try { originals.set(source, await readFile(source)); }
      catch { originals.set(source, null); } // Official parsing supplies its usual read error.
    }
    const { changes, fuzz } = await host.computePatchChanges(input.input, directory, patchOptions);
    const operations = [];
    const touched = [];
    const planned = new Map();
    const { client: connection, utf8 } = await editor();
    const encode = bytes => utf8 && isUtf8(bytes) ? bytes.toString() : [...bytes];
    const text = value => utf8 ? value : [...Buffer.from(value)];
    for (const [file, change] of Object.entries(changes)) {
      const source = resolvePath(directory, file, restrictToCwd);
      if (change.type === 'add') {
        const previous = planned.get(source);
        operations.push(previous == null
          ? { op: 'create', path: source, text: text(change.newContent) }
          : { op: 'write', path: source, text: text(change.newContent), old: encode(previous) });
        planned.set(source, Buffer.from(change.newContent));
        touched.push(file);
        continue;
      }
      // Cline's parser restores each file's chosen EOL in oldContent. The raw
      // snapshot supplies core's exact guard, including mixed-EOL bytes.
      const original = originals.get(source);
      const current = await readFile(source);
      if (original === null || !original.equals(current) || lf(original.toString()) !== lf(change.oldContent)) {
        throw new Error(`File changed while preparing patch: ${file}`);
      }
      // Cline computes every hunk against the original disk state, then applies
      // file entries in order. Alias paths therefore use the preceding planned
      // bytes as their core guard while retaining the original snapshot check.
      const previousSource = planned.has(source) ? planned.get(source) : original;
      if (change.type === 'delete') {
        if (previousSource !== null) operations.push({ op: 'delete', path: source, old: encode(previousSource) });
        planned.set(source, null);
        touched.push(`${file}: [deleted]`);
      } else if (change.movePath) {
        const destination = resolvePath(directory, change.movePath, restrictToCwd);
        if (destination === source) {
          if (previousSource !== null) operations.push({ op: 'delete', path: source, old: encode(previousSource) });
          planned.set(source, null);
          touched.push(`${file} -> ${change.movePath}`);
          continue;
        }
        let previous = planned.get(destination);
        if (!planned.has(destination)) {
          try { previous = await readFile(destination); }
          catch (error) { if (error.code !== 'ENOENT') throw error; previous = null; }
        }
        operations.push(previous === null
          ? { op: 'create', path: destination, text: text(change.newContent) }
          : { op: 'write', path: destination, text: text(change.newContent), old: encode(previous) });
        planned.set(destination, Buffer.from(change.newContent));
        if (previousSource !== null) operations.push({ op: 'delete', path: source, old: encode(previousSource) });
        planned.set(source, null);
        touched.push(`${file} -> ${change.movePath}`);
      } else {
        operations.push(previousSource === null
          ? { op: 'create', path: source, text: text(change.newContent) }
          : { op: 'write', path: source, text: text(change.newContent), old: encode(previousSource) });
        planned.set(source, Buffer.from(change.newContent));
        touched.push(file);
      }
    }
    context.signal?.throwIfAborted();
    const plan = await connection.request('plan', {
      ...(utf8 ? { content_encoding: 'utf8' } : {}), operations,
    });
    const abort = () => { void connection.request('abort', { plan_id: plan.plan_id }).catch(() => {}); };
    context.signal?.addEventListener('abort', abort, { once: true });
    try {
      context.signal?.throwIfAborted();
      await connection.request('commit', {
        plan_id: plan.plan_id, policy: { read_only: false, writable_roots: null },
      });
    } catch (error) {
      await connection.request('abort', { plan_id: plan.plan_id }).catch(() => {});
      throw error;
    } finally {
      context.signal?.removeEventListener('abort', abort);
    }
    const lines = ['Successfully applied patch to the following files:', ...touched];
    if (fuzz > 0) lines.push(`Note: Patch applied with fuzz factor ${fuzz}`);
    return lines.join('\n');
  };
  const tools = host.createBuiltinTools({ ...options, cwd, enableEditor: false, enableApplyPatch: true,
    executors: { ...options.executors, applyPatch } });
  return {
    tools,
    async close() { if (client) await client.close(); },
  };
}
