import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { runClineBenchmark, renderClineReport, validateClineReport } from '../benches/cline_provider.mjs';

const root = path.resolve(import.meta.dirname, '..');
const executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip');

test('Cline benchmark executes official registry dispatch, patch batches and operation failures', async () => {
  const report = await runClineBenchmark({ executable, repeats: 2, warmups: 1,
    buildProfile: process.env.XNIP_BENCH_BUILD_PROFILE ?? 'debug',
    scenarioNames: ['tiny-config', 'crlf', 'no-final-newline', 'same-file-batch', 'multi-file-batch',
      'cline-fuzzy-whitespace', 'cline-first-match', 'cline-noop', 'cline-batch-adjacent', 'cline-batch-disjoint',
      'cline-bom-crlf', 'cline-reject-context', 'cline-reject-existing-add', 'cline-reject-sentinels',
      'cline-alias-update-update', 'cline-alias-add-add', 'cline-alias-delete-delete', 'cline-alias-move-update', 'cline-alias-same-path-move',
      'large-65536', 'cline-eof-hint-large-65536'] });
  assert.equal(report.results.length, 42);
  assert.equal(report.provenance.host.version, '0.2.0');
  assert.match(report.provenance.binary.sha256, /^[a-f0-9]{64}$/);
  assert.equal(report.setup.length, 2);
  assert.equal(report.setup[0].tool_schema_sha256, report.setup[1].tool_schema_sha256);
  assert.ok(report.setup.every(item => item.setup_ms > 0));
  for (const row of report.results) {
    assert.equal(row.correct_runs, 2);
    for (const sample of [...row.warmups, ...row.samples]) {
      assert.equal(sample.model_calls, 0);
      const names = row.scenario === 'cline-alias-add-add' ? ['apply_patch'] : ['read_files', 'apply_patch'];
      assert.equal(sample.tool_calls, names.length, 'one grouped existing-file read and one patch, including multi-file fixtures');
      assert.deepEqual(sample.calls.map(call => call.name), names);
      assert.deepEqual(sample.agent_events, { started: names.length, finished: names.length });
      assert.ok(sample.argument_bytes > 0 && sample.result_bytes >= sample.output_bytes);
    }
    if (row.scenario === 'cline-noop') assert.ok(row.samples.every(sample => !sample.calls[1].is_error));
    if (row.scenario.startsWith('cline-reject-')) assert.ok(row.samples.every(sample =>
      sample.calls[1].is_error && !sample.calls[1].dispatch_is_error), 'official operation failure stays distinct from Agent dispatch status');
  }
  const rendered = renderClineReport(report);
  assert.ok(rendered.includes('Zero actual model stream calls') && rendered.includes(report.provenance.binary.sha256));
  const broken = structuredClone(report);
  broken.results[0].correct_runs = 0;
  assert.throws(() => validateClineReport(broken), /correct_runs derived from samples/);
  const incomplete = structuredClone(report);
  delete incomplete.results[0].samples[0].outcomes['src/example.txt'];
  assert.throws(() => validateClineReport(incomplete), /complete expected file oracle/);
});
