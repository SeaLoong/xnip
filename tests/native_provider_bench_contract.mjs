import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { runBenchmark, renderReport, renderEncodingComparison, validateReport } from '../benches/native_provider.mjs';

const root = path.resolve(import.meta.dirname, '..');
const versions = (process.env.XNIP_DSH_BENCH_VERSIONS ?? '0.2.1-rc.2,0.2.1-rc.5').split(',');
const executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip');

for (const version of versions) {
  test(`benchmark measures official ${version} tools, byte outcomes and the real native bridge`, async () => {
    const report = await runBenchmark({ executable, versions: [version], repeats: 2, warmups: 1,
      buildProfile: process.env.XNIP_BENCH_BUILD_PROFILE ?? 'debug',
      scenarioNames: ['tiny-config', 'same-file-batch', 'mixed-eol-all', 'noop-lf', 'reject-unobserved',
        'reject-stale', 'reject-missing', 'reject-ambiguous', 'reject-readonly', 'write-create', 'write-noop-observed'] });
    assert.equal(report.results.length, 22);
    assert.equal(report.provenance.hosts[version]['@deepseek-ai/dsh'].version, version);
    assert.match(report.provenance.binary.sha256, /^[a-f0-9]{64}$/);
    assert.ok(report.provenance.binary.bytes > 0);
    for (const row of report.results) {
      assert.equal(row.outcome_matches_runs, 2);
      assert.equal(row.samples.length, 2);
      assert.ok(row.samples.every(sample => sample.tool_calls > 0 && sample.argument_bytes > 0 && sample.result_bytes > 0));
      if (row.provider === 'xnip-native') assert.equal(row.correct_runs, 2);
    }
    const native = report.results.find(row => row.provider === 'xnip-native' && row.scenario === 'tiny-config');
    for (const sample of native.samples) {
      assert.deepEqual(sample.bridge.actions, { plan: 1, read: 1, commit: 1 });
      assert.equal(sample.bridge.requests, 3);
      assert.ok(sample.bridge.request_bytes > 0);
      assert.ok(sample.bridge.response_bytes > 0);
      assert.deepEqual(sample.calls.map(call => call.name), ['read', 'edit']);
    }
    const unseen = report.results.find(row => row.provider === 'xnip-native' && row.scenario === 'reject-unobserved');
    assert.ok(unseen.samples.every(sample => sample.bridge.requests === 0 && sample.calls[0].error_code === 'FS_NOT_OBSERVED'));
    const originalMixed = report.results.find(row => row.provider === 'dsh-original' && row.scenario === 'mixed-eol-all');
    assert.equal(originalMixed.byte_exact_runs, 0, 'retain the observed original-provider storage difference in the report');
    const readOnlySetup = report.setup.filter(item => item.mode === 'read-only');
    assert.equal(readOnlySetup.length, 2);
    assert.equal(readOnlySetup[0].tool_schema_sha256, readOnlySetup[1].tool_schema_sha256);
    assert.equal(readOnlySetup[0].prompt_sha256, readOnlySetup[1].prompt_sha256);
    assert.ok(report.setup.every(item => item.setup_ms > 0));
    const markdown = renderReport(report);
    assert.ok(markdown.includes('mixed-eol-all') && markdown.includes('Zero model calls'));
    assert.ok(markdown.includes(report.provenance.binary.sha256));
    const legacy = await runBenchmark({ executable, versions: [version], repeats: 2, warmups: 1,
      buildProfile: process.env.XNIP_BENCH_BUILD_PROFILE ?? 'debug', contentEncoding: 'bytes', scenarioNames: ['tiny-config'] });
    const focused = { ...report, scenarios: report.scenarios.filter(item => item.name === 'tiny-config'),
      setup: report.setup.filter(item => item.mode === 'workspace-write'), results: report.results.filter(row => row.scenario === 'tiny-config') };
    const legacyNative = legacy.results.find(row => row.provider === 'xnip-native');
    assert.ok(legacyNative.summary.bridge.total_bytes.median > native.summary.bridge.total_bytes.median,
      'same binary and fixture transfer fewer actual bytes when UTF-8 is negotiated');
    assert.ok(renderEncodingComparison(legacy, focused).includes('Legacy mutation median / p95 ms'));
    // A report with a broken native oracle must fail the artifact validation gate.
    const broken = structuredClone(report);
    broken.results.find(row => row.provider === 'xnip-native').correct_runs = 0;
    assert.throws(() => validateReport(broken), /correct_runs derived from samples/);
    const incomplete = structuredClone(report);
    incomplete.results.find(row => row.provider === 'xnip-native').samples[0].outcomes = {};
    assert.throws(() => validateReport(incomplete), /every expected file has a byte outcome/);
  });
}
