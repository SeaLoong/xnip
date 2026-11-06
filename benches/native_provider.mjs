#!/usr/bin/env node
/** Real official DSH tool execution, with one persistent provider per policy mode. */
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { createFixture } from '../integrations/dsh-native/runtime.mjs';

const exec = promisify(execFile);
export const root = path.resolve(import.meta.dirname, '..');
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const bytes = value => Buffer.byteLength(JSON.stringify(value));
const fileInfo = value => ({ bytes: Buffer.byteLength(value), sha256: hash(value) });
const zeroBridge = () => ({ requests: 0, request_bytes: 0, response_bytes: 0, total_bytes: 0, actions: {} });
const sampleMeasures = ['read_ms', 'mutation_ms', 'tool_ms', 'tool_calls', 'argument_bytes', 'result_bytes', 'tool_bytes'];
const bridgeMeasures = ['requests', 'request_bytes', 'response_bytes', 'total_bytes'];

export function statistics(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return { runs: sorted.length, min: sorted[0],
    median: sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2,
    p95: sorted[Math.ceil(sorted.length * 0.95) - 1], max: sorted.at(-1) };
}

export async function scenarios() {
  const { stdout } = await exec(process.env.XNIP_TEST_PYTHON ?? 'python3', ['-c', `import json,sys
sys.path.insert(0, 'benches')
from edit_cost import scenarios
print(json.dumps([dict(name=s.name,files=s.files,edits=s.edits,expected=s.expected(),purpose=s.purpose) for s in scenarios()]))
`], { cwd: root });
  const original = JSON.parse(stdout).map(item => ({ ...item, cohort: 'original13',
    purpose: item.name.endsWith('-batch') ? item.purpose.replace('in one call', 'through separate official tool calls') : item.purpose }));
  assert.equal(original.length, 13);
  const one = (name, before, old, text, purpose, fields = {}) => ({ name, cohort: 'provider-extension',
    files: { 'example.txt': before }, edits: [{ file: 'example.txt', old, text, ...(fields.all ? { all: true } : {}) }],
    expected: { 'example.txt': before.replaceAll(old, () => text) }, purpose, ...fields });
  const extra = [
    one('bom-lf', '\ufeffhead\nmode=old\n尾巴', 'mode=old', 'mode=new', 'retain UTF-8 BOM and final bytes'),
    one('bom-crlf', '\ufeffhead\r\nmode=old\r\n尾巴', 'mode=old', 'mode=new', 'retain BOM and CRLF outside a literal edit'),
    one('mixed-eol-span', 'head\r\nold\r\nblock\nend\r\n', 'old\nblock', 'new\nblock', 'match the host LF view while preserving untouched storage',
      { expected: { 'example.txt': 'head\r\nnew\nblock\nend\r\n' } }),
    one('mixed-eol-all', 'A\r\nold\nA\nold\r\nEOF', 'A\nold', 'B\nnew', 'replace normalized matches with different raw newline spans',
      { all: true, expected: { 'example.txt': 'B\nnew\nB\nnew\r\nEOF' } }),
    { name: 'adjacent-edits', cohort: 'provider-extension', files: { 'example.txt': 'left=1\nright=2\n' },
      edits: [{ file: 'example.txt', old: 'left=1', text: 'left=10' }, { file: 'example.txt', old: 'right=2', text: 'right=20' }],
      expected: { 'example.txt': 'left=10\nright=20\n' }, purpose: 'two neighboring edits through two official tool calls' },
    { name: 'disjoint-three-edits', cohort: 'provider-extension', files: { 'example.txt': 'a=1\nkeep\nb=2\nkeep\nc=3\n' },
      edits: [{ file: 'example.txt', old: 'a=1', text: 'a=10' }, { file: 'example.txt', old: 'b=2', text: 'b=20' }, { file: 'example.txt', old: 'c=3', text: 'c=30' }],
      expected: { 'example.txt': 'a=10\nkeep\nb=20\nkeep\nc=30\n' }, purpose: 'three separated edits with host observation refresh' },
    one('noop-lf', 'head\nmode=old\n', 'mode=old', 'mode=old', 'official edit argument parser rejects an identical replacement',
      { error: 'HOST_TOOL_ERROR', errorMessage: 'old_string and new_string must differ' }),
    one('reject-missing', 'head\nmode=old\n', 'missing', 'replacement', 'missing literal leaves storage intact',
      { error: 'FS_EDIT_NOT_FOUND', expected: { 'example.txt': 'head\nmode=old\n' } }),
    one('reject-ambiguous', 'old\nold\n', 'old', 'new', 'ambiguous literal leaves storage intact',
      { error: 'FS_AMBIGUOUS_EDIT', expected: { 'example.txt': 'old\nold\n' } }),
    one('reject-empty-search', 'head\n', '', 'new', 'empty search leaves storage intact',
      { error: 'HOST_TOOL_ERROR', errorMessage: 'old_string must be a non-empty string', expected: { 'example.txt': 'head\n' } }),
    one('reject-unobserved', 'old\n', 'old', 'new', 'official observation policy rejects edits before read',
      { observe: false, error: 'FS_NOT_OBSERVED', expected: { 'example.txt': 'old\n' } }),
    one('reject-stale', 'old\n', 'external', 'new', 'external update after read preserves external bytes',
      { external: { 'example.txt': 'external\n' }, error: 'FS_STALE_VERSION', expected: { 'example.txt': 'external\n' } }),
    one('reject-readonly', 'old\n', 'old', 'new', 'official read-only policy preserves storage',
      { mode: 'read-only', error: 'FS_SANDBOX_DENIED', expected: { 'example.txt': 'old\n' } }),
    { name: 'write-create', cohort: 'provider-extension', files: {}, edits: [],
      writes: [{ file: 'created.txt', text: 'created\r\n尾巴' }], expected: { 'created.txt': 'created\r\n尾巴' }, purpose: 'official write tool creates a file' },
    { name: 'write-overwrite-observed', cohort: 'provider-extension', files: { 'example.txt': 'old\r\n' }, edits: [],
      writes: [{ file: 'example.txt', text: 'new\r\n尾巴' }], expected: { 'example.txt': 'new\r\n尾巴' }, purpose: 'official write tool overwrites an observed file' },
    { name: 'write-noop-observed', cohort: 'provider-extension', files: { 'example.txt': 'same\r\n尾巴' }, edits: [],
      writes: [{ file: 'example.txt', text: 'same\r\n尾巴' }], expected: { 'example.txt': 'same\r\n尾巴' }, purpose: 'official write tool accepts identical observed content' },
  ];
  for (const size of [64 * 1024, 1024 * 1024, 5 * 1024 * 1024]) {
    const tail = '\nmode=old\n';
    const line = 'x'.repeat(127) + '\n';
    const filler = size - Buffer.byteLength(tail);
    const before = line.repeat(Math.floor(filler / line.length)) + 'x'.repeat(filler % line.length) + tail;
    extra.push(one(`large-${size}`, before, 'mode=old', 'mode=new', `one small edit in exactly ${size} bytes`));
  }
  return [...original, ...extra];
}

function bridgeCounter(runtime) {
  const total = zeroBridge();
  const editor = runtime.ctx.fs.editor;
  if (editor) {
    const write = editor.child.stdin.write.bind(editor.child.stdin);
    editor.child.stdin.write = (chunk, ...args) => { total.request_bytes += Buffer.byteLength(chunk); return write(chunk, ...args); };
    editor.child.stdout.on('data', chunk => { total.response_bytes += chunk.length; });
    const request = editor.request.bind(editor);
    editor.request = (action, ...args) => {
      total.requests++;
      total.actions[action] = (total.actions[action] ?? 0) + 1;
      return request(action, ...args);
    };
  }
  return {
    snapshot: () => structuredClone(total),
    delta(before) {
      return { requests: total.requests - before.requests, request_bytes: total.request_bytes - before.request_bytes,
        response_bytes: total.response_bytes - before.response_bytes,
        total_bytes: total.request_bytes - before.request_bytes + total.response_bytes - before.response_bytes,
        actions: Object.fromEntries(Object.entries(total.actions).map(([action, count]) => [action, count - (before.actions[action] ?? 0)]).filter(([, count]) => count)) };
    },
  };
}

async function putFiles(cwd, files) {
  for (const [name, value] of Object.entries(files)) {
    await fs.mkdir(path.dirname(path.join(cwd, name)), { recursive: true });
    await fs.writeFile(path.join(cwd, name), value);
  }
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

async function runSample(runtime, counter, scenario, cwd) {
  await fs.rm(cwd, { recursive: true, force: true });
  await fs.mkdir(cwd, { recursive: true });
  await putFiles(cwd, scenario.files);
  const startBridge = counter.snapshot();
  const calls = [];
  async function call(phase, name, args) {
    const started = performance.now();
    const result = await runtime.call(name, args);
    const elapsed_ms = performance.now() - started;
    calls.push({ phase, name, elapsed_ms, argument_bytes: bytes(args), result_bytes: bytes(result),
      is_error: result.isError, error_code: result.isError ? (JSON.stringify(result).match(/\bFS_[A-Z_]+\b/)?.[0] ?? 'HOST_TOOL_ERROR') : null,
      ...(result.isError ? { error_text: JSON.stringify(result).replaceAll(cwd, '<fixture>') } : {}) });
    return result;
  }
  if (scenario.observe !== false) for (const name of Object.keys(scenario.files)) {
    const result = await call('read', 'read', { file_path: path.join(cwd, name) });
    assert.equal(result.isError, false, `${scenario.name}: prerequisite read`);
  }
  if (scenario.external) await putFiles(cwd, scenario.external);
  for (const edit of scenario.edits) await call('mutation', 'edit', {
    file_path: path.join(cwd, edit.file), old_string: edit.old, new_string: edit.text,
    ...(edit.all ? { replace_all: true } : {}),
  });
  for (const write of scenario.writes ?? []) await call('mutation', 'write', { file_path: path.join(cwd, write.file), content: write.text });
  const bridge = counter.delta(startBridge);
  const outcomes = {};
  for (const [name, expected] of Object.entries(scenario.expected)) {
    const actual = await fs.readFile(path.join(cwd, name));
    outcomes[name] = { expected: fileInfo(expected), actual: fileInfo(actual), byte_exact: actual.equals(Buffer.from(expected)) };
  }
  const mutations = calls.filter(call => call.phase === 'mutation');
  const outcome_matches = mutations.every(call => scenario.error
    ? call.is_error && call.error_code === scenario.error && (!scenario.errorMessage || call.error_text.includes(scenario.errorMessage))
    : !call.is_error);
  const sum = field => calls.reduce((total, call) => total + call[field], 0);
  const unexpected_files = (await storedFiles(cwd)).filter(name => !Object.hasOwn(scenario.expected, name));
  const byte_exact = Object.values(outcomes).every(file => file.byte_exact) && unexpected_files.length === 0;
  return { read_ms: calls.filter(call => call.phase === 'read').reduce((total, call) => total + call.elapsed_ms, 0),
    mutation_ms: mutations.reduce((total, call) => total + call.elapsed_ms, 0), tool_ms: sum('elapsed_ms'),
    tool_calls: calls.length, argument_bytes: sum('argument_bytes'), result_bytes: sum('result_bytes'),
    tool_bytes: sum('argument_bytes') + sum('result_bytes'),
    bridge, calls, outcomes, unexpected_files, byte_exact, outcome_matches, correct: byte_exact && outcome_matches };
}

async function sourceProvenance() {
  const names = ['Cargo.toml', 'Cargo.lock', 'benches/native_provider.mjs', 'benches/edit_cost.py'];
  async function visit(directory) {
    for (const item of await fs.readdir(path.join(root, directory), { withFileTypes: true })) {
      const name = path.posix.join(directory, item.name);
      if (item.isDirectory()) await visit(name);
      else if (/\.(rs|mjs|json)$/.test(item.name)) names.push(name);
    }
  }
  await visit('src');
  await visit('integrations/native');
  await visit('integrations/dsh-native');
  const hashes = Object.fromEntries(await Promise.all(names.sort().map(async name => [name, hash(await fs.readFile(path.join(root, name)))])));
  const { stdout } = await exec('git', ['rev-parse', 'HEAD'], { cwd: root });
  return { git_head: stdout.trim(), working_tree_files_sha256: hashes, working_tree_sha256: hash(JSON.stringify(hashes)) };
}

export async function runBenchmark({ executable = path.join(root, 'target/release/xnip'),
  versions = ['0.2.1-rc.2', '0.2.1-rc.5'], repeats = 3, warmups = 1, scenarioNames,
  hostsDir = process.env.XNIP_DSH_NATIVE_HOSTS ?? path.join(root, 'target/native-hosts'),
  buildProfile = 'release', contentEncoding = 'negotiated', onProgress = () => {} } = {}) {
  assert.ok(Number.isInteger(repeats) && repeats > 0);
  assert.ok(Number.isInteger(warmups) && warmups >= 0);
  assert.ok(['negotiated', 'bytes'].includes(contentEncoding));
  executable = path.resolve(executable);
  const allScenarios = await scenarios();
  const selected = scenarioNames ? allScenarios.filter(item => scenarioNames.includes(item.name)) : allScenarios;
  assert.ok(selected.length, 'selected benchmark scenarios');
  if (scenarioNames) assert.equal(selected.length, new Set(scenarioNames).size, 'every selected scenario exists');
  const binary = await fs.readFile(executable);
  const { stdout: binaryVersion } = await exec(executable, ['--version'], { env: { ...process.env, XNIP_AUTO_UPDATE: '0' } });
  const report = { schema_version: 1, created_at: new Date().toISOString(),
    methodology: { host_tools: ['read', 'write', 'edit'], model_calls: 0,
      policy: 'official observation and sandbox services; prerequisite reads through the official read tool',
      known_inputs: 'fixtures supply replacement text; prerequisite reads retain the official default bounded output window',
      clock: 'performance.now monotonic milliseconds', p95: 'nearest rank',
      setup: 'one persistent provider per host/version/policy, including native process handshake; first host module import is cold',
      warmup_runs_per_scenario: warmups, measured_runs_per_scenario: repeats,
      timed_region: 'sum of actual tool execution durations; fixture reset, external updates and byte verification are outside',
      tool_bytes: 'UTF-8 JSON.stringify arguments and complete ToolRuntime result at the local tool boundary',
      bridge_bytes: 'actual native child stdin/stdout bytes during warm tool calls; handshake and process startup belong to setup',
      native_content_encoding: contentEncoding === 'bytes' ? 'legacy byte arrays selected after the normal capability handshake' : 'provider selects UTF-8 when advertised, otherwise legacy byte arrays',
      correctness: 'independent expected storage bytes, complete workspace file set and expected host error code/message; identical edit is rejected by official argument parser, identical write succeeds',
      ordering: 'host versions in supplied order; original then native per policy; scenarios in fixture order; persistent process warmed per scenario',
      batch_fixtures: 'historical batch fixture names are retained; each requested edit executes one official edit tool call',
      scope: 'local deterministic host/tool execution; model generation, billing and interactive approval latency are outside this measurement' },
    provenance: { node: process.version, node_executable: path.basename(process.execPath),
      platform: os.platform(), arch: os.arch(), kernel: os.release(), cpus: os.cpus().length,
      cpu_model: os.cpus()[0]?.model, binary: { filename: path.basename(executable), build_profile: buildProfile,
        version: binaryVersion.trim(), bytes: binary.length, sha256: hash(binary) },
      source: await sourceProvenance(), hosts: {} },
    scenarios: selected.map(item => ({ name: item.name, cohort: item.cohort, purpose: item.purpose,
      mode: item.mode ?? 'workspace-write', expected_error: item.error ?? null, expected_error_message: item.errorMessage ?? null,
      files: Object.fromEntries(Object.entries(item.files).map(([name, value]) => [name, fileInfo(value)])),
      expected: Object.fromEntries(Object.entries(item.expected).map(([name, value]) => [name, fileInfo(value)])),
      edit_calls: item.edits.length, write_calls: item.writes?.length ?? 0 })), setup: [], results: [] };
  const scratch = await fs.mkdtemp(path.join(root, 'target/native-provider-bench-'));
  try {
    for (const version of versions) {
      const hostRoot = path.resolve(hostsDir, 'dsh-' + version);
      const packageNames = ['dsh', 'dsh-fs-sandbox', 'dsh-fs-local', 'dsh-fs-observation-policy', 'dsh-tool-fs', 'dsh-tools', 'dsh-sandbox-policy'];
      report.provenance.hosts[version] = Object.fromEntries(await Promise.all(packageNames.map(async name => {
        const filename = path.join(hostRoot, 'node_modules/@deepseek-ai', name, 'package.json');
        const content = await fs.readFile(filename);
        const entry = await fs.readFile(path.join(hostRoot, 'node_modules/@deepseek-ai', name, name === 'dsh' ? 'lib/bin.js' : 'lib/index.js'));
        return ['@deepseek-ai/' + name, { version: JSON.parse(content).version,
          package_sha256: hash(content), entrypoint_sha256: hash(entry) }];
      })));
      for (const mode of [...new Set(selected.map(item => item.mode ?? 'workspace-write'))]) {
        let catalog;
        for (const native of [false, true]) {
          const provider = native ? 'xnip-native' : 'dsh-original';
          const cwd = path.join(scratch, version, mode);
          await fs.mkdir(cwd, { recursive: true });
          const started = performance.now();
          const runtime = await createFixture({ hostRoot, cwd, executable, native, mode });
          const setup_ms = performance.now() - started;
          if (native && contentEncoding === 'bytes') runtime.ctx.fs.utf8 = false;
          try {
            const description = await runtime.describe();
            assert.deepEqual(description.tools.map(item => item.name), ['read', 'write', 'edit']);
            if (catalog) assert.deepEqual(description, catalog, 'same host tools, schemas and system prompt');
            else catalog = description;
            report.setup.push({ host_version: version, provider, mode, setup_ms,
              native_content_encoding: native ? (runtime.ctx.fs.utf8 ? 'utf8' : 'bytes') : null,
              tool_schema_sha256: hash(JSON.stringify(description.tools)), prompt_sha256: hash(description.prompt) });
            const counter = bridgeCounter(runtime);
            for (const scenario of selected.filter(item => (item.mode ?? 'workspace-write') === mode)) {
              const samples = [];
              for (let iteration = 0; iteration < warmups + repeats; iteration++) {
                // A new path keeps the unobserved case independent of prior observations.
                const directory = path.join(cwd, scenario.name, String(iteration));
                const sample = await runSample(runtime, counter, scenario, directory);
                if (iteration >= warmups) samples.push(sample);
              }
              const summary = Object.fromEntries(sampleMeasures
                .map(field => [field, statistics(samples.map(sample => sample[field]))]));
              summary.bridge = Object.fromEntries(bridgeMeasures
                .map(field => [field, statistics(samples.map(sample => sample.bridge[field]))]));
              const row = { host_version: version, provider, mode, scenario: scenario.name, cohort: scenario.cohort,
                summary, byte_exact_runs: samples.filter(sample => sample.byte_exact).length,
                outcome_matches_runs: samples.filter(sample => sample.outcome_matches).length,
                correct_runs: samples.filter(sample => sample.correct).length, samples };
              report.results.push(row);
              onProgress(`${version} ${provider} ${scenario.name}: ${summary.mutation_ms.median.toFixed(2)} ms edit, ${row.correct_runs}/${repeats} correct`);
            }
          } finally { await runtime.close(); }
        }
      }
    }
  } finally { await fs.rm(scratch, { recursive: true, force: true }); }
  assert.equal(hash(await fs.readFile(executable)), report.provenance.binary.sha256, 'measured binary stayed fixed');
  assert.equal((await sourceProvenance()).working_tree_sha256, report.provenance.source.working_tree_sha256, 'measured source stayed fixed');
  validateReport(report);
  return report;
}

export function validateReport(report) {
  assert.equal(report.methodology.model_calls, 0);
  const versions = Object.keys(report.provenance.hosts);
  assert.equal(report.results.length, versions.length * report.scenarios.length * 2, 'complete host/provider/scenario matrix');
  assert.equal(new Set(report.results.map(row => `${row.host_version}/${row.provider}/${row.scenario}`)).size, report.results.length);
  for (const row of report.results) {
    const scenario = report.scenarios.find(item => item.name === row.scenario);
    assert.ok(scenario, 'row references a declared fixture');
    assert.equal(row.samples.length, report.methodology.measured_runs_per_scenario);
    for (const sample of row.samples) {
      assert.deepEqual(Object.keys(sample.outcomes).sort(), Object.keys(scenario.expected).sort(), 'every expected file has a byte outcome');
      for (const [name, outcome] of Object.entries(sample.outcomes)) assert.deepEqual(outcome.expected, scenario.expected[name], 'file outcome uses the declared oracle');
      assert.equal(sample.tool_calls, sample.calls.length);
      assert.ok(sample.calls.every(call => Number.isFinite(call.elapsed_ms) && call.elapsed_ms >= 0));
      assert.equal(sample.argument_bytes, sample.calls.reduce((total, call) => total + call.argument_bytes, 0));
      assert.equal(sample.result_bytes, sample.calls.reduce((total, call) => total + call.result_bytes, 0));
      assert.equal(sample.tool_bytes, sample.argument_bytes + sample.result_bytes);
      assert.equal(sample.bridge.total_bytes, sample.bridge.request_bytes + sample.bridge.response_bytes);
      assert.ok(Math.abs(sample.tool_ms - sample.read_ms - sample.mutation_ms) < 0.001);
      assert.equal(sample.correct, sample.byte_exact && sample.outcome_matches);
      assert.equal(sample.byte_exact, Object.values(sample.outcomes).every(outcome => outcome.byte_exact) && sample.unexpected_files.length === 0);
      for (const outcome of Object.values(sample.outcomes)) assert.equal(outcome.byte_exact,
        outcome.expected.bytes === outcome.actual.bytes && outcome.expected.sha256 === outcome.actual.sha256);
      if (row.provider === 'dsh-original') assert.deepEqual(sample.bridge, zeroBridge());
    }
    for (const field of sampleMeasures)
      assert.deepEqual(row.summary[field], statistics(row.samples.map(sample => sample[field])));
    for (const field of bridgeMeasures)
      assert.deepEqual(row.summary.bridge[field], statistics(row.samples.map(sample => sample.bridge[field])));
    for (const [field, sampleField] of [['byte_exact_runs', 'byte_exact'], ['outcome_matches_runs', 'outcome_matches'], ['correct_runs', 'correct']])
      assert.equal(row[field], row.samples.filter(sample => sample[sampleField]).length, `${field} derived from samples`);
    if (row.provider === 'xnip-native') assert.equal(row.correct_runs, row.samples.length, `${row.host_version}/${row.scenario}: native storage bytes and host outcome`);
  }
}

export function renderReport(report) {
  const runs = report.methodology.measured_runs_per_scenario;
  const lines = ['# Native filesystem provider benchmark', '',
    `Measured ${report.created_at}. ${report.scenarios.length} fixtures, ${runs} measured runs and ${report.methodology.warmup_runs_per_scenario} warm-up per provider/host/policy. Zero model calls.`, '',
    `Native bridge selection: ${report.methodology.native_content_encoding}.`, '',
    'Both lanes execute the official DSH `read`, `write` and `edit` tools, observation policy and sandbox policy. The original lane uses its packaged filesystem; the xnip lane replaces that provider. Tools, schemas and prompts are checked equal. This cohort measures local execution and storage bytes.', '',
    'The original 13 fixture names are retained. Batch fixtures run one official tool call per edit, matching the native tool API; their results represent a sequence of edits.', '',
    'Fixtures supply known replacement text. Prerequisite reads retain the official bounded output window, including for edits near the end of large files.', '',
    'Fixture reset and byte verification run outside timing. Read and mutation times are summed tool execution durations on a persistent host and native process. Setup includes provider construction and the native handshake; the first official module import is cold. Median and nearest-rank p95 use the measured samples; with three runs, p95 is the maximum.', '',
    'The byte oracle preserves original UTF-8 BOM, CRLF, mixed line endings and EOF bytes outside the requested edit. A host-normalized result appears as a byte mismatch even when the tool reports success. Rejection cases must return the expected host error and preserve the expected storage bytes. Timing comparisons should be read alongside correctness.', '',
    `Binary: ${report.provenance.binary.version}, ${report.provenance.binary.build_profile}, SHA-256 \`${report.provenance.binary.sha256}\`. Node ${report.provenance.node}; ${report.provenance.platform}/${report.provenance.arch}; ${report.provenance.cpu_model}. Source digest \`${report.provenance.source.working_tree_sha256}\`. Complete file and package fingerprints are in the JSON companion.`, '',
    '## Host setup', '', '| DSH | Policy | Provider | Setup ms |', '|---|---|---|---:|'];
  for (const setup of report.setup) lines.push(`| ${setup.host_version} | ${setup.mode} | ${setup.provider} | ${setup.setup_ms.toFixed(2)} |`);
  for (const version of Object.keys(report.provenance.hosts)) {
    lines.push('', `## DSH ${version}`, '',
      'Times are mutation median / p95 in milliseconds; read median is shown separately. Bridge bytes count actual native stdin + stdout during tool execution. Tool bytes count serialized arguments + complete local results; they describe this API boundary.', '',
      '| Fixture | Original mutation ms | xnip mutation ms | Original / xnip read ms | Original / xnip correct | xnip bridge bytes | Original / xnip tool bytes |',
      '|---|---:|---:|---:|---:|---:|---:|');
    for (const scenario of report.scenarios) {
      const old = report.results.find(row => row.host_version === version && row.scenario === scenario.name && row.provider === 'dsh-original');
      const next = report.results.find(row => row.host_version === version && row.scenario === scenario.name && row.provider === 'xnip-native');
      const time = row => `${row.summary.mutation_ms.median.toFixed(2)} / ${row.summary.mutation_ms.p95.toFixed(2)}`;
      const toolBytes = row => Math.round(row.summary.tool_bytes.median).toLocaleString('en-US');
      lines.push(`| ${scenario.name} | ${time(old)} | ${time(next)} | ${old.summary.read_ms.median.toFixed(2)} / ${next.summary.read_ms.median.toFixed(2)} | ${old.correct_runs}/${runs} / ${next.correct_runs}/${runs} | ${Math.round(next.summary.bridge.total_bytes.median).toLocaleString('en-US')} | ${toolBytes(old)} / ${toolBytes(next)} |`);
    }
    const mismatches = report.results.filter(row => row.host_version === version && row.correct_runs < runs);
    lines.push('', `Storage/outcome differences: ${mismatches.length ? mismatches.map(row => `\`${row.provider}/${row.scenario}\` (${row.byte_exact_runs}/${runs} byte exact; ${row.outcome_matches_runs}/${runs} expected outcome)`).join(', ') : 'all measured results match the byte and outcome oracle'}.`);
  }
  lines.push('', '## Reproduce', '', '```sh', 'node benches/native/dsh_native_setup.mjs',
    'CARGO_BUILD_JOBS=4 cargo build --release --locked --offline',
    'node benches/native_provider.mjs --executable target/release/xnip --repeats 3 --warmups 1',
    'node --test-reporter=tap tests/native_provider_bench_contract.mjs', '```', '',
    'The setup helper installs official host packages under `target/native-hosts/dsh-0.2.1-rc.2` and `dsh-0.2.1-rc.5`. The benchmark uses the Python standard library to load the unchanged original 13 fixtures. The runner verifies that measured source and executable hashes stay fixed throughout execution.', '',
    'The separate [bridge encoding control](native-provider-encoding.md) compares legacy arrays with negotiated UTF-8 using the same executable and source.', '',
    'Historical representation, 13-fixture and 41-profile reports retain their own cohorts and measurements. This report adds actual native-provider execution. Raw samples include per-call timings and errors, complete byte counts, file hashes, package versions and fingerprints. Scheduling, page cache, warm-up order and a small sample count affect latency; fresh-machine repetition is useful for wider performance conclusions.', '');
  return lines.join('\n');
}

export function renderEncodingComparison(legacy, utf8) {
  assert.equal(legacy.provenance.binary.sha256, utf8.provenance.binary.sha256, 'encoding control uses the same executable');
  assert.equal(legacy.provenance.source.working_tree_sha256, utf8.provenance.source.working_tree_sha256, 'encoding control uses the same source');
  assert.deepEqual(legacy.provenance.hosts, utf8.provenance.hosts, 'encoding control uses the same host packages');
  assert.deepEqual(legacy.scenarios, utf8.scenarios, 'encoding control uses the same fixtures');
  assert.equal(legacy.methodology.measured_runs_per_scenario, utf8.methodology.measured_runs_per_scenario);
  assert.ok(legacy.setup.filter(item => item.provider === 'xnip-native').every(item => item.native_content_encoding === 'bytes'));
  assert.ok(utf8.setup.filter(item => item.provider === 'xnip-native').every(item => item.native_content_encoding === 'utf8'));
  const lines = ['# Native bridge encoding control', '',
    'Both cohorts execute the same official DSH tools, policies, fixtures, xnip executable and source. The control completes the ordinary capability handshake, then selects the supported legacy byte-array path. The normal cohort negotiates UTF-8. One persistent process serves each provider/version; every fixture has its own warm-up. The legacy cohort runs first.', '',
    'This isolates the bridge representation choice within the measured local workflow. Returned file snapshots and exact guards retain their byte oracle. Tool and filesystem work remain part of the timed operation. Median and nearest-rank p95 use measured samples; three runs make p95 the maximum. Scheduling, GC and a small sample count affect the magnitude of latency differences.', '',
    `Binary SHA-256: \`${utf8.provenance.binary.sha256}\` (${utf8.provenance.binary.build_profile}). Source SHA-256: \`${utf8.provenance.source.working_tree_sha256}\`. ${utf8.methodology.measured_runs_per_scenario} measured runs, ${utf8.methodology.warmup_runs_per_scenario} warm-up per fixture. Raw JSON retains both cohorts and their original-provider reference lanes.`, '',
    '| DSH | Fixture | Legacy mutation median / p95 ms | UTF-8 mutation median / p95 ms | Legacy / UTF-8 bridge bytes | Legacy / UTF-8 correct |',
    '|---|---|---:|---:|---:|---:|'];
  for (const next of utf8.results.filter(row => row.provider === 'xnip-native')) {
    const before = legacy.results.find(row => row.provider === next.provider && row.host_version === next.host_version && row.scenario === next.scenario);
    assert.ok(before);
    const timing = row => `${row.summary.mutation_ms.median.toFixed(2)} / ${row.summary.mutation_ms.p95.toFixed(2)}`;
    const byteCount = row => Math.round(row.summary.bridge.total_bytes.median).toLocaleString('en-US');
    lines.push(`| ${next.host_version} | ${next.scenario} | ${timing(before)} | ${timing(next)} | ${byteCount(before)} / ${byteCount(next)} | ${before.correct_runs}/${before.samples.length} / ${next.correct_runs}/${next.samples.length} |`);
  }
  lines.push('', '## Reproduce', '', '```sh',
    'node benches/native_provider.mjs --compare-encodings --executable target/release/xnip --repeats 3 --warmups 1',
    '```', '',
    'See the [full provider matrix](native-provider-results.md) for the 32-fixture cohort, methodology and setup. The JSON companion contains read timings, local tool bytes, native request/response bytes, error outcomes and per-file expected/actual hashes. Zero model calls.', '',
    'The [earlier debug observations](native-provider-debug-history.json) retain optimization-before/after samples with their distinct executable hashes. The release control above holds the source and executable fixed; those early observations have a narrower provenance record.', '');
  return lines.join('\n');
}

async function main() {
  const options = {};
  let outputPrefix = path.join(root, 'docs/benchmarks/native-provider-results');
  let compareEncodings = false;
  let explicitOutput = false;
  for (let i = 2; i < process.argv.length; i++) {
    const name = process.argv[i];
    if (name === '--executable') options.executable = process.argv[++i];
    else if (name === '--hosts-dir') options.hostsDir = process.argv[++i];
    else if (name === '--repeats') options.repeats = Number(process.argv[++i]);
    else if (name === '--warmups') options.warmups = Number(process.argv[++i]);
    else if (name === '--versions') options.versions = process.argv[++i].split(',');
    else if (name === '--scenarios') options.scenarioNames = process.argv[++i].split(',');
    else if (name === '--build-profile') options.buildProfile = process.argv[++i];
    else if (name === '--content-encoding') options.contentEncoding = process.argv[++i];
    else if (name === '--compare-encodings') compareEncodings = true;
    else if (name === '--output-prefix') { outputPrefix = process.argv[++i]; explicitOutput = true; }
    else throw new Error(`Unknown argument ${name}`);
  }
  options.onProgress = line => process.stderr.write(line + '\n');
  if (compareEncodings) {
    if (!explicitOutput) outputPrefix = path.join(root, 'docs/benchmarks/native-provider-encoding');
    options.scenarioNames ??= ['large-1048576', 'large-5242880'];
    const legacy = await runBenchmark({ ...options, contentEncoding: 'bytes' });
    const utf8 = await runBenchmark({ ...options, contentEncoding: 'negotiated' });
    const markdown = renderEncodingComparison(legacy, utf8);
    await fs.mkdir(path.dirname(outputPrefix), { recursive: true });
    await fs.writeFile(outputPrefix + '.json', JSON.stringify({ schema_version: 1, legacy, utf8 }, null, 2) + '\n');
    await fs.writeFile(outputPrefix + '.md', markdown);
    console.log(`Encoding control written to ${path.relative(root, outputPrefix)}.{json,md}`);
    return;
  }
  const report = await runBenchmark(options);
  await fs.mkdir(path.dirname(outputPrefix), { recursive: true });
  await fs.writeFile(outputPrefix + '.json', JSON.stringify(report, null, 2) + '\n');
  await fs.writeFile(outputPrefix + '.md', renderReport(report));
  console.log(`${report.results.length} measured groups written to ${path.relative(root, outputPrefix)}.{json,md}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
