#!/usr/bin/env node
/** Actual Pi SDK tools on built-in and xnip-backed IO; zero model calls. */
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { scenarios, statistics, root } from './native_provider.mjs';

const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const fileInfo = value => ({ bytes: Buffer.byteLength(value), sha256: hash(value) });
const measures = ['read_ms', 'edit_ms', 'tool_ms', 'tool_calls', 'argument_bytes', 'result_bytes', 'tool_bytes'];

export async function piScenarios() {
  const fixtures = (await scenarios()).filter(item => item.cohort === 'original13' || /^large-\d+$/.test(item.name)).map(item => ({ ...item,
    purpose: item.name === 'same-file-batch' ? 'two disjoint blocks in one Pi edits[] call'
      : item.name === 'multi-file-batch' ? 'three files through three Pi edit calls' : item.purpose }));
  const one = (name, before, old, text, expected, extra = {}) => ({ name, cohort: 'pi-extension',
    files: { 'example.txt': before }, edits: [{ file: 'example.txt', old, text }],
    expected: { 'example.txt': expected }, ...extra });
  return [...fixtures,
    one('pi-fuzzy-quote', 'keep  \nconst title = “old”;\nend  \n', 'const title = "old";', 'const title = "new";',
      'keep  \nconst title = "new";\nend  \n', { purpose: 'official fuzzy quote matching preserves untouched line bytes' }),
    one('pi-reject-missing', 'old\n', 'missing', 'new', 'old\n', { error: 'Could not find the exact text', purpose: 'missing search preserves storage' }),
    one('pi-reject-ambiguous', 'same same\n', 'same', 'new', 'same same\n', { error: 'Found 2 occurrences', purpose: 'ambiguous search preserves storage' }),
    one('pi-noop', 'same\n', 'same', 'same', 'same\n', { error: 'No changes made', purpose: 'identical edit follows the official host rejection' }),
  ];
}

async function sourceProvenance() {
  const names = ['Cargo.toml', 'Cargo.lock', 'benches/pi_provider.mjs', 'benches/native_provider.mjs',
    'benches/edit_cost.py', 'benches/native/pi_adapter.mjs', 'benches/native/pi_package.json',
    'benches/native/pi_package-lock.json', 'tests/pi_native_runtime.mjs'];
  async function visit(directory) {
    for (const item of await fs.readdir(path.join(root, directory), { withFileTypes: true })) {
      const name = path.posix.join(directory, item.name);
      if (item.isDirectory()) await visit(name);
      else if (/\.(rs|mjs|json)$/.test(item.name)) names.push(name);
    }
  }
  for (const directory of ['src', 'integrations/native', 'integrations/pi-native']) await visit(directory);
  const files = Object.fromEntries(await Promise.all(names.sort().map(async name => [name, hash(await fs.readFile(path.join(root, name)))])));
  return { sha256: hash(JSON.stringify(files)), files_sha256: files };
}

function catalog(runtime, host, cwd) {
  assert.deepEqual(runtime.session.getActiveToolNames().sort(), ['edit', 'read', 'write']);
  return ['read', 'edit', 'write'].map(name => {
    const actual = runtime.session.getToolDefinition(name);
    const expected = host[`create${name[0].toUpperCase()}${name.slice(1)}ToolDefinition`](cwd);
    const fields = ['name', 'label', 'description', 'parameters', 'outputSchema', 'constrainedSampling',
      'promptSnippet', 'promptGuidelines', 'renderShell'];
    for (const field of fields) assert.deepEqual(actual[field], expected[field], `${name}.${field}`);
    for (const field of ['prepareArguments', 'renderCall', 'renderResult']) assert.equal(actual[field], expected[field], `${name}.${field}`);
    return Object.fromEntries(fields.filter(field => actual[field] !== undefined).map(field => [field, actual[field]]));
  });
}

async function storedFiles(cwd, relative = '') {
  const files = [];
  for (const entry of await fs.readdir(path.join(cwd, relative), { withFileTypes: true })) {
    const name = path.posix.join(relative, entry.name);
    if (entry.isDirectory()) files.push(...await storedFiles(cwd, name));
    else files.push(name);
  }
  return files.sort();
}

async function sample(runtime, fixture, directory) {
  await fs.mkdir(directory, { recursive: true });
  for (const [name, value] of Object.entries(fixture.files)) {
    await fs.mkdir(path.dirname(path.join(directory, name)), { recursive: true });
    await fs.writeFile(path.join(directory, name), value);
  }
  const calls = [];
  async function call(phase, name, args) {
    const started = performance.now();
    const outcome = await runtime.call(name, args);
    const elapsed_ms = performance.now() - started;
    const result = outcome.result;
    const argumentsJson = JSON.stringify(args), resultJson = JSON.stringify(result);
    calls.push({ phase, name, elapsed_ms, argument_bytes: Buffer.byteLength(argumentsJson), result_bytes: Buffer.byteLength(resultJson), is_error: outcome.isError,
      argument_sha256: hash(argumentsJson), result_sha256: hash(resultJson),
      ...(outcome.isError ? { error: resultJson.replaceAll(directory, '<fixture>') } : {}) });
    return outcome;
  }
  for (const name of Object.keys(fixture.files)) assert.equal((await call('read', 'read', { path: path.join(directory, name) })).isError, false);
  const groups = new Map();
  for (const edit of fixture.edits) {
    const edits = groups.get(edit.file) ?? [];
    const original = fixture.files[edit.file];
    edits.push(edit.all ? { oldText: original, newText: original.replaceAll(edit.old, edit.text) }
      : { oldText: edit.old, newText: edit.text });
    groups.set(edit.file, edits);
  }
  for (const [name, edits] of groups) await call('edit', 'edit', { path: path.join(directory, name), edits });
  const outcomes = {};
  for (const [name, expected] of Object.entries(fixture.expected)) {
    const actual = await fs.readFile(path.join(directory, name));
    outcomes[name] = { expected: fileInfo(expected), actual: fileInfo(actual), byte_exact: actual.equals(Buffer.from(expected)) };
  }
  const unexpected_files = (await storedFiles(directory)).filter(name => !Object.hasOwn(fixture.expected, name));
  const edits = calls.filter(call => call.phase === 'edit');
  const outcome_matches = edits.every(call => fixture.error ? call.is_error && call.error.includes(fixture.error) : !call.is_error);
  const byte_exact = Object.values(outcomes).every(outcome => outcome.byte_exact) && unexpected_files.length === 0;
  const sum = field => calls.reduce((total, call) => total + call[field], 0);
  assert.equal(runtime.modelCalls, 0);
  assert.equal(runtime.session.agent.state.messages.length, 0);
  assert.equal(runtime.session.isIdle, true);
  return { read_ms: calls.filter(call => call.phase === 'read').reduce((total, call) => total + call.elapsed_ms, 0),
    edit_ms: edits.reduce((total, call) => total + call.elapsed_ms, 0), tool_ms: sum('elapsed_ms'),
    tool_calls: calls.length, argument_bytes: sum('argument_bytes'), result_bytes: sum('result_bytes'),
    tool_bytes: sum('argument_bytes') + sum('result_bytes'), calls, outcomes, unexpected_files,
    byte_exact, outcome_matches, correct: byte_exact && outcome_matches, model_calls: runtime.modelCalls };
}

export function validatePiReport(report) {
  assert.equal(report.results.length, report.scenarios.length * 2);
  assert.equal(new Set(report.results.map(row => `${row.provider}/${row.scenario}`)).size, report.results.length);
  for (const row of report.results) {
    const scenario = report.scenarios.find(item => item.name === row.scenario);
    assert.ok(scenario);
    assert.equal(row.samples.length, report.methodology.measured_runs_per_scenario);
    for (const sample of row.samples) {
      assert.equal(sample.model_calls, 0);
      assert.equal(sample.tool_calls, sample.calls.length);
      assert.equal(sample.calls.filter(call => call.phase === 'read').length, scenario.read_calls);
      assert.equal(sample.calls.filter(call => call.phase === 'edit').length, scenario.edit_calls);
      assert.ok(sample.calls.every(call => Number.isFinite(call.elapsed_ms) && call.elapsed_ms >= 0));
      assert.equal(sample.argument_bytes, sample.calls.reduce((total, call) => total + call.argument_bytes, 0));
      assert.equal(sample.result_bytes, sample.calls.reduce((total, call) => total + call.result_bytes, 0));
      assert.equal(sample.tool_bytes, sample.argument_bytes + sample.result_bytes);
      assert.ok(Math.abs(sample.tool_ms - sample.read_ms - sample.edit_ms) < 0.001);
      assert.equal(sample.byte_exact, Object.values(sample.outcomes).every(outcome => outcome.byte_exact) && sample.unexpected_files.length === 0);
      assert.deepEqual(Object.keys(sample.outcomes).sort(), Object.keys(scenario.expected).sort(), 'complete expected file oracle');
      for (const [name, outcome] of Object.entries(sample.outcomes)) {
        assert.deepEqual(outcome.expected, scenario.expected[name]);
        assert.equal(outcome.byte_exact, outcome.expected.bytes === outcome.actual.bytes && outcome.expected.sha256 === outcome.actual.sha256);
      }
      assert.equal(sample.outcome_matches, sample.calls.filter(call => call.phase === 'edit').every(call => scenario.expected_error
        ? call.is_error && call.error.includes(scenario.expected_error) : !call.is_error));
      assert.equal(sample.correct, sample.byte_exact && sample.outcome_matches);
    }
    for (const field of measures) assert.deepEqual(row.summary[field], statistics(row.samples.map(sample => sample[field])));
    for (const [field, sampleField] of [['byte_exact_runs', 'byte_exact'], ['outcome_matches_runs', 'outcome_matches'], ['correct_runs', 'correct']])
      assert.equal(row[field], row.samples.filter(sample => sample[sampleField]).length, `${field} derived from samples`);
    assert.equal(row.correct_runs, row.samples.length, `${row.provider}/${row.scenario}: byte and host outcome oracle`);
  }
  for (const row of report.results.filter(row => row.provider === 'xnip-native')) {
    const original = report.results.find(item => item.provider === 'pi-original' && item.scenario === row.scenario);
    for (let index = 0; index < row.samples.length; index++) assert.deepEqual(
      row.samples[index].calls.map(call => [call.name, call.argument_sha256, call.result_sha256, call.is_error]),
      original.samples[index].calls.map(call => [call.name, call.argument_sha256, call.result_sha256, call.is_error]),
      `${row.scenario}: same host tool arguments and results`);
  }
}

export async function runPiBenchmark({ executable = path.join(root, 'target/release/xnip'), repeats = 3,
  warmups = 1, scenarioNames, buildProfile = 'release', onProgress = () => {} } = {}) {
  assert.ok(Number.isInteger(repeats) && repeats > 0);
  assert.ok(Number.isInteger(warmups) && warmups >= 0);
  executable = path.resolve(executable);
  const fixtures = await piScenarios();
  const selected = scenarioNames ? fixtures.filter(item => scenarioNames.includes(item.name)) : fixtures;
  assert.ok(selected.length);
  if (scenarioNames) assert.equal(selected.length, new Set(scenarioNames).size);
  const binary = await fs.readFile(executable);
  const source = await sourceProvenance();
  const started = performance.now();
  const { createFixture, host, VERSION, COMMIT } = await import('../tests/pi_native_runtime.mjs');
  const bootstrap_ms = performance.now() - started;
  const packagePath = path.join(root, 'target/native-bench/pi-v1.3.0/node_modules/@earendil-works/pi-coding-agent');
  const manifest = await fs.readFile(path.join(packagePath, 'package.json'));
  assert.equal(JSON.parse(manifest).version, VERSION);
  const report = { schema_version: 1, created_at: new Date().toISOString(),
    methodology: { measured_runs_per_scenario: repeats, warmup_runs_per_scenario: warmups, model_calls: 0,
      host: 'official Pi SDK loader, registry, runToolCall and before/after tool hooks; idle session with model stream counter',
      governance: 'Pi local tools and mutation queue; preparatory reads are included in this workflow',
      batching: 'edits are grouped by file in the public edits[] schema; global replace uses a unique full-file oldText/newText',
      timed_region: 'sum of performance.now tool execution durations; file creation, oracle and byte counting are outside',
      setup: 'SDK package/bootstrap timed once; provider session creation includes native capability handshake; one persistent session per lane',
      bytes: 'UTF-8 JSON.stringify public tool arguments and actual result object; SDK control envelope and native IPC have separate boundaries',
      p95: 'nearest rank; three samples give the maximum', ordering: 'original provider then native; fixture order; per-fixture warm-up',
      scope: 'local known-input tools and storage bytes; model generation and billing are outside this cohort' },
    provenance: { node: process.version, platform: os.platform(), arch: os.arch(), kernel: os.release(),
      cpu_model: os.cpus()[0]?.model, binary: { filename: path.basename(executable), build_profile: buildProfile,
        bytes: binary.length, sha256: hash(binary) }, source, host: { name: '@earendil-works/pi-coding-agent', version: VERSION,
        official_source_commit: COMMIT, package_sha256: hash(manifest),
        edit_entrypoint_sha256: hash(await fs.readFile(path.join(packagePath, 'dist/core/tools/edit.js'))),
        matching_entrypoint_sha256: hash(await fs.readFile(path.join(packagePath, 'dist/core/tools/edit-diff.js'))) } },
    bootstrap_ms, scenarios: selected.map(item => ({ name: item.name, cohort: item.cohort, purpose: item.purpose,
      expected_error: item.error ?? null, read_calls: Object.keys(item.files).length, edit_calls: new Set(item.edits.map(edit => edit.file)).size,
      files: Object.fromEntries(Object.entries(item.files).map(([name, value]) => [name, fileInfo(value)])),
      expected: Object.fromEntries(Object.entries(item.expected).map(([name, value]) => [name, fileInfo(value)])) })),
    setup: [], results: [] };
  const cwd = await fs.mkdtemp(path.join(root, 'target/pi-provider-bench-'));
  let expectedCatalog;
  try {
    for (const native of [false, true]) {
      const provider = native ? 'xnip-native' : 'pi-original';
      const started = performance.now();
      const runtime = await createFixture({ cwd, executable, native });
      const setup_ms = performance.now() - started;
      try {
        const description = catalog(runtime, host, cwd);
        if (expectedCatalog) assert.deepEqual(description, expectedCatalog);
        else expectedCatalog = description;
        report.setup.push({ provider, setup_ms, tool_schema_sha256: hash(JSON.stringify(description)) });
        for (const fixture of selected) {
          const samples = [];
          for (let index = 0; index < warmups + repeats; index++) {
            const directory = path.join(cwd, 'cases', fixture.name, String(index));
            await fs.rm(directory, { recursive: true, force: true });
            const result = await sample(runtime, fixture, directory);
            if (index >= warmups) samples.push(result);
          }
          const summary = Object.fromEntries(measures.map(field => [field, statistics(samples.map(sample => sample[field]))]));
          report.results.push({ provider, scenario: fixture.name, cohort: fixture.cohort, summary,
            byte_exact_runs: samples.filter(sample => sample.byte_exact).length,
            outcome_matches_runs: samples.filter(sample => sample.outcome_matches).length,
            correct_runs: samples.filter(sample => sample.correct).length, samples });
          onProgress(`${provider} ${fixture.name}: ${summary.edit_ms.median.toFixed(2)} ms edit`);
        }
        assert.deepEqual(runtime.errors, []);
      } finally { await runtime.close(); }
    }
  } finally { await fs.rm(cwd, { recursive: true, force: true }); }
  assert.ok(binary.equals(await fs.readFile(executable)), 'measured binary stayed fixed');
  assert.equal((await sourceProvenance()).sha256, source.sha256, 'measured source stayed fixed');
  validatePiReport(report);
  return report;
}

export function renderPiReport(report) {
  const runs = report.methodology.measured_runs_per_scenario;
  const lines = ['# Pi native provider benchmark', '',
    `Pi ${report.provenance.host.version}; ${report.scenarios.length} fixtures; ${runs} measured runs and ${report.methodology.warmup_runs_per_scenario} warm-up per fixture/provider. Measured ${report.created_at}. Zero model calls, checked with the real runtime counter and idle session.`, '',
    'Both lanes execute the official SDK read/edit tools and mutation queue through runToolCall and its before/after hooks. The native lane retains the host schemas, argument preparation, matching and renderers; xnip supplies IO. The catalog and metadata are checked equal. This Pi cohort retains its own governance and public result boundary.', '',
    'Each file is read before editing. Edits in one file are grouped in edits[] using original-file coordinates. Global replacement uses a unique full-file block. Fixture creation, byte verification and serialization counting are outside tool timing. One persistent session serves each lane. Setup includes session creation and native handshake; SDK bootstrap is separate. Medians and nearest-rank p95 use the raw samples; with three runs p95 is the maximum.', '',
    `Binary ${report.provenance.binary.build_profile}, SHA-256 \`${report.provenance.binary.sha256}\`; source \`${report.provenance.source.sha256}\`. Node ${report.provenance.node}; ${report.provenance.platform}/${report.provenance.arch}; ${report.provenance.cpu_model}. SDK bootstrap ${report.bootstrap_ms.toFixed(2)} ms.`, '',
    '| Provider | Session setup ms |', '|---|---:|'];
  for (const item of report.setup) lines.push(`| ${item.provider} | ${item.setup_ms.toFixed(2)} |`);
  lines.push('', '| Fixture | Original edit median / p95 ms | xnip edit median / p95 ms | Original / xnip read median ms | Original / xnip tool bytes | Original / xnip correct |',
    '|---|---:|---:|---:|---:|---:|');
  for (const fixture of report.scenarios) {
    const original = report.results.find(row => row.provider === 'pi-original' && row.scenario === fixture.name);
    const native = report.results.find(row => row.provider === 'xnip-native' && row.scenario === fixture.name);
    const time = row => `${row.summary.edit_ms.median.toFixed(2)} / ${row.summary.edit_ms.p95.toFixed(2)}`;
    const bytes = row => Math.round(row.summary.tool_bytes.median).toLocaleString('en-US');
    lines.push(`| ${fixture.name} | ${time(original)} | ${time(native)} | ${original.summary.read_ms.median.toFixed(2)} / ${native.summary.read_ms.median.toFixed(2)} | ${bytes(original)} / ${bytes(native)} | ${original.correct_runs}/${runs} / ${native.correct_runs}/${runs} |`);
  }
  lines.push('', 'The independent oracle checks final bytes, expected host error text and the complete fixture file set. Raw JSON retains each call, local argument/result bytes, file hashes, model counter and source/package fingerprints. The historical 13/41 matrix remains its original cohort. Scheduling, GC, page cache and a small run count affect observed latency.', '',
    '## Reproduce', '', '```sh', 'node benches/pi_provider.mjs --executable target/release/xnip --repeats 3 --warmups 1',
    'node --test-reporter=tap tests/pi_provider_bench_contract.mjs', '```', '',
    'The runtime fixture prepares pinned official Pi 1.3.0 packages in target/native-bench/pi-v1.3.0. See the [DSH provider cohort](native-provider-results.md) for its separately measured tool, governance and native-IPC boundary.', '');
  return lines.join('\n');
}

async function main() {
  const options = {};
  let output = path.join(root, 'docs/benchmarks/pi-provider-results');
  for (let index = 2; index < process.argv.length; index++) {
    const argument = process.argv[index];
    if (argument === '--executable') options.executable = process.argv[++index];
    else if (argument === '--repeats') options.repeats = Number(process.argv[++index]);
    else if (argument === '--warmups') options.warmups = Number(process.argv[++index]);
    else if (argument === '--scenarios') options.scenarioNames = process.argv[++index].split(',');
    else if (argument === '--build-profile') options.buildProfile = process.argv[++index];
    else if (argument === '--output-prefix') output = process.argv[++index];
    else throw new Error(`Unknown argument ${argument}`);
  }
  const report = await runPiBenchmark({ ...options, onProgress: text => process.stderr.write(text + '\n') });
  await fs.mkdir(path.dirname(output), { recursive: true });
  await fs.writeFile(output + '.json', JSON.stringify(report, null, 2) + '\n');
  await fs.writeFile(output + '.md', renderPiReport(report));
  console.log(`${report.results.length} Pi provider groups written to ${path.relative(root, output)}.{json,md}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
