import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

/** Versioned editor session, shared by plan/commit and owned by one provider. */
export class EditorClient {
  constructor(executable = 'xnip', cwd = process.cwd()) {
    this.nextId = 0;
    this.pending = new Map();
    this.stderr = '';
    this.child = spawn(executable, ['native', 'serve'], {
      cwd, stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, XNIP_AUTO_UPDATE: '0' },
    });
    this.child.stderr.on('data', bytes => { this.stderr = (this.stderr + bytes).slice(-4096); });
    this.child.stdin.on('error', error => this.fail(error));
    this.lines = createInterface({ input: this.child.stdout });
    this.lines.on('line', line => {
      let reply;
      try { reply = JSON.parse(line); }
      catch { this.fail(new Error('Invalid native bridge JSON response')); this.close(); return; }
      const pending = this.pending.get(reply.id);
      if (!pending) {
        this.fail(new Error(reply.error?.message ?? 'Uncorrelated native bridge response'));
        return;
      }
      this.pending.delete(reply.id);
      if (reply.error) {
        const error = new Error(reply.error.message ?? String(reply.error));
        error.code = reply.error.code;
        pending.reject(error);
      } else pending.resolve(reply.result);
    });
    this.child.on('error', error => this.fail(error));
    this.child.on('exit', (code, signal) => {
      this.closed = true;
      this.fail(new Error(`Editor exited (${code ?? signal}): ${this.stderr}`));
    });
  }
  fail(error) {
    this.failure = error;
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
  }
  request(action, fields = {}) {
    if (this.failure || this.closed) return Promise.reject(this.failure ?? new Error('Editor session closed'));
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: result => {
        if (action === 'commit' && result.journal) {
          console.error('xnip: committed; recovery journal retained at %s', result.journal);
        }
        resolve(result);
      }, reject });
      // Match Node's UTF-8 file IO for strings containing lone UTF-16 units.
      const line = JSON.stringify({ id, protocol: 1, action, ...fields },
        (_key, value) => typeof value === 'string' ? value.toWellFormed() : value) + '\n';
      if (Buffer.byteLength(line) > 32 * 1024 * 1024) {
        this.pending.delete(id);
        reject(new Error('Editor request exceeds 32 MiB protocol limit'));
        return;
      }
      this.child.stdin.write(line, error => {
        if (error) { this.pending.delete(id); reject(error); }
      });
    });
  }
  close() {
    if (this.closePromise) return this.closePromise;
    this.closed = true;
    // EOF is queued after any already-written commit. Let the transaction finish or
    // roll back before exiting; killing it here would strand an in-flight edit.
    this.closePromise = new Promise(resolve => {
      if (this.child.exitCode !== null || this.child.signalCode !== null) resolve();
      else this.child.once('exit', resolve);
    });
    this.child.stdin.end();
    return this.closePromise;
  }
}

/** Canonical source for self-contained local adapter packages. */
export const clientSource = new URL(import.meta.url);
