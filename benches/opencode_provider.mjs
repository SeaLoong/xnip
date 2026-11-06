#!/usr/bin/env node
/** Real OpenCode CLI tool dispatch, original editor and explicit xnip plugin. */
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import os from 'node:os';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';
import { gzipSync, gunzipSync } from 'node:zlib';
import { scenarios, statistics, root } from './native_provider.mjs';
import { createFixture, VERSION } from '../tests/opencode_native_runtime.mjs';

const hash = (value) => crypto.createHash('sha256').update(value).digest('hex');
const info = (value) => ({ bytes: Buffer.byteLength(value), sha256: hash(value) });
const metrics = ['tool_ms', 'tool_calls', 'argument_bytes', 'result_bytes', 'tool_bytes'];
const knownErrors = {
  'missing-guard': {
    'opencode-original': 'Could not find oldString in the file.',
    'xnip-native': 'old text was not found',
  },
  'ambiguous-guard': {
    'opencode-original': 'Found multiple matches for oldString.',
    'xnip-native': 'old text is ambiguous',
  },
  'identical-edit': { 'opencode-original': 'oldString and newString are identical.' },
  'policy-denied': {
    'opencode-original': 'Tool edit is disabled for agent build',
    'xnip-native':
      'The user has specified a rule which prevents you from using this specific tool call.',
  },
};
function matchesOutcome(call, expectedError) {
  return expectedError
    ? call.ok === false && call.code === 1 && call.error.includes(expectedError)
    : call.ok === true;
}
function capturedPayload(payload) {
  const json = JSON.stringify(payload);
  return Buffer.byteLength(json) > 65536
    ? { encoding: 'gzip+base64', data: gzipSync(json).toString('base64') }
    : { encoding: 'json', value: payload };
}
function payloadValue(capture) {
  assert.ok(['json', 'gzip+base64'].includes(capture.encoding));
  return capture.encoding === 'json'
    ? capture.value
    : JSON.parse(gunzipSync(Buffer.from(capture.data, 'base64')).toString('utf8'));
}

export async function opencodeScenarios() {
  const base = (await scenarios()).filter(
    (item) => item.cohort === 'original13' || /^large-\d+$/.test(item.name),
  );
  const one = (name, before, old, text, expected, extra = {}) => ({
    name,
    cohort: 'opencode-plugin',
    files: { 'example.txt': before },
    edits: [{ file: 'example.txt', old, text }],
    expected: { 'example.txt': expected },
    ...extra,
  });
  return [
    ...base,
    one('bom-crlf', '\ufeffhead\r\nold\r\n尾巴', 'old', 'new', '\ufeffhead\r\nnew\r\n尾巴'),
    one('missing-guard', 'old\n', 'missing', 'new', 'old\n', {
      errors: { ...knownErrors['missing-guard'] },
    }),
    one('ambiguous-guard', 'old old\n', 'old', 'new', 'old old\n', {
      errors: { ...knownErrors['ambiguous-guard'] },
    }),
    one('identical-edit', 'old\n', 'old', 'old', 'old\n', {
      errors: { ...knownErrors['identical-edit'] },
    }),
    one('policy-denied', 'old\n', 'old', 'new', 'old\n', {
      permission: { edit: 'deny' },
      errors: { ...knownErrors['policy-denied'] },
    }),
    {
      name: 'write-create',
      cohort: 'opencode-plugin',
      files: {},
      edits: [],
      writes: [{ file: 'created.txt', text: 'created\r\n尾巴' }],
      expected: { 'created.txt': 'created\r\n尾巴' },
    },
    {
      name: 'patch-multifile',
      cohort: 'opencode-plugin',
      files: { 'a.txt': 'old\n', 'b.txt': 'second\n' },
      edits: [],
      patches: [
        { file: 'a.txt', old: 'old', text: 'new' },
        { file: 'b.txt', old: 'second', text: 'updated' },
      ],
      expected: { 'a.txt': 'new\n', 'b.txt': 'updated\n' },
    },
  ];
}
async function sourceProvenance() {
  const names = [
    'Cargo.toml',
    'Cargo.lock',
    'integrations/catalog.json',
    'benches/opencode_provider.mjs',
    'benches/native_provider.mjs',
    'benches/edit_cost.py',
    'benches/native/opencode_native_setup.mjs',
    'benches/native/node_runtime.mjs',
    'tests/opencode_native_runtime.mjs',
  ];
  async function visit(dir) {
    for (const entry of await fs.readdir(path.join(root, dir), { withFileTypes: true })) {
      const name = path.posix.join(dir, entry.name);
      if (entry.isDirectory()) await visit(name);
      else if (/\.(rs|mjs|json)$/.test(entry.name)) names.push(name);
    }
  }
  for (const dir of [
    'src',
    'integrations/native',
    'integrations/opencode',
    'benches/native/opencode_native',
  ])
    await visit(dir);
  const files = Object.fromEntries(
    await Promise.all(
      names.sort().map(async (name) => [name, hash(await fs.readFile(path.join(root, name)))]),
    ),
  );
  return { sha256: hash(JSON.stringify(files)), files_sha256: files };
}
async function filesAt(cwd, relative = '') {
  const names = [];
  for (const entry of await fs.readdir(path.join(cwd, relative), { withFileTypes: true })) {
    const name = path.posix.join(relative, entry.name);
    if (entry.isDirectory()) names.push(...(await filesAt(cwd, name)));
    else names.push(name);
  }
  return names.sort();
}
async function sample(runtime, fixture, directory, provider) {
  await fs.rm(directory, { recursive: true, force: true });
  await fs.mkdir(directory, { recursive: true });
  for (const [name, content] of Object.entries(fixture.files)) {
    await fs.mkdir(path.dirname(path.join(directory, name)), { recursive: true });
    await fs.writeFile(path.join(directory, name), content);
  }
  const calls = [];
  async function call(name, args) {
    const started = performance.now();
    let result;
    try {
      result = await runtime.call(name, args);
    } catch (cause) {
      throw new Error(`${provider}/${fixture.name}/${name}: CLI infrastructure failure`, { cause });
    }
    const elapsed_ms = performance.now() - started;
    const payload = result.ok ? result.value : { code: result.code, error: result.error };
    const argsJson = JSON.stringify(args),
      resultJson = JSON.stringify(payload);
    calls.push({
      name,
      arguments: args,
      result_payload: capturedPayload(payload),
      stdout_bytes: result.stdout_bytes,
      stderr_bytes: result.stderr_bytes,
      elapsed_ms,
      argument_bytes: Buffer.byteLength(argsJson),
      result_bytes: Buffer.byteLength(resultJson),
      argument_sha256: hash(argsJson),
      result_sha256: hash(resultJson),
      ok: result.ok,
      ...(result.ok
        ? { output: result.value.result.output }
        : { error: result.error, code: result.code }),
    });
    return result;
  }
  for (const edit of fixture.edits)
    await call(provider === 'xnip-native' ? 'xnip_edit' : 'edit', {
      filePath: path.join(directory, edit.file),
      oldString: edit.old,
      newString: edit.text,
      replaceAll: edit.all ?? false,
    });
  for (const write of fixture.writes ?? [])
    await call(provider === 'xnip-native' ? 'xnip_write' : 'write', {
      filePath: path.join(directory, write.file),
      content: write.text,
    });
  if (fixture.patches) {
    if (provider === 'xnip-native') {
      const parts = ['*** Begin Patch'];
      for (const edit of fixture.patches)
        parts.push(
          '*** Update File: ' + path.relative(runtime.cwd, path.join(directory, edit.file)),
          '@@',
          '-' + edit.old,
          '+' + edit.text,
        );
      parts.push('*** End Patch');
      await call('xnip_apply', { patch: parts.join('\n') });
    } else
      for (const edit of fixture.patches)
        await call('edit', {
          filePath: path.join(directory, edit.file),
          oldString: edit.old,
          newString: edit.text,
        });
  }
  const outcomes = {};
  for (const [name, expected] of Object.entries(fixture.expected)) {
    const actual = await fs.readFile(path.join(directory, name));
    outcomes[name] = {
      expected: info(expected),
      actual: info(actual),
      byte_exact: actual.equals(Buffer.from(expected)),
    };
  }
  const unexpected_files = (await filesAt(directory)).filter(
    (name) => !Object.hasOwn(fixture.expected, name),
  );
  const byte_exact =
    Object.values(outcomes).every((value) => value.byte_exact) && unexpected_files.length === 0;
  const outcome_matches = calls.every((call) => matchesOutcome(call, fixture.errors?.[provider]));
  const sum = (field) => calls.reduce((total, call) => total + call[field], 0);
  assert.equal(runtime.modelCalls, 0);
  return {
    tool_ms: sum('elapsed_ms'),
    tool_calls: calls.length,
    argument_bytes: sum('argument_bytes'),
    result_bytes: sum('result_bytes'),
    tool_bytes: sum('argument_bytes') + sum('result_bytes'),
    calls,
    outcomes,
    unexpected_files,
    byte_exact,
    outcome_matches,
    correct: byte_exact && outcome_matches,
    model_calls: runtime.modelCalls,
  };
}
function expectedCalls(fixture, provider, workspace, iteration, platform) {
  const paths = platform === 'win32' ? path.win32 : path.posix;
  const directory = paths.join(workspace, fixture.name, String(iteration));
  const planned = [];
  for (const edit of fixture.edits) {
    planned.push({
      name: provider === 'xnip-native' ? 'xnip_edit' : 'edit',
      arguments: {
        filePath: paths.join(directory, edit.file),
        oldString: edit.old,
        newString: edit.text,
        replaceAll: edit.all ?? false,
      },
    });
  }
  for (const write of fixture.writes) {
    planned.push({
      name: provider === 'xnip-native' ? 'xnip_write' : 'write',
      arguments: {
        filePath: paths.join(directory, write.file),
        content: write.text,
      },
    });
  }
  if (fixture.patches.length) {
    if (provider === 'xnip-native') {
      const lines = ['*** Begin Patch'];
      for (const edit of fixture.patches) {
        const file = paths.relative(workspace, paths.join(directory, edit.file));
        lines.push(`*** Update File: ${file}`, '@@', `-${edit.old}`, `+${edit.text}`);
      }
      lines.push('*** End Patch');
      planned.push({ name: 'xnip_apply', arguments: { patch: lines.join('\n') } });
    } else {
      for (const edit of fixture.patches) {
        planned.push({
          name: 'edit',
          arguments: {
            filePath: paths.join(directory, edit.file),
            oldString: edit.old,
            newString: edit.text,
          },
        });
      }
    }
  }
  return planned;
}
export function validateOpenCodeReport(report) {
  assert.equal(report.schema_version, 2);
  assert.equal(report.results.length, report.scenarios.length * 2);
  assert.equal(
    new Set(report.results.map((row) => row.provider + '/' + row.scenario)).size,
    report.results.length,
  );
  for (const row of report.results) {
    assert.ok(['opencode-original', 'xnip-native'].includes(row.provider));
    const fixture = report.scenarios.find((value) => value.name === row.scenario);
    assert.ok(fixture);
    assert.deepEqual(
      fixture.errors,
      knownErrors[fixture.name] ?? {},
      `${row.scenario}: expected public error reasons`,
    );
    assert.equal(row.samples.length, report.methodology.measured_runs);
    assert.equal(row.warmups.length, report.methodology.warmups);
    const workflows = [
      ...row.warmups.map((sample, index) => [sample, index]),
      ...row.samples.map((sample, index) => [sample, index + report.methodology.warmups]),
    ];
    for (const [sample, iteration] of workflows) {
      assert.equal(sample.model_calls, 0);
      assert.equal(sample.tool_calls, sample.calls.length);
      assert.ok(sample.calls.length > 0, 'Every workflow executes an actual tool');
      assert.deepEqual(
        sample.calls.map((call) => ({ name: call.name, arguments: call.arguments })),
        expectedCalls(
          fixture,
          row.provider,
          report.provenance.workspace,
          iteration,
          report.provenance.platform,
        ),
        `${row.provider}/${row.scenario}: planned tool sequence and arguments`,
      );
      for (const call of sample.calls) {
        const argsJson = JSON.stringify(call.arguments),
          payload = payloadValue(call.result_payload);
        const resultJson = JSON.stringify(payload);
        assert.equal(call.argument_bytes, Buffer.byteLength(argsJson));
        assert.equal(call.argument_sha256, hash(argsJson));
        assert.equal(call.result_bytes, Buffer.byteLength(resultJson));
        assert.equal(call.result_sha256, hash(resultJson));
        if (call.ok) {
          assert.equal(payload.tool, call.name);
          assert.deepEqual(payload.input, call.arguments);
          const receipt = call.name.startsWith('xnip_')
            ? row.scenario === 'identical-edit'
              ? 'unchanged'
              : 'written'
            : call.name === 'write'
              ? 'Wrote file successfully.'
              : 'Edit applied successfully.';
          assert.equal(
            typeof payload.result?.output,
            'string',
            `${row.provider}/${row.scenario}: known tool success receipt`,
          );
          assert.equal(
            payload.result.output.trim(),
            receipt,
            `${row.provider}/${row.scenario}: known tool success receipt`,
          );
          assert.equal(call.output, payload.result.output);
        } else {
          assert.equal(call.code, payload.code);
          assert.equal(call.error, payload.error);
        }
      }

      assert.deepEqual(Object.keys(sample.outcomes).sort(), Object.keys(fixture.expected).sort());
      for (const [name, outcome] of Object.entries(sample.outcomes)) {
        assert.deepEqual(outcome.expected, fixture.expected[name]);
        assert.equal(
          outcome.byte_exact,
          outcome.expected.bytes === outcome.actual.bytes &&
            outcome.expected.sha256 === outcome.actual.sha256,
        );
      }
      assert.equal(
        sample.byte_exact,
        Object.values(sample.outcomes).every((value) => value.byte_exact) &&
          sample.unexpected_files.length === 0,
      );
      assert.equal(
        sample.outcome_matches,
        sample.calls.every((call) => matchesOutcome(call, fixture.errors?.[row.provider])),
        `${row.provider}/${row.scenario}: public outcome reason`,
      );
      assert.equal(sample.correct, sample.byte_exact && sample.outcome_matches);
      assert.equal(
        sample.outcome_matches,
        true,
        `${row.provider}/${row.scenario}: public outcome reason`,
      );
      assert.equal(sample.byte_exact, true, `${row.provider}/${row.scenario}: exact byte goal`);
      assert.equal(
        sample.argument_bytes,
        sample.calls.reduce((sum, call) => sum + call.argument_bytes, 0),
      );
      assert.equal(
        sample.result_bytes,
        sample.calls.reduce((sum, call) => sum + call.result_bytes, 0),
      );
      assert.equal(sample.tool_bytes, sample.argument_bytes + sample.result_bytes);
      assert.ok(
        Math.abs(sample.tool_ms - sample.calls.reduce((sum, call) => sum + call.elapsed_ms, 0)) <
          0.001,
      );
    }
    for (const field of metrics)
      assert.deepEqual(row.summary[field], statistics(row.samples.map((sample) => sample[field])));
    assert.equal(row.correct_runs, row.samples.filter((sample) => sample.correct).length);
    assert.equal(
      row.outcome_matches_runs,
      row.samples.filter((sample) => sample.outcome_matches).length,
    );
    assert.equal(
      row.outcome_matches_runs,
      row.samples.length,
      `${row.provider}/${row.scenario}: expected host result`,
    );
    assert.equal(
      row.correct_runs,
      row.samples.length,
      `${row.provider}/${row.scenario}: exact byte goal`,
    );
  }
  return report;
}
export async function runOpenCodeBenchmark({
  executable = path.join(root, 'target/release/xnip'),
  repeats = 3,
  warmups = 1,
  scenarioNames,
  onProgress = () => {},
  buildProfile = 'release',
} = {}) {
  assert.ok(Number.isInteger(repeats) && repeats > 0, 'repeats must be a positive integer');
  assert.ok(Number.isInteger(warmups) && warmups >= 0, 'warmups must be a nonnegative integer');
  const all = await opencodeScenarios(),
    selected = scenarioNames ? all.filter((value) => scenarioNames.includes(value.name)) : all;
  assert.ok(selected.length);
  const source = await sourceProvenance(),
    binary = await fs.readFile(executable);
  const cwd = await fs.mkdtemp(path.join(root, 'target/opencode-provider-bench-'));
  const report = {
    schema_version: 2,
    created_at: new Date().toISOString(),
    methodology: {
      measured_runs: repeats,
      warmups,
      timing:
        'Full CLI invocation through runtime.call, including argument serialization, process startup/shutdown, stdout file capture/read and JSON decoding; each call launches a fresh OpenCode process. Fixture setup, benchmark payload archival/byte accounting and file-goal verification are outside timing.',
      comparison:
        'Independent exact byte goals for both providers, known success receipts and public guard/permission error reasons; custom xnip tools retain exact matching while original edit retains fuzzy/EOL behavior. Formatters/LSP disabled in both lanes.',
      model_scope:
        'One configured local inert provider; HTTP trap counts actual model endpoint requests. Official debug source executes tools without a model loop; interactive ASK and session before/after hooks lie outside this entry.',
    },
    provenance: {
      binary: { path: executable, sha256: hash(binary), build_profile: buildProfile },
      workspace: cwd,
      source,
      node: process.version,
      platform: process.platform,
      arch: process.arch,
      cpu: os.cpus()[0].model,
    },
    scenarios: selected.map((value) => ({
      name: value.name,
      cohort: value.cohort,
      purpose: value.purpose ?? value.name,
      expected: Object.fromEntries(
        Object.entries(value.expected).map(([name, text]) => [name, info(text)]),
      ),
      errors: value.errors ?? {},
      edits: structuredClone(value.edits ?? []),
      writes: structuredClone(value.writes ?? []),
      patches: structuredClone(value.patches ?? []),
    })),
    setup: [],
    results: [],
  };
  try {
    for (const native of [false, true]) {
      const provider = native ? 'xnip-native' : 'opencode-original';
      let runtime;
      try {
        for (const permission of [false, true]) {
          const started = performance.now();
          runtime = await createFixture({
            cwd,
            executable,
            native,
            permission: permission ? { edit: 'deny' } : {},
          });
          const catalog = await runtime.describe();
          report.provenance.host ??= runtime.provenance;
          assert.deepEqual(runtime.provenance, report.provenance.host);
          if (native)
            for (const name of ['xnip_edit', 'xnip_write', 'xnip_apply', 'xnip_read'])
              assert.equal(catalog.tools[name], true);
          report.setup.push({
            provider,
            permission: permission ? 'deny' : 'allow',
            setup_ms: performance.now() - started,
            catalog_sha256: hash(JSON.stringify(catalog)),
          });
          for (const fixture of selected.filter(
            (value) => Boolean(value.permission) === permission,
          )) {
            const samples = [],
              warmupResults = [];
            for (let index = 0; index < warmups + repeats; index++) {
              const result = await sample(
                runtime,
                fixture,
                path.join(cwd, fixture.name, String(index)),
                provider,
              );
              (index < warmups ? warmupResults : samples).push(result);
            }
            const summary = Object.fromEntries(
              metrics.map((field) => [field, statistics(samples.map((value) => value[field]))]),
            );
            report.results.push({
              provider,
              scenario: fixture.name,
              summary,
              correct_runs: samples.filter((value) => value.correct).length,
              outcome_matches_runs: samples.filter((value) => value.outcome_matches).length,
              samples,
              warmups: warmupResults,
            });
            onProgress(
              provider +
                ' ' +
                fixture.name +
                ': ' +
                summary.tool_ms.median.toFixed(2) +
                ' ms; ' +
                samples.filter((sample) => sample.outcome_matches).length +
                '/' +
                samples.length +
                ' public outcomes',
            );
          }
          await runtime.close();
          runtime = undefined;
        }
      } finally {
        await runtime?.close();
      }
    }
  } finally {
    await fs.rm(cwd, { recursive: true, force: true });
  }
  assert.equal(hash(await fs.readFile(executable)), hash(binary));
  assert.equal((await sourceProvenance()).sha256, source.sha256);
  return validateOpenCodeReport(report);
}
export function renderOpenCodeReport(report) {
  const out = [
    '# OpenCode native plugin benchmark',
    '',
    `OpenCode ${report.provenance.host.version}; ${report.scenarios.length} scenarios, ${report.methodology.measured_runs} measured runs and ${report.methodology.warmups} warm-up per provider/scenario. Zero actual model endpoint requests.`,
    '',
    report.methodology.timing,
    '',
    report.methodology.comparison,
    '',
    report.methodology.model_scope,
    '',
    `Binary SHA-256 \`${report.provenance.binary.sha256}\`; source \`${report.provenance.source.sha256}\`. Raw JSON retains source/package fingerprints, actual arguments/results, recomputable digests, public errors and complete file oracles. Large result payloads use lossless gzip+base64 encoding.`,
    '',
    'The plugin adds explicit xnip tools. OpenCode builtins retain their own result/UI, formatting and fuzzy matching behavior. The multi-file patch fixture uses one xnip_apply call versus separate original edit calls; tool counts remain visible. This is tool-only CLI execution, not agent generation or billed tokens. Three measured runs make nearest-rank p95 the maximum.',
    '',
    '| Scenario | Original median / p95 ms | xnip median / p95 ms | Original / xnip calls | Original / xnip goal+outcome |',
    '|---|---:|---:|---:|---:|',
  ];
  for (const fixture of report.scenarios) {
    const a = report.results.find(
        (row) => row.provider === 'opencode-original' && row.scenario === fixture.name,
      ),
      b = report.results.find(
        (row) => row.provider === 'xnip-native' && row.scenario === fixture.name,
      );
    out.push(
      `| ${fixture.name} | ${a.summary.tool_ms.median.toFixed(2)} / ${a.summary.tool_ms.p95.toFixed(2)} | ${b.summary.tool_ms.median.toFixed(2)} / ${b.summary.tool_ms.p95.toFixed(2)} | ${a.summary.tool_calls.median} / ${b.summary.tool_calls.median} | ${a.correct_runs}/${a.samples.length} / ${b.correct_runs}/${b.samples.length} |`,
    );
  }
  out.push(
    '',
    '## Reproduce',
    '',
    '```sh',
    'node benches/native/opencode_native_setup.mjs',
    'node benches/opencode_provider.mjs --executable target/release/xnip --repeats 3 --warmups 1',
    '```',
    '',
    '[Integration and verification scope](../opencode.md).',
  );
  return out.join('\n') + '\n';
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { parseArgs } = await import('node:util');
  const { values } = parseArgs({
    options: {
      executable: { type: 'string' },
      repeats: { type: 'string', default: '3' },
      warmups: { type: 'string', default: '1' },
      scenarios: { type: 'string' },
      'output-prefix': { type: 'string', default: 'docs/benchmarks/opencode-provider-results' },
      help: { type: 'boolean', default: false },
    },
  });
  if (values.help)
    console.log(
      'Usage: node benches/opencode_provider.mjs [--executable PATH] [--repeats N] [--warmups N] [--scenarios NAME,...] [--output-prefix PATH]',
    );
  else {
    const report = await runOpenCodeBenchmark({
      executable: values.executable,
      repeats: Number(values.repeats),
      warmups: Number(values.warmups),
      scenarioNames: values.scenarios?.split(','),
      onProgress: (message) => process.stderr.write(message + '\n'),
    });
    await fs.mkdir(path.dirname(values['output-prefix']), { recursive: true });
    await fs.writeFile(values['output-prefix'] + '.json', JSON.stringify(report, null, 2) + '\n');
    await fs.writeFile(values['output-prefix'] + '.md', renderOpenCodeReport(report));
  }
}
