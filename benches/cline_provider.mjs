#!/usr/bin/env node
/** Official Cline SDK tool dispatch with its original and xnip apply_patch executors. */
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { scenarios, statistics, root } from './native_provider.mjs';

const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const fileInfo = value => ({ bytes: Buffer.byteLength(value), sha256: hash(value) });
const measures = ['read_ms', 'patch_ms', 'tool_ms', 'tool_calls', 'argument_bytes', 'result_bytes', 'output_bytes', 'tool_bytes'];
const patch = (...body) => ['*** Begin Patch', ...body, '*** End Patch'].join('\n');

export async function clineScenarios() {
  const fixtures = (await scenarios()).filter(item => item.cohort === 'original13' || /^large-\d+$/.test(item.name))
    .map(item => ({ ...item, purpose: item.name === 'same-file-batch' ? 'two blocks in one apply_patch call'
      : item.name === 'multi-file-batch' ? 'three files in one apply_patch call' : item.purpose }));
  // Generate the supplied native representation before timing. splitlines without
  // terminators keeps every patch instruction separate, including a file with no EOF LF.
  const patches = JSON.parse(execFileSync(process.env.XNIP_TEST_PYTHON ?? 'python3', ['-c', `import difflib,json,sys
def lines(text):
    return text.replace('\\r\\n', '\\n').splitlines()
result={}
for fixture in json.load(sys.stdin):
    body=['*** Begin Patch']
    for name,before in fixture['files'].items():
        diff=list(difflib.unified_diff(lines(before),lines(fixture['expected'][name]),n=3,lineterm=''))
        if diff:
            body.append('*** Update File: '+name)
            body.extend('@@' if line.startswith('@@') else line for line in diff[2:])
    body.append('*** End Patch')
    result[fixture['name']]='\\n'.join(body)
print(json.dumps(result,ensure_ascii=False))
`], { input: JSON.stringify(fixtures), encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 }));
  const one = (name, before, body, expected, extra = {}) => ({ name, cohort: 'cline-extension',
    files: { 'example.txt': before }, expected: { 'example.txt': expected }, patch: patch(...body), ...extra });
  const generated = fixtures.map(item => ({ ...item, patch: patches[item.name], patchRepresentation: 'three-line unhinted context' }));
  const pressure = generated.find(item => item.name === 'large-65536');
  const hinted = generated.filter(item => /^large-\d+$/.test(item.name)).map(item => ({ ...item,
    name: 'cline-eof-hint-' + item.name, cohort: 'cline-eof-hint', editLocation: 'EOF',
    purpose: item.purpose + '; EOF location supplied by the official patch marker',
    patch: item.patch.replace('\n*** End Patch', '\n \n*** End of File\n*** End Patch'),
    patchRepresentation: 'three-line context with trailing empty context and official End of File hint' }));
  return [...generated.filter(item => item.cohort === 'original13'),
    { ...pressure, cohort: 'cline-unhinted-context', editLocation: 'EOF', purpose: '64 KiB repeated-line EOF context scan without a location hint' }, ...hinted,
    one('cline-fuzzy-whitespace', 'head\n  old  \nend\n', ['*** Update File: example.txt', '@@', '-old', '+new'],
      'head\nnew\nend\n', { purpose: 'official whitespace fallback matches a whole patch line' }),
    one('cline-first-match', 'same\nsame\n', ['*** Update File: example.txt', '@@', '-same', '+new'],
      'new\nsame\n', { purpose: 'official patch context selects the first matching line' }),
    one('cline-noop', 'same\n', ['*** Update File: example.txt', '@@', '-same', '+same'],
      'same\n', { purpose: 'identical patch retains the official success response' }),
    one('cline-batch-adjacent', 'left=1\nright=2\ntail\n', ['*** Update File: example.txt', '@@', '-left=1', '-right=2', '+left=10', '+right=20'],
      'left=10\nright=20\ntail\n', { purpose: 'neighboring changes in one native patch chunk' }),
    one('cline-batch-disjoint', 'head\nleft=1\nkeep\nkeep\nright=2\ntail\n',
      ['*** Update File: example.txt', '@@', '-left=1', '+left=10', '@@', '-right=2', '+right=20'],
      'head\nleft=10\nkeep\nkeep\nright=20\ntail\n', { purpose: 'two native chunks in one file and tool call' }),
    one('cline-bom-crlf', '\ufeffhead\r\nold\r\ntail\r\n', ['*** Update File: example.txt', '@@', '-old', '+new'],
      '\ufeffhead\r\nnew\r\ntail\r\n', { purpose: 'retain BOM and the source CRLF style' }),
    one('cline-reject-context', 'old\n', ['*** Update File: example.txt', '@@', '-missing', '+new'],
      'old\n', { error: 'did not match the current file content', purpose: 'unmatched patch context preserves storage' }),
    one('cline-reject-existing-add', 'old\n', ['*** Add File: example.txt', '+new'],
      'old\n', { error: 'File already exists', purpose: 'add-existing guard preserves the existing file' }),
    one('cline-reject-sentinels', 'old\n', [], 'old\n', { patch: '*** Begin Patch\n*** Update File: example.txt\n@@\n-old\n+new',
      error: 'incomplete sentinels', purpose: 'an incomplete patch returns the official validation failure' }),
    one('cline-alias-update-update', 'old\n', ['*** Update File: example.txt', '@@', '-old', '+first',
      '*** Update File: ./example.txt', '@@', '-old', '+second'], 'second\n',
      { purpose: 'distinct patch path spellings update one target using original contents; the later result wins' }),
    { name: 'cline-alias-add-add', cohort: 'cline-extension', files: {}, expected: { 'created.txt': 'second' },
      patch: patch('*** Add File: created.txt', '+first', '*** Add File: ./created.txt', '+second'),
      purpose: 'two add spellings for an initially absent target retain the later content and its EOF bytes' },
    { name: 'cline-alias-delete-delete', cohort: 'cline-extension', files: { 'example.txt': 'old\n' }, expected: {},
      patch: patch('*** Delete File: example.txt', '*** Delete File: ./example.txt'),
      purpose: 'two delete spellings for one original target retain official success and final absence' },
    { name: 'cline-alias-move-update', cohort: 'cline-extension', files: { 'source.txt': 'source\n', 'destination.txt': 'target\n' },
      expected: { 'destination.txt': 'final\n' },
      patch: patch('*** Update File: source.txt', '*** Move to: destination.txt', '@@', '-source', '+moved',
        '*** Update File: ./destination.txt', '@@', '-target', '+final'),
      purpose: 'a later alias update uses the initial destination context after move-overwrite' },
    { name: 'cline-alias-same-path-move', cohort: 'cline-extension', files: { 'example.txt': 'old\n' }, expected: {},
      patch: patch('*** Update File: example.txt', '*** Move to: ./example.txt', '@@', '-old', '+moved'),
      purpose: 'the official same-target move writes then removes the target, yielding final absence' },
  ];
}

async function sourceProvenance() {
  const names = ['Cargo.toml', 'Cargo.lock', 'benches/cline_provider.mjs', 'benches/native_provider.mjs',
    'benches/edit_cost.py', 'benches/native/cline_native_setup.mjs', 'benches/native/node_runtime.mjs', 'tests/cline_native_runtime.mjs'];
  async function visit(directory) {
    for (const item of await fs.readdir(path.join(root, directory), { withFileTypes: true })) {
      const name = path.posix.join(directory, item.name);
      if (item.isDirectory()) await visit(name);
      else if (/\.(rs|mjs|json)$/.test(item.name)) names.push(name);
    }
  }
  for (const directory of ['src', 'integrations/native', 'integrations/cline-native', 'benches/native/cline_native']) await visit(directory);
  const files = Object.fromEntries(await Promise.all(names.sort().map(async name => [name, hash(await fs.readFile(path.join(root, name)))])));
  return { sha256: hash(JSON.stringify(files)), files_sha256: files };
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

async function sample(runtime, fixture, cwd, relative) {
  const directory = path.join(cwd, relative);
  await fs.rm(directory, { recursive: true, force: true });
  await fs.mkdir(directory, { recursive: true });
  for (const [name, value] of Object.entries(fixture.files)) {
    await fs.mkdir(path.dirname(path.join(directory, name)), { recursive: true });
    await fs.writeFile(path.join(directory, name), value);
  }
  const calls = [], eventStart = runtime.events.length;
  async function call(phase, name, args) {
    const started = performance.now();
    const result = await runtime.call(name, args);
    const elapsed_ms = performance.now() - started;
    const argsJson = JSON.stringify(args), resultJson = JSON.stringify(result), outputJson = JSON.stringify(result.output);
    const is_error = Boolean(result.isError || (name === 'read_files' ? result.output.some(item => !item.success) : !result.output.success));
    calls.push({ phase, name, elapsed_ms, argument_bytes: Buffer.byteLength(argsJson), result_bytes: Buffer.byteLength(resultJson),
      output_bytes: Buffer.byteLength(outputJson), argument_sha256: hash(argsJson), output_sha256: hash(outputJson), is_error,
      dispatch_is_error: result.isError === true,
      ...(is_error ? { error: outputJson.replaceAll(directory, '<fixture>').replaceAll(relative, '<case>') } : {}) });
    return calls.at(-1);
  }
  if (Object.keys(fixture.files).length) {
    const read = await call('read', 'read_files', { files: Object.keys(fixture.files).map(name => ({ path: path.join(directory, name) })) });
    assert.equal(read.is_error, false, `${fixture.name}: prerequisite read`);
  }
  // Keep each marker's spelling: the official executor distinguishes a and ./a
  // while computing changes, then resolves them to the same storage target.
  const input = fixture.patch.replace(/^(\*\*\* (?:(?:Update|Add|Delete) File|Move to): )(.+)$/gm,
    (_, marker, name) => marker + relative + '/' + name);
  await call('patch', 'apply_patch', { input });
  const outcomes = {};
  for (const [name, expected] of Object.entries(fixture.expected)) {
    const actual = await fs.readFile(path.join(directory, name));
    outcomes[name] = { expected: fileInfo(expected), actual: fileInfo(actual), byte_exact: actual.equals(Buffer.from(expected)) };
  }
  const unexpected_files = (await storedFiles(directory)).filter(name => !Object.hasOwn(fixture.expected, name));
  const patches = calls.filter(call => call.phase === 'patch');
  const outcome_matches = patches.every(call => fixture.error ? call.is_error && call.error.includes(fixture.error) : !call.is_error);
  const byte_exact = Object.values(outcomes).every(outcome => outcome.byte_exact) && unexpected_files.length === 0;
  const sum = field => calls.reduce((total, call) => total + call[field], 0);
  const events = runtime.events.slice(eventStart);
  const agent_events = { started: events.filter(event => event.type === 'tool-started').length,
    finished: events.filter(event => event.type === 'tool-finished').length };
  assert.equal(runtime.modelCalls, 0);
  assert.equal(runtime.agent.snapshot().status, 'idle');
  return { read_ms: calls.filter(call => call.phase === 'read').reduce((total, call) => total + call.elapsed_ms, 0),
    patch_ms: patches.reduce((total, call) => total + call.elapsed_ms, 0), tool_ms: sum('elapsed_ms'),
    tool_calls: calls.length, argument_bytes: sum('argument_bytes'), result_bytes: sum('result_bytes'), output_bytes: sum('output_bytes'),
    tool_bytes: sum('argument_bytes') + sum('result_bytes'), calls, outcomes, unexpected_files, agent_events,
    byte_exact, outcome_matches, correct: byte_exact && outcome_matches, model_calls: runtime.modelCalls };
}

export function validateClineReport(report) {
  assert.equal(report.results.length, report.scenarios.length * 2);
  assert.equal(new Set(report.results.map(row => `${row.provider}/${row.scenario}`)).size, report.results.length);
  for (const row of report.results) {
    assert.ok(['cline-original', 'xnip-native'].includes(row.provider));
    const scenario = report.scenarios.find(item => item.name === row.scenario);
    assert.ok(scenario);
    assert.equal(row.samples.length, report.methodology.measured_runs_per_scenario);
    assert.equal(row.warmups.length, report.methodology.warmup_runs_per_scenario);
    for (const sample of [...row.warmups, ...row.samples]) {
      assert.equal(sample.model_calls, 0);
      assert.equal(sample.tool_calls, sample.calls.length);
      assert.equal(sample.calls.filter(call => call.phase === 'read').length, scenario.read_calls);
      assert.equal(sample.calls.filter(call => call.phase === 'patch').length, 1);
      assert.equal(sample.agent_events.started, sample.tool_calls);
      assert.equal(sample.agent_events.finished, sample.tool_calls);
      assert.ok(sample.calls.every(call => Number.isFinite(call.elapsed_ms) && call.elapsed_ms >= 0));
      for (const field of ['argument_bytes', 'result_bytes', 'output_bytes']) assert.equal(sample[field], sample.calls.reduce((total, call) => total + call[field], 0));
      assert.equal(sample.tool_bytes, sample.argument_bytes + sample.result_bytes);
      assert.ok(Math.abs(sample.tool_ms - sample.read_ms - sample.patch_ms) < 0.001);
      assert.deepEqual(Object.keys(sample.outcomes).sort(), Object.keys(scenario.expected).sort(), 'complete expected file oracle');
      for (const [name, outcome] of Object.entries(sample.outcomes)) {
        assert.deepEqual(outcome.expected, scenario.expected[name]);
        assert.equal(outcome.byte_exact, outcome.expected.bytes === outcome.actual.bytes && outcome.expected.sha256 === outcome.actual.sha256);
      }
      assert.equal(sample.byte_exact, Object.values(sample.outcomes).every(outcome => outcome.byte_exact) && sample.unexpected_files.length === 0);
      assert.equal(sample.outcome_matches, sample.calls.filter(call => call.phase === 'patch').every(call => scenario.expected_error
        ? call.is_error && call.error.includes(scenario.expected_error) : !call.is_error));
      assert.equal(sample.correct, sample.byte_exact && sample.outcome_matches);
    }
    for (const field of measures) assert.deepEqual(row.summary[field], statistics(row.samples.map(sample => sample[field])));
    for (const [field, sampleField] of [['byte_exact_runs', 'byte_exact'], ['outcome_matches_runs', 'outcome_matches'], ['correct_runs', 'correct']])
      assert.equal(row[field], row.samples.filter(sample => sample[sampleField]).length, `${field} derived from samples`);
    assert.equal(row.correct_runs, row.samples.length, `${row.provider}/${row.scenario}: independent byte and host outcome oracle`);
  }
  for (const row of report.results.filter(row => row.provider === 'xnip-native')) {
    const original = report.results.find(item => item.provider === 'cline-original' && item.scenario === row.scenario);
    for (let index = 0; index < row.samples.length; index++) assert.deepEqual(
      row.samples[index].calls.map(call => [call.name, call.argument_sha256, call.output_sha256, call.is_error]),
      original.samples[index].calls.map(call => [call.name, call.argument_sha256, call.output_sha256, call.is_error]),
      `${row.scenario}: same host tool arguments and output`);
  }
}

export async function runClineBenchmark({ executable = path.join(root, 'target/release/xnip'), repeats = 3,
  warmups = 1, scenarioNames, buildProfile = 'release', onProgress = () => {} } = {}) {
  assert.ok(Number.isInteger(repeats) && repeats > 0);
  assert.ok(Number.isInteger(warmups) && warmups > 0, 'a warm-up initializes the lazy native bridge');
  executable = path.resolve(executable);
  const fixtures = await clineScenarios();
  const selected = scenarioNames ? fixtures.filter(item => scenarioNames.includes(item.name)) : fixtures;
  assert.ok(selected.length);
  if (scenarioNames) assert.equal(selected.length, new Set(scenarioNames).size);
  const binary = await fs.readFile(executable), source = await sourceProvenance();
  const started = performance.now();
  const { createClineFixture } = await import('../tests/cline_native_runtime.mjs');
  const bootstrap_ms = performance.now() - started;
  const report = { schema_version: 1, created_at: new Date().toISOString(),
    methodology: { measured_runs_per_scenario: repeats, warmup_runs_per_scenario: warmups, model_calls: 0,
      host: 'published Cline SDK registry, Agent tool policy and before/after hooks; fixture calls the TypeScript-private executeToolCalls method exposed in published JavaScript',
      product_boundary: 'production uses the public createBuiltinTools executor hook under the normal Agent lifecycle; this fixture enters deterministic tool dispatch directly',
      native_seam: 'apply_patch executor; read_files remains the official builtin filesystem reader',
      matching: 'official computePatchChanges; known-input fixture differences become native line patches with three context lines',
      large_file_representation: 'three explicitly named EOF-hint fixtures retain the original repeated-line bytes and goal; unhinted repeated 64 KiB remains a separate context-scan pressure case',
      batching: 'all files and chunks in one apply_patch call; preparatory reads are grouped in one read_files call',
      timed_region: 'sum of performance.now tool execution durations; fixture creation, patch generation, verification and byte counting are outside',
      setup: 'factory, SDK verification and Agent initialization per lane; first lane includes cold SDK import; native process starts lazily at the first warm-up patch that reaches persistence',
      process_lifetime: 'official SDK wrapper timeout timers keep Node alive after returned tool results; this idle process lifetime is outside warm tool execution timing',
      bytes: 'UTF-8 JSON.stringify public arguments and complete returned SDK tool-result part, including its call ID; output_bytes separately counts the actual output object',
      error_outcomes: 'SDK dispatch isError and operation output.success are recorded separately; official wrappers may return success:false with dispatch isError absent',
      p95: 'nearest rank; three samples give the maximum', ordering: 'original then native; fixture order; per-fixture warm-up',
      scope: 'SDK tool executor cohort with model stream counter; model generation, IDE UI and billing are outside this measurement' },
    provenance: { node: process.version, platform: os.platform(), arch: os.arch(), kernel: os.release(), cpu_model: os.cpus()[0]?.model,
      binary: { filename: path.basename(executable), build_profile: buildProfile, bytes: binary.length, sha256: hash(binary) }, source },
    bootstrap_ms, scenarios: selected.map(item => ({ name: item.name, cohort: item.cohort, purpose: item.purpose,
      expected_error: item.error ?? null, read_calls: Object.keys(item.files).length ? 1 : 0, patch_sha256: hash(item.patch),
      patch_utf8_bytes: Buffer.byteLength(item.patch), patch_representation: item.patchRepresentation ?? 'native patch instructions',
      edit_location: item.editLocation ?? 'fixture-supplied changes',
      files: Object.fromEntries(Object.entries(item.files).map(([name, value]) => [name, fileInfo(value)])),
      expected: Object.fromEntries(Object.entries(item.expected).map(([name, value]) => [name, fileInfo(value)])) })), setup: [], results: [] };
  const cwd = await fs.mkdtemp(path.join(root, 'target/cline-provider-bench-'));
  let expectedCatalog;
  try {
    for (const native of [false, true]) {
      const provider = native ? 'xnip-native' : 'cline-original';
      const started = performance.now();
      const runtime = await createClineFixture({ cwd, executable, native });
      const setup_ms = performance.now() - started;
      try {
        const description = await runtime.describe();
        assert.deepEqual(description.tools.map(tool => tool.name), ['read_files', 'apply_patch']);
        if (expectedCatalog) assert.deepEqual(description, expectedCatalog, 'same official registry and schema');
        else expectedCatalog = description;
        if (report.provenance.host) assert.deepEqual(runtime.provenance, report.provenance.host);
        else report.provenance.host = runtime.provenance;
        report.setup.push({ provider, setup_ms, tool_schema_sha256: hash(JSON.stringify(description)) });
        for (const fixture of selected) {
          const samples = [], warmupResults = [];
          for (let index = 0; index < warmups + repeats; index++) {
            const result = await sample(runtime, fixture, cwd, path.posix.join('cases', fixture.name, String(index)));
            (index < warmups ? warmupResults : samples).push(result);
          }
          const summary = Object.fromEntries(measures.map(field => [field, statistics(samples.map(sample => sample[field]))]));
          report.results.push({ provider, scenario: fixture.name, cohort: fixture.cohort, summary,
            byte_exact_runs: samples.filter(sample => sample.byte_exact).length,
            outcome_matches_runs: samples.filter(sample => sample.outcome_matches).length,
            correct_runs: samples.filter(sample => sample.correct).length, warmups: warmupResults, samples });
          onProgress(`${provider} ${fixture.name}: ${summary.patch_ms.median.toFixed(2)} ms patch`);
        }
        assert.equal(runtime.modelCalls, 0);
      } finally { await runtime.close(); }
    }
  } finally { await fs.rm(cwd, { recursive: true, force: true }); }
  assert.ok(binary.equals(await fs.readFile(executable)), 'measured binary stayed fixed');
  assert.equal((await sourceProvenance()).sha256, source.sha256, 'measured source stayed fixed');
  validateClineReport(report);
  return report;
}

export function renderClineReport(report) {
  const runs = report.methodology.measured_runs_per_scenario;
  const lines = ['# Cline apply_patch provider benchmark', '',
    `Cline SDK ${report.provenance.host.version}; ${report.scenarios.length} fixtures; ${runs} measured runs and ${report.methodology.warmup_runs_per_scenario} warm-up per fixture/provider. Measured ${report.created_at}. Zero actual model stream calls.`, '',
    'Both lanes use the published SDK registry, read_files and apply_patch tools, schema validation, tool policy and before/after hooks. Production uses the public createBuiltinTools executor hook under the normal Agent lifecycle. This fixture calls Agent.executeToolCalls, a TypeScript-private method exposed in published JavaScript, to enter deterministic tool dispatch with zero model stream calls. The original lane uses the packaged patch executor; xnip replaces that executor and retains official computePatchChanges matching and output wrapping. Catalogs and actual operation outputs are checked equal.', '',
    'The native backend covers apply_patch; read_files retains its original reader. This SDK cohort measures local tool execution. Interactive IDE flows, model generation and billing have their own boundaries.', '',
    'The original 13 fixtures retain independent expected bytes. Native patches use three-line context from their known before/after text; patch generation runs before timing. Multiple files and chunks share one patch call and one grouped preparatory read. Added cases exercise official whitespace matching, first-match context, no-op success, batches, BOM/CRLF, rejection outcomes and path aliases. Alias marker spellings remain intact when cases are placed in their fixture directories; the independent oracle records final content or final absence for the official update/add/delete/move order.', '',
    'Large-file changes sit at EOF. Three explicitly named EOF-hint cases preserve the original repeated-line bytes and goal while supplying the official End of File marker and trailing empty context. A separate unhinted repeated 64 KiB case retains context-scan pressure. The [official parser component probe](cline-matcher-probe.json) compares the same 64 KiB input/goal: 459-byte unhinted patch versus 477-byte hinted patch. The [earlier partial diagnostic](cline-provider-partial-diagnostic.json) preserves unhinted 1 MiB progress around 13.9 seconds; sampling stopped during 5 MiB before a complete report. These diagnostics have separate component/partial boundaries from this complete SDK matrix.', '',
    'Fixture reset, byte verification and serialization counting are outside timing. One persistent Agent serves each lane. Setup covers factory, SDK verification and Agent initialization; the first original lane includes cold SDK import. The native process initializes lazily when a warm-up patch first reaches persistence. Raw warm-up results retain that initialization cost separately. Official SDK wrapper timeout timers keep Node alive after tool results return; this idle process lifetime is outside warm tool execution timing. Median and nearest-rank p95 use measured samples; with three runs p95 is the maximum.', '',
    `Binary ${report.provenance.binary.build_profile}, SHA-256 \`${report.provenance.binary.sha256}\`; source \`${report.provenance.source.sha256}\`. Node ${report.provenance.node}; ${report.provenance.platform}/${report.provenance.arch}; ${report.provenance.cpu_model}. Helper import ${report.bootstrap_ms.toFixed(2)} ms. Package entry and lock fingerprints are in the JSON companion.`, '',
    '| Provider | Factory / Agent setup ms | First warm-up patch ms |', '|---|---:|---:|'];
  for (const item of report.setup) {
    const first = report.results.find(row => row.provider === item.provider);
    lines.push(`| ${item.provider} | ${item.setup_ms.toFixed(2)} | ${first.warmups[0].patch_ms.toFixed(2)} |`);
  }
  lines.push('', '| Fixture | Original patch median / p95 ms | xnip patch median / p95 ms | Original / xnip read median ms | Original / xnip SDK tool bytes | Original / xnip correct |',
    '|---|---:|---:|---:|---:|---:|');
  for (const fixture of report.scenarios) {
    const original = report.results.find(row => row.provider === 'cline-original' && row.scenario === fixture.name);
    const native = report.results.find(row => row.provider === 'xnip-native' && row.scenario === fixture.name);
    const time = row => `${row.summary.patch_ms.median.toFixed(2)} / ${row.summary.patch_ms.p95.toFixed(2)}`;
    const bytes = row => Math.round(row.summary.tool_bytes.median).toLocaleString('en-US');
    lines.push(`| ${fixture.name} | ${time(original)} | ${time(native)} | ${original.summary.read_ms.median.toFixed(2)} / ${native.summary.read_ms.median.toFixed(2)} | ${bytes(original)} / ${bytes(native)} | ${original.correct_runs}/${runs} / ${native.correct_runs}/${runs} |`);
  }
  lines.push('', 'SDK tool bytes sum public arguments and the actual returned tool-result part, including its call ID. Raw output_bytes separately counts the operation output. An operation success:false is an error outcome even when dispatch isError is absent. The byte oracle verifies every expected file, its independent expected hash and the complete fixture file set. Per-call hashes verify original/native arguments and outputs match.', '',
    'The JSON companion retains warm-up and measured samples, per-call timings/bytes/errors, actual Agent started/finished event counts, model stream counters and source/package fingerprints. The historical 13/41 matrix remains its original cohort. Scheduling, GC, page cache and a small run count affect observed latency.', '',
    '## Reproduce', '', '```sh', 'node benches/native/cline_native_setup.mjs',
    'node benches/cline_provider.mjs --executable target/release/xnip --repeats 3 --warmups 1',
    'node --test-reporter=tap tests/cline_provider_bench_contract.mjs', '```', '',
    'The fixed SDK packages are prepared under target/native-hosts/cline-0.2.0. See the independently measured [DSH](native-provider-results.md) and [Pi](pi-provider-results.md) cohorts for their respective tool and governance boundaries.', '');
  return lines.join('\n');
}

async function main() {
  const options = {};
  let output = path.join(root, 'docs/benchmarks/cline-provider-results');
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
  const report = await runClineBenchmark({ ...options, onProgress: text => process.stderr.write(text + '\n') });
  await fs.mkdir(path.dirname(output), { recursive: true });
  await fs.writeFile(output + '.json', JSON.stringify(report, null, 2) + '\n');
  await fs.writeFile(output + '.md', renderClineReport(report));
  console.log(`${report.results.length} Cline provider groups written to ${path.relative(root, output)}.{json,md}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
