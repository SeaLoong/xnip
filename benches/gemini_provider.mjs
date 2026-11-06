#!/usr/bin/env node
/** Official Gemini SDK tools with original and xnip-backed local filesystem services. */
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { scenarios, statistics, root } from './native_provider.mjs';
import { EditorClient } from '../integrations/native/editor-client.mjs';

const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const fileInfo = value => ({ bytes: Buffer.byteLength(value), sha256: hash(value) });
const measures = ['read_ms', 'mutation_ms', 'tool_ms', 'tool_calls', 'argument_bytes', 'result_bytes', 'exception_bytes', 'tool_bytes'];
const bridgeMeasures = ['requests', 'request_bytes', 'response_bytes', 'total_bytes'];

export async function geminiScenarios() {
  const baseline = (await scenarios()).filter(item => item.cohort === 'original13' || /^large-\d+$/.test(item.name));
  const one = (name, before, old, text, expected, fields = {}) => ({ name, cohort: 'gemini-extension',
    files: { 'example.txt': before }, edits: [{ file: 'example.txt', old, text }],
    expected: { 'example.txt': expected }, ...fields });
  return [...baseline,
    one('gemini-regex-spacing', 'head\n    return   1;\ntail\n', 'return 1;', 'return 2;', 'head\n    return 2;\ntail\n',
      { purpose: 'official whitespace strategy resolves a supplied near match' }),
    one('gemini-reject-missing', 'old\n', 'missing', 'new', 'old\n',
      { error: 'edit_no_occurrence_found', purpose: 'official missing-match result preserves storage' }),
    one('gemini-reject-count', 'old\nold\n', 'old', 'new', 'old\nold\n',
      { error: 'edit_expected_occurrence_mismatch', purpose: 'default occurrence count rejects an ambiguous edit' }),
    one('gemini-reject-explicit-count', 'old\n', 'old', 'new', 'old\n',
      { error: 'edit_expected_occurrence_mismatch', expectedReplacements: 2, purpose: 'explicit occurrence count is checked by the host' }),
    one('gemini-noop', 'old\n', 'old', 'old', 'old\n',
      { error: 'edit_no_change', purpose: 'official identical-edit rejection preserves storage' }),
    { name: 'gemini-edit-create', cohort: 'gemini-extension', files: {},
      edits: [{ file: 'created.txt', old: '', text: 'created\r\n尾巴' }], expected: { 'created.txt': 'created\r\n尾巴' },
      purpose: 'replace creates a new file when old_string is empty' },
    one('gemini-reject-existing-create', 'existing\n', '', 'created\n', 'existing\n',
      { error: 'attempt_to_create_existing_file', purpose: 'empty search on an existing file retains host rejection' }),
    { name: 'gemini-write-create', cohort: 'gemini-extension', files: {}, edits: [],
      writes: [{ file: 'created.txt', text: 'created\r\n尾巴' }], expected: { 'created.txt': 'created\r\n尾巴' },
      purpose: 'write_file creates text through the registered official tool' },
    { name: 'gemini-write-overwrite', cohort: 'gemini-extension', files: { 'example.txt': 'old\r\n' }, edits: [],
      writes: [{ file: 'example.txt', text: 'new\r\n尾巴' }], expected: { 'example.txt': 'new\r\n尾巴' },
      purpose: 'write_file replaces observed text through the same host pipeline' },
    { name: 'gemini-write-noop', cohort: 'gemini-extension', files: { 'example.txt': 'same\r\n尾巴' }, edits: [],
      writes: [{ file: 'example.txt', text: 'same\r\n尾巴' }], expected: { 'example.txt': 'same\r\n尾巴' },
      purpose: 'write_file receives identical content' },
    one('gemini-bom-crlf', '\ufeffhead\r\nmode=old\r\n尾巴', 'mode=old', 'mode=new', '\ufeffhead\r\nmode=new\r\n尾巴',
      { purpose: 'BOM and CRLF storage fixture' }),
    one('gemini-mixed-eol', 'head\r\nold\r\nblock\nend\r\n', 'old\nblock', 'new\nblock', 'head\r\nnew\nblock\nend\r\n',
      { purpose: 'independent storage oracle exposes the host newline policy' }),
    one('gemini-confirmed-edit', 'old\n', 'old', 'new', 'new\n',
      { policy: 'ask_user', purpose: 'official confirmation details and ProceedOnce callback precede execution' }),
    one('gemini-policy-denied', 'old\n', 'old', 'new', 'old\n',
      { policy: 'deny', observe: false, exception: 'denied by policy', purpose: 'official policy rejection preserves storage' }),
  ];
}

function bridgeCounter() {
  const totals = { requests: 0, request_bytes: 0, response_bytes: 0, actions: {} };
  const snapshot = () => structuredClone(totals);
  return {
    snapshot,
    createClient(executable, cwd) {
      const client = new EditorClient(executable, cwd);
      const write = client.child.stdin.write.bind(client.child.stdin);
      client.child.stdin.write = (chunk, ...args) => { totals.request_bytes += Buffer.byteLength(chunk); return write(chunk, ...args); };
      client.child.stdout.on('data', chunk => { totals.response_bytes += chunk.length; });
      const request = client.request.bind(client);
      client.request = (action, ...args) => {
        totals.requests++;
        totals.actions[action] = (totals.actions[action] ?? 0) + 1;
        return request(action, ...args);
      };
      return client;
    },
    delta(before) {
      const request_bytes = totals.request_bytes - before.request_bytes;
      const response_bytes = totals.response_bytes - before.response_bytes;
      return { requests: totals.requests - before.requests, request_bytes, response_bytes, total_bytes: request_bytes + response_bytes,
        actions: Object.fromEntries(Object.entries(totals.actions).map(([action, count]) => [action, count - (before.actions[action] ?? 0)]).filter(([, count]) => count)) };
    },
  };
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

async function sourceProvenance() {
  const names = ['Cargo.toml', 'Cargo.lock', 'benches/gemini_provider.mjs', 'benches/native_provider.mjs', 'benches/edit_cost.py',
    'tests/gemini_native_runtime.mjs', 'benches/native/gemini_native_setup.mjs', 'benches/native/node_runtime.mjs'];
  async function visit(directory) {
    for (const entry of await fs.readdir(path.join(root, directory), { withFileTypes: true })) {
      const name = path.posix.join(directory, entry.name);
      if (entry.isDirectory()) await visit(name);
      else if (/\.(rs|mjs|json)$/.test(entry.name)) names.push(name);
    }
  }
  for (const directory of ['src', 'integrations/native', 'integrations/gemini-native', 'benches/native/gemini_native']) await visit(directory);
  const files_sha256 = Object.fromEntries(await Promise.all(names.sort().map(async name => [name, hash(await fs.readFile(path.join(root, name)))])));
  return { sha256: hash(JSON.stringify(files_sha256)), files_sha256 };
}

const matchesOutcome = (call, fixture) => fixture.exception ? call.exception?.message.includes(fixture.exception) === true
  : fixture.error ? call.is_error && call.error_type === fixture.error && !call.exception : !call.is_error;

async function sample(runtime, counter, fixture, directory) {
  await fs.mkdir(directory, { recursive: true });
  for (const [name, text] of Object.entries(fixture.files)) {
    await fs.mkdir(path.dirname(path.join(directory, name)), { recursive: true });
    await fs.writeFile(path.join(directory, name), text);
  }
  const calls = [];
  const bridgeStart = counter.snapshot();
  const eventStart = runtime.events.length;
  async function call(phase, name, args) {
    const argumentsJson = JSON.stringify(args);
    const record = { phase, name, argument_bytes: Buffer.byteLength(argumentsJson), argument_sha256: hash(argumentsJson) };
    const started = performance.now();
    let result;
    try {
      result = await runtime.call(name, args);
      record.elapsed_ms = performance.now() - started;
    } catch (error) {
      record.elapsed_ms = performance.now() - started;
      record.exception = { name: error.name, message: error.message.replaceAll(directory, '<fixture>') };
      record.exception_bytes = Buffer.byteLength(error.message);
    }
    const resultJson = result === undefined ? null : JSON.stringify(result);
    record.result_bytes = resultJson === null ? 0 : Buffer.byteLength(resultJson);
    record.result_sha256 = resultJson === null ? null : hash(resultJson);
    record.exception_bytes ??= 0;
    record.is_error = Boolean(record.exception || result?.error);
    record.error_type = result?.error?.type ?? null;
    if (result?.error) record.error = result.error.message.replaceAll(directory, '<fixture>');
    calls.push(record);
    return record;
  }
  if (fixture.observe !== false) for (const name of Object.keys(fixture.files)) {
    assert.equal((await call('read', 'read_file', { file_path: path.join(directory, name) })).is_error, false, fixture.name + ': read');
  }
  for (const edit of fixture.edits) {
    const args = { file_path: path.join(directory, edit.file), instruction: fixture.purpose,
      old_string: edit.old, new_string: edit.text };
    if (edit.all) args.expected_replacements = fixture.files[edit.file].split(edit.old).length - 1;
    if (fixture.expectedReplacements !== undefined) args.expected_replacements = fixture.expectedReplacements;
    await call('mutation', 'replace', args);
  }
  for (const write of fixture.writes ?? []) await call('mutation', 'write_file', { file_path: path.join(directory, write.file), content: write.text });
  const bridge = counter.delta(bridgeStart);
  const outcomes = {};
  for (const [name, expected] of Object.entries(fixture.expected)) {
    const actual = await fs.readFile(path.join(directory, name));
    outcomes[name] = { expected: fileInfo(expected), actual: fileInfo(actual), byte_exact: actual.equals(Buffer.from(expected)) };
  }
  const unexpected_files = (await storedFiles(directory)).filter(name => !Object.hasOwn(fixture.expected, name));
  const mutations = calls.filter(call => call.phase === 'mutation');
  const outcome_matches = mutations.every(call => matchesOutcome(call, fixture));
  const byte_exact = Object.values(outcomes).every(outcome => outcome.byte_exact) && unexpected_files.length === 0;
  const sum = field => calls.reduce((total, call) => total + call[field], 0);
  const policy_events = {};
  for (const event of runtime.events.slice(eventStart)) policy_events[event.type] = (policy_events[event.type] ?? 0) + 1;
  assert.equal(runtime.modelCalls, 0);
  return { read_ms: calls.filter(call => call.phase === 'read').reduce((total, call) => total + call.elapsed_ms, 0),
    mutation_ms: mutations.reduce((total, call) => total + call.elapsed_ms, 0), tool_ms: sum('elapsed_ms'),
    tool_calls: calls.length, argument_bytes: sum('argument_bytes'), result_bytes: sum('result_bytes'), exception_bytes: sum('exception_bytes'),
    tool_bytes: sum('argument_bytes') + sum('result_bytes') + sum('exception_bytes'), calls, bridge, policy_events, outcomes, unexpected_files,
    byte_exact, outcome_matches, correct: byte_exact && outcome_matches, model_calls: runtime.modelCalls };
}

export function validateGeminiReport(report) {
  assert.equal(report.results.length, report.scenarios.length * 2);
  assert.equal(new Set(report.results.map(row => `${row.provider}/${row.scenario}`)).size, report.results.length);
  for (const row of report.results) {
    assert.ok(['gemini-original', 'xnip-native'].includes(row.provider), 'declared Gemini provider');
    const fixture = report.scenarios.find(item => item.name === row.scenario);
    assert.ok(fixture);
    assert.equal(row.samples.length, report.methodology.measured_runs_per_scenario);
    for (const sample of row.samples) {
      assert.equal(sample.model_calls, 0);
      assert.equal(sample.tool_calls, sample.calls.length);
      assert.equal(sample.calls.filter(call => call.phase === 'read').length, fixture.read_calls);
      assert.equal(sample.calls.filter(call => call.phase === 'mutation').length, fixture.mutation_calls);
      assert.ok(sample.calls.every(call => Number.isFinite(call.elapsed_ms) && call.elapsed_ms >= 0));
      for (const field of ['argument_bytes', 'result_bytes', 'exception_bytes'])
        assert.equal(sample[field], sample.calls.reduce((total, call) => total + call[field], 0));
      assert.equal(sample.tool_bytes, sample.argument_bytes + sample.result_bytes + sample.exception_bytes);
      assert.ok(Math.abs(sample.tool_ms - sample.read_ms - sample.mutation_ms) < 0.001);
      assert.equal(sample.bridge.total_bytes, sample.bridge.request_bytes + sample.bridge.response_bytes);
      assert.deepEqual(Object.keys(sample.outcomes).sort(), Object.keys(fixture.expected).sort(), 'complete expected file oracle');
      for (const [name, outcome] of Object.entries(sample.outcomes)) {
        assert.deepEqual(outcome.expected, fixture.expected[name], 'file outcome uses the declared oracle');
        assert.equal(outcome.byte_exact, outcome.expected.bytes === outcome.actual.bytes && outcome.expected.sha256 === outcome.actual.sha256);
      }
      assert.equal(sample.byte_exact, Object.values(sample.outcomes).every(outcome => outcome.byte_exact) && sample.unexpected_files.length === 0);
      assert.equal(sample.outcome_matches, sample.calls.filter(call => call.phase === 'mutation')
        .every(call => matchesOutcome(call, { error: fixture.expected_error, exception: fixture.expected_exception })));
      assert.equal(sample.correct, sample.byte_exact && sample.outcome_matches);
      assert.equal(sample.outcome_matches, true, row.scenario + ': expected SDK result');
      if (row.provider === 'gemini-original') assert.equal(sample.bridge.requests, 0);
    }
    for (const field of measures) assert.deepEqual(row.summary[field], statistics(row.samples.map(sample => sample[field])));
    for (const field of bridgeMeasures) assert.deepEqual(row.summary.bridge[field], statistics(row.samples.map(sample => sample.bridge[field])));
    for (const [field, sampleField] of [['byte_exact_runs', 'byte_exact'], ['outcome_matches_runs', 'outcome_matches'], ['correct_runs', 'correct']])
      assert.equal(row[field], row.samples.filter(sample => sample[sampleField]).length, `${field} derived from samples`);
  }
  for (const row of report.results.filter(row => row.provider === 'xnip-native')) {
    const original = report.results.find(item => item.provider === 'gemini-original' && item.scenario === row.scenario);
    for (let index = 0; index < row.samples.length; index++) {
      const next = row.samples[index], previous = original.samples[index];
      assert.deepEqual(next.outcomes, previous.outcomes, row.scenario + ': same resulting storage bytes');
      assert.deepEqual(next.calls.map(call => [call.name, call.argument_sha256, call.result_sha256, call.exception, call.error_type]),
        previous.calls.map(call => [call.name, call.argument_sha256, call.result_sha256, call.exception, call.error_type]), row.scenario + ': same SDK inputs and outcomes');
    }
  }
}

export async function runGeminiBenchmark({ executable = path.join(root, 'target/release/xnip'), repeats = 3, warmups = 1,
  scenarioNames, buildProfile = 'release', hostDirectory, onProgress = () => {} } = {}) {
  assert.ok(Number.isInteger(repeats) && repeats > 0);
  assert.ok(Number.isInteger(warmups) && warmups >= 0);
  executable = path.resolve(executable);
  const fixtures = await geminiScenarios();
  const selected = scenarioNames ? fixtures.filter(item => scenarioNames.includes(item.name)) : fixtures;
  assert.ok(selected.length);
  if (scenarioNames) assert.equal(selected.length, new Set(scenarioNames).size);
  const binary = await fs.readFile(executable);
  const source = await sourceProvenance();
  const started = performance.now();
  const { prepareFixture, loadSdk, VERSION } = await import('../tests/gemini_native_runtime.mjs');
  await loadSdk(hostDirectory);
  const bootstrap_ms = performance.now() - started;
  const directory = hostDirectory ?? path.join(root, 'target/native-hosts/gemini-' + VERSION);
  const manifest = await fs.readFile(path.join(directory, 'node_modules/@google/gemini-cli-core/package.json'));
  const receipt = JSON.parse(await fs.readFile(path.join(directory, 'native-host.json')));
  const report = { schema_version: 1, created_at: new Date().toISOString(),
    methodology: { measured_runs_per_scenario: repeats, warmup_runs_per_scenario: warmups, model_calls: 0,
      host: 'official Config, ToolRegistry and read_file/replace/write_file; build, policy/confirmation and execute on each call',
      scope: 'Gemini SDK 0.27.2 local filesystem configuration; OS sandbox, CLI profiles, model turns and billing have separate boundaries',
      matching: 'official matching, occurrence validation, rendering and newline rules; Config disableLLMCorrection=true in both lanes',
      confirmation: 'official policy bus; ask_user invokes generated confirmation and ProceedOnce callback; human interaction delay is outside timing',
      known_inputs: 'fixtures supply literal replacements; global replacement specifies the exact expected_replacements count',
      timing: 'sum of performance.now tool durations; fixture creation, serialization counts and independent byte oracle are outside',
      setup: 'SDK verification/import bootstrap once; Config/auth fixture/registry/native handshake timed per provider and policy; persistent fixture warmed per scenario',
      tool_bytes: 'UTF-8 JSON.stringify tool arguments and actual ToolResult; thrown exception message bytes are a separate field and join the local boundary total',
      bridge_bytes: 'actual native child stdin/stdout; capability handshake recorded under setup and excluded from warm samples',
      correctness: 'complete expected file set and independent expected bytes; provider equivalence additionally checks actual result hashes and storage hashes',
      p95: 'nearest rank; three samples give the maximum', ordering: 'policy group; original then native; fixture order; per-fixture warm-up' },
    provenance: { node: process.version, platform: os.platform(), arch: os.arch(), kernel: os.release(), cpu_model: os.cpus()[0]?.model,
      binary: { filename: path.basename(executable), build_profile: buildProfile, bytes: binary.length, sha256: hash(binary) }, source,
      host: { name: '@google/gemini-cli-core', version: VERSION, package_sha256: hash(manifest),
        dependency_tree_sha256: receipt.dependency_tree_sha256, provenance: receipt.provenance } },
    bootstrap_ms, scenarios: selected.map(item => ({ name: item.name, cohort: item.cohort, purpose: item.purpose, policy: item.policy ?? 'allow',
      read_calls: item.observe === false ? 0 : Object.keys(item.files).length, mutation_calls: item.edits.length + (item.writes?.length ?? 0),
      expected_error: item.error ?? null, expected_exception: item.exception ?? null,
      files: Object.fromEntries(Object.entries(item.files).map(([name, text]) => [name, fileInfo(text)])),
      expected: Object.fromEntries(Object.entries(item.expected).map(([name, text]) => [name, fileInfo(text)])) })), setup: [], results: [] };
  const cwd = await fs.mkdtemp(path.join(root, 'target/gemini-provider-bench-'));
  try {
    for (const policy of [...new Set(selected.map(item => item.policy ?? 'allow'))]) {
      let catalog;
      for (const native of [false, true]) {
        const provider = native ? 'xnip-native' : 'gemini-original';
        const counter = bridgeCounter();
        const bridgeStart = counter.snapshot();
        const started = performance.now();
        const runtime = await prepareFixture({ cwd, executable, native, policyDecision: policy, directory, createClient: counter.createClient });
        const setup_ms = performance.now() - started;
        try {
          const description = runtime.describe();
          assert.equal(description.model_calls, 0);
          assert.deepEqual(description.tools.map(tool => tool.name).sort(), ['read_file', 'replace', 'write_file']);
          if (catalog) assert.deepEqual(description.tools, catalog, 'same registered SDK schemas');
          else catalog = description.tools;
          report.setup.push({ provider, policy, setup_ms, tool_schema_sha256: hash(JSON.stringify(description.tools)), bridge: counter.delta(bridgeStart) });
          for (const fixture of selected.filter(item => (item.policy ?? 'allow') === policy)) {
            const samples = [];
            for (let iteration = 0; iteration < warmups + repeats; iteration++) {
              const directory = path.join(cwd, fixture.name, String(iteration));
              await fs.rm(directory, { recursive: true, force: true });
              const result = await sample(runtime, counter, fixture, directory);
              if (iteration >= warmups) samples.push(result);
            }
            const summary = Object.fromEntries(measures.map(field => [field, statistics(samples.map(sample => sample[field]))]));
            summary.bridge = Object.fromEntries(bridgeMeasures.map(field => [field, statistics(samples.map(sample => sample.bridge[field]))]));
            const row = { provider, policy, scenario: fixture.name, summary, samples,
              byte_exact_runs: samples.filter(sample => sample.byte_exact).length, outcome_matches_runs: samples.filter(sample => sample.outcome_matches).length,
              correct_runs: samples.filter(sample => sample.correct).length };
            report.results.push(row);
            onProgress(`${provider} ${fixture.name}: ${summary.mutation_ms.median.toFixed(2)} ms mutation, ${row.correct_runs}/${repeats} storage+outcome exact`);
          }
        } finally { await runtime.close(); }
      }
    }
  } finally { await fs.rm(cwd, { recursive: true, force: true }); }
  assert.equal(hash(await fs.readFile(executable)), report.provenance.binary.sha256, 'measured binary stayed fixed');
  assert.equal((await sourceProvenance()).sha256, source.sha256, 'measured source stayed fixed');
  validateGeminiReport(report);
  return report;
}

export function renderGeminiReport(report) {
  const runs = report.methodology.measured_runs_per_scenario;
  const lines = ['# Gemini SDK filesystem provider benchmark', '',
    `Gemini SDK ${report.provenance.host.version}; ${report.scenarios.length} fixtures, ${runs} measured runs and ${report.methodology.warmup_runs_per_scenario} warm-up per lane. Zero model calls. Measured ${report.created_at}.`, '',
    'Both lanes use official Config and ToolRegistry services with read_file, replace and write_file. Each call builds the official invocation, checks the policy bus, performs any requested confirmation callback and executes the tool. The original service and xnip service retain identical registered schemas, matching, occurrence validation and rendering.', '',
    'This cohort covers local SDK filesystem configurations for the pinned version. Config uses its supported disableLLMCorrection=true option in both lanes. The official empty fake-response generator counts attempted model calls; every measured workflow must keep that counter at zero. CLI profiles, OS sandbox sessions, interactive user delay and model/billing measurements have their own boundaries.', '',
    'Fixtures supply known inputs. Global edits set expected_replacements to the fixture count; sequential edits use separate replace calls. Every existing file is read first except the policy-denied fixture. SDK bootstrap is timed separately, followed by per-policy provider setup. Warm timing includes the complete tool-call pipeline; fixture reset, JSON byte accounting and independent file verification run outside timing. Three samples make nearest-rank p95 the maximum.', '',
    'The byte oracle preserves input bytes outside the requested change. The host newline policy can produce a different stored representation in both lanes; those differences remain visible. Provider compatibility separately requires identical resulting file hashes and actual SDK tool-result hashes. Local tool bytes count arguments, actual ToolResult JSON and separately recorded exception messages. Native bridge bytes count actual child stdin/stdout and describe an additional local boundary.', '',
    `Binary SHA-256 \`${report.provenance.binary.sha256}\` (${report.provenance.binary.build_profile}); maintained source \`${report.provenance.source.sha256}\`. Node ${report.provenance.node}; ${report.provenance.platform}/${report.provenance.arch}; ${report.provenance.cpu_model}. SDK bootstrap ${report.bootstrap_ms.toFixed(2)} ms.`, '',
    '| Policy | Provider | Setup ms | Setup bridge bytes |', '|---|---|---:|---:|'];
  for (const setup of report.setup) lines.push(`| ${setup.policy} | ${setup.provider} | ${setup.setup_ms.toFixed(2)} | ${setup.bridge.total_bytes.toLocaleString('en-US')} |`);
  lines.push('', '| Fixture | Original mutation median / p95 ms | xnip mutation median / p95 ms | Original / xnip read median ms | Original / xnip tool bytes | xnip bridge bytes | Original / xnip byte+outcome exact |',
    '|---|---:|---:|---:|---:|---:|---:|');
  for (const fixture of report.scenarios) {
    const original = report.results.find(row => row.provider === 'gemini-original' && row.scenario === fixture.name);
    const native = report.results.find(row => row.provider === 'xnip-native' && row.scenario === fixture.name);
    const timing = row => `${row.summary.mutation_ms.median.toFixed(2)} / ${row.summary.mutation_ms.p95.toFixed(2)}`;
    const count = value => Math.round(value).toLocaleString('en-US');
    lines.push(`| ${fixture.name} | ${timing(original)} | ${timing(native)} | ${original.summary.read_ms.median.toFixed(2)} / ${native.summary.read_ms.median.toFixed(2)} | ${count(original.summary.tool_bytes.median)} / ${count(native.summary.tool_bytes.median)} | ${count(native.summary.bridge.total_bytes.median)} | ${original.correct_runs}/${runs} / ${native.correct_runs}/${runs} |`);
  }
  const differences = report.results.filter(row => row.byte_exact_runs < runs);
  lines.push('', `Storage oracle differences: ${differences.length ? differences.map(row => `\`${row.provider}/${row.scenario}\` (${row.byte_exact_runs}/${runs} exact)`).join(', ') : 'all measured storage bytes match the independent oracle'}.`, '',
    'Every native workflow matches the original provider’s actual SDK input/result fingerprints and final stored bytes. These local measurements preserve the observed execution costs; process IO and transaction work remain part of the native lane.', '',
    '## Reproduce', '', '```sh', 'node benches/native/gemini_native_setup.mjs', 'CARGO_BUILD_JOBS=4 cargo build --release --locked --offline',
    'node benches/gemini_provider.mjs --executable target/release/xnip --repeats 3 --warmups 1',
    'node --test-reporter=tap tests/gemini_provider_bench_contract.mjs', '```', '',
    'Prepared packages live under target/native-hosts/gemini-0.27.2. The JSON companion records per-call timings, tool and bridge byte counts, exceptions, policy events, expected/actual file hashes, full maintained-source fingerprints and the dependency-tree digest. Existing host and historical representation cohorts retain their own data.', '');
  return lines.join('\n');
}

async function main() {
  const options = {};
  let output = path.join(root, 'docs/benchmarks/gemini-provider-results');
  for (let index = 2; index < process.argv.length; index++) {
    const name = process.argv[index];
    if (name === '--executable') options.executable = process.argv[++index];
    else if (name === '--repeats') options.repeats = Number(process.argv[++index]);
    else if (name === '--warmups') options.warmups = Number(process.argv[++index]);
    else if (name === '--scenarios') options.scenarioNames = process.argv[++index].split(',');
    else if (name === '--build-profile') options.buildProfile = process.argv[++index];
    else if (name === '--host-directory') options.hostDirectory = process.argv[++index];
    else if (name === '--output-prefix') output = process.argv[++index];
    else throw new Error('Unknown argument ' + name);
  }
  const report = await runGeminiBenchmark({ ...options, onProgress: text => process.stderr.write(text + '\n') });
  await fs.mkdir(path.dirname(output), { recursive: true });
  await fs.writeFile(output + '.json', JSON.stringify(report, null, 2) + '\n');
  await fs.writeFile(output + '.md', renderGeminiReport(report));
  console.log(`${report.results.length} measured groups written to ${path.relative(root, output)}.{json,md}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
