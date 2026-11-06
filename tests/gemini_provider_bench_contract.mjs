import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { runGeminiBenchmark, renderGeminiReport, validateGeminiReport } from '../benches/gemini_provider.mjs';

const executable = process.env.XNIP_TEST_BINARY ?? path.resolve('target/debug/xnip');

test('Gemini benchmark executes official registry, policies, host matching and native IO', async () => {
  const report = await runGeminiBenchmark({ executable, repeats: 2, warmups: 1,
    buildProfile: process.env.XNIP_BENCH_BUILD_PROFILE ?? 'debug',
    scenarioNames: ['tiny-config', 'same-file-batch', 'global-replace', 'gemini-regex-spacing', 'gemini-reject-missing',
      'gemini-reject-count', 'gemini-noop', 'gemini-write-create', 'gemini-mixed-eol', 'gemini-confirmed-edit', 'gemini-policy-denied'] });
  assert.equal(report.results.length, 22);
  assert.equal(report.provenance.host.version, '0.27.2');
  assert.match(report.provenance.host.dependency_tree_sha256, /^[a-f0-9]{64}$/);
  assert.equal(report.setup.length, 6);
  for (const row of report.results) {
    assert.equal(row.outcome_matches_runs, 2);
    assert.ok(row.samples.every(sample => sample.model_calls === 0 && sample.argument_bytes > 0));
    if (row.scenario !== 'gemini-mixed-eol') assert.equal(row.correct_runs, 2);
  }
  const native = report.results.find(row => row.provider === 'xnip-native' && row.scenario === 'tiny-config');
  for (const sample of native.samples) {
    assert.equal(sample.bridge.actions.plan, 1);
    assert.equal(sample.bridge.actions.commit, 1);
    assert.ok(sample.bridge.actions.read >= 1 && sample.bridge.request_bytes > 0 && sample.bridge.response_bytes > 0);
  }
  const batch = report.results.find(row => row.provider === 'xnip-native' && row.scenario === 'same-file-batch');
  assert.ok(batch.samples.every(sample => sample.tool_calls === 3));
  const confirmation = report.results.find(row => row.provider === 'xnip-native' && row.scenario === 'gemini-confirmed-edit');
  assert.ok(confirmation.samples.every(sample => sample.policy_events['confirmation-details'] >= 1));
  const denied = report.results.find(row => row.provider === 'xnip-native' && row.scenario === 'gemini-policy-denied');
  assert.ok(denied.samples.every(sample => sample.bridge.requests === 0 && sample.calls[0].exception.message.includes('denied by policy')));
  const mixed = report.results.filter(row => row.scenario === 'gemini-mixed-eol');
  assert.ok(mixed.every(row => row.byte_exact_runs === 0 && row.outcome_matches_runs === 2));
  assert.ok(renderGeminiReport(report).includes('Zero model calls'));
  const incomplete = structuredClone(report);
  incomplete.results[0].samples[0].outcomes = {};
  assert.throws(() => validateGeminiReport(incomplete), /complete expected file oracle/);
  const substituted = structuredClone(report);
  Object.values(substituted.results[0].samples[0].outcomes)[0].expected.sha256 = '0'.repeat(64);
  assert.throws(() => validateGeminiReport(substituted), /file outcome uses the declared oracle/);
  const undeclared = structuredClone(report);
  undeclared.results[0].provider = 'undeclared';
  assert.throws(() => validateGeminiReport(undeclared), /declared Gemini provider/);
});
