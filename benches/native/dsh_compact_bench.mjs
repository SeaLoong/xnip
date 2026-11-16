#!/usr/bin/env node
/** Real official tools, idle model, independently checked disk bytes. */
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { createFixture } from '../../integrations/dsh-native/runtime.mjs';
import { VERSIONS } from './dsh_native_setup.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const require = createRequire(path.join(root, 'target/tokenizer/package.json'));
const { getEncoding } = require('js-tiktoken');
const encodings = Object.fromEntries(['cl100k_base', 'o200k_base'].map(name => [name, getEncoding(name)]));
const tokens = value => Object.fromEntries(Object.entries(encodings).map(([name, encoding]) =>
  [name, encoding.encode(typeof value === 'string' ? value : JSON.stringify(value)).length]));
const executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip');
const scenarios = [
  { name: 'tiny-crlf', before: 'keep\r\nold\r\n尾巴', args: { old_string: 'old', new_string: 'new' }, after: 'keep\r\nnew\r\n尾巴' },
  { name: 'bom-mixed-eol', before: '\ufeffkeep\r\nold\nEOF', args: { old_string: 'old', new_string: 'new' }, after: '\ufeffkeep\r\nnew\nEOF' },
  { name: 'replace-all', before: 'old\r\nold\nEOF', args: { old_string: 'old', new_string: 'new', replace_all: true }, after: 'new\r\nnew\nEOF' },
  { name: 'long-context', before: 'untouched\n'.repeat(300) + 'old\nEOF', args: { old_string: 'old', new_string: 'new' }, after: 'untouched\n'.repeat(300) + 'new\nEOF' },
];
const report = { methodology: 'Official schemas + prompt + one read/edit exchange; content fields are model-visible. UI metadata and canonical values are excluded. No model generations. Tokenizers are proxies, not DeepSeek billing counts.',
  tokenizer: 'js-tiktoken@1.0.21', binary_sha256: createHash('sha256').update(await fs.readFile(executable)).digest('hex'), versions: [] };
const cwd = await fs.mkdtemp(path.join(root, 'target/dsh-token-bench-'));
try {
  for (const version of process.argv.slice(2).length ? process.argv.slice(2) : VERSIONS) {
    const measured = { version, modes: {} };
    for (const compact of [false, true]) {
      const fixture = await createFixture({ hostRoot: path.join(root, 'target/native-hosts/dsh-' + version), cwd, executable, compact });
      let modelCalls = 0;
      fixture.ctx.on('llm/stream', () => { modelCalls++; throw new Error('Benchmark executes registered tools directly'); });
      try {
        const description = await fixture.describe();
        const mode = { schema_tokens: tokens(description.tools), prompt_tokens: tokens(description.prompt), scenarios: [] };
        for (const scenario of scenarios) {
          const file = path.join(cwd, scenario.name + '.txt');
          await fs.writeFile(file, scenario.before);
          const readArgs = { file_path: file };
          const editArgs = { file_path: file, ...scenario.args };
          const read = await fixture.call('read', readArgs);
          const edit = await fixture.call('edit', editArgs);
          assert.equal(read.isError, false, JSON.stringify(read));
          assert.equal(edit.isError, false, JSON.stringify(edit));
          assert.deepEqual(await fs.readFile(file), Buffer.from(scenario.after));
          mode.scenarios.push({ name: scenario.name, argument_tokens: tokens([readArgs, editArgs]),
            read_tokens: tokens(read.content), edit_tokens: tokens(edit.content),
            result_tokens: tokens([read.content, edit.content]), byte_exact: true });
        }
        assert.equal(modelCalls, 0);
        measured.modes[compact ? 'compact' : 'native'] = { ...mode, model_calls: modelCalls };
      } finally { await fixture.close(); }
    }
    report.versions.push(measured);
  }
} finally { await fs.rm(cwd, { recursive: true, force: true }); }
const output = path.join(root, 'target/benchmarks/dsh-compact.json');
await fs.mkdir(path.dirname(output), { recursive: true });
await fs.writeFile(output, JSON.stringify(report, null, 2) + '\n');
console.log(output);
for (const { version, modes } of report.versions) console.log(JSON.stringify({ version,
  native_schema: modes.native.schema_tokens, compact_schema: modes.compact.schema_tokens,
  native_edit: modes.native.scenarios[0].edit_tokens, compact_edit: modes.compact.scenarios[0].edit_tokens }));
