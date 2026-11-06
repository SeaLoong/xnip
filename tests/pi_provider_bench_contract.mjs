import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { runPiBenchmark, renderPiReport, validatePiReport } from '../benches/pi_provider.mjs';

const root = path.resolve(import.meta.dirname, '..');
const executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip');

test('Pi benchmark executes both SDK providers, batching, fuzzy matching and host rejection outcomes', async () => {
  const report = await runPiBenchmark({ executable, repeats: 2, warmups: 1,
    buildProfile: process.env.XNIP_BENCH_BUILD_PROFILE ?? 'debug',
    scenarioNames: ['tiny-config', 'same-file-batch', 'global-replace', 'pi-fuzzy-quote', 'pi-reject-missing', 'pi-reject-ambiguous', 'pi-noop'] });
  assert.equal(report.results.length, 14);
  assert.equal(report.provenance.host.version, '1.3.0');
  assert.match(report.provenance.binary.sha256, /^[a-f0-9]{64}$/);
  assert.equal(report.setup.length, 2);
  assert.equal(report.setup[0].tool_schema_sha256, report.setup[1].tool_schema_sha256);
  assert.ok(report.setup.every(item => item.setup_ms > 0));
  for (const row of report.results) {
    assert.equal(row.correct_runs, 2);
    assert.ok(row.samples.every(sample => sample.model_calls === 0 && sample.argument_bytes > 0 && sample.result_bytes > 0));
    if (row.scenario === 'same-file-batch') for (const sample of row.samples) {
      assert.equal(sample.tool_calls, 2, 'read plus one native Pi edit containing both edits');
      assert.deepEqual(sample.calls.map(call => call.name), ['read', 'edit']);
    }
    if (row.scenario === 'pi-noop') assert.ok(row.samples.every(sample => sample.calls[1].is_error));
  }
  const original = report.results.find(row => row.provider === 'pi-original' && row.scenario === 'pi-fuzzy-quote');
  const native = report.results.find(row => row.provider === 'xnip-native' && row.scenario === 'pi-fuzzy-quote');
  assert.equal(original.samples[0].outcomes['example.txt'].actual.sha256, native.samples[0].outcomes['example.txt'].actual.sha256);
  const rendered = renderPiReport(report);
  assert.ok(rendered.includes('Zero model calls') && rendered.includes(report.provenance.binary.sha256));
  const broken = structuredClone(report);
  broken.results[0].correct_runs = 0;
  assert.throws(() => validatePiReport(broken), /correct_runs derived from samples/);
  const incomplete = structuredClone(report);
  delete incomplete.results[0].samples[0].outcomes['src/example.txt'];
  assert.throws(() => validatePiReport(incomplete), /complete expected file oracle/);
});
