import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import crypto from 'node:crypto';
import { gunzipSync } from 'node:zlib';
import {
  runOpenCodeBenchmark,
  validateOpenCodeReport,
  renderOpenCodeReport,
} from '../benches/opencode_provider.mjs';
const root = path.resolve(import.meta.dirname, '..');
test('OpenCode benchmark executes actual CLI tools, permissions, plugin IO and independent bytes', async () => {
  const report = await runOpenCodeBenchmark({
    executable: process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip'),
    buildProfile: 'debug',
    repeats: 2,
    warmups: 1,
    scenarioNames: [
      'tiny-config',
      'long-line',
      'missing-guard',
      'ambiguous-guard',
      'identical-edit',
      'policy-denied',
      'patch-multifile',
    ],
  });
  assert.equal(report.results.length, 14);
  assert.equal(report.provenance.host.version, '1.19.3');
  assert.equal(report.setup.length, 4);
  assert.ok(
    report.results.every(
      (row) => row.correct_runs === 2 && row.samples.every((sample) => sample.model_calls === 0),
    ),
  );
  const grouped = report.results.find(
    (row) => row.provider === 'xnip-native' && row.scenario === 'patch-multifile',
  );
  assert.ok(
    grouped.samples.every(
      (sample) => sample.tool_calls === 1 && sample.calls[0].name === 'xnip_apply',
    ),
  );
  const original = report.results.find(
    (row) => row.provider === 'opencode-original' && row.scenario === 'patch-multifile',
  );
  assert.ok(original.samples.every((sample) => sample.tool_calls === 2));
  assert.match(renderOpenCodeReport(report), /Full CLI invocation through runtime.call/);
  const incomplete = structuredClone(report);
  const key = Object.keys(incomplete.results[0].samples[0].outcomes)[0];
  delete incomplete.results[0].samples[0].outcomes[key];
  assert.throws(() => validateOpenCodeReport(incomplete));
  const forged = structuredClone(report);
  forged.results[0].correct_runs = 0;
  assert.throws(() => validateOpenCodeReport(forged));
  const payloadOf = (call) =>
    call.result_payload.encoding === 'json'
      ? call.result_payload.value
      : JSON.parse(gunzipSync(Buffer.from(call.result_payload.data, 'base64')).toString('utf8'));
  const replacePayload = (call, value) => {
    const json = JSON.stringify(value);
    call.result_payload = { encoding: 'json', value };
    call.result_bytes = Buffer.byteLength(json);
    call.result_sha256 = crypto.createHash('sha256').update(json).digest('hex');
  };
  const long = report.results.find(
    (row) => row.provider === 'opencode-original' && row.scenario === 'long-line',
  );
  assert.ok(long.samples.every((sample) => sample.calls[0].stdout_bytes > 146176));
  assert.ok(
    long.samples.every((sample) => sample.calls[0].result_payload.encoding === 'gzip+base64'),
  );
  for (const failure of ['spawn ENOENT', 'Tool nonexistent is not found for agent build']) {
    const broken = structuredClone(report);
    const row = broken.results.find(
      (row) => row.provider === 'xnip-native' && row.scenario === 'missing-guard',
    );
    const call = row.samples[0].calls[0];
    call.error = failure;
    call.code = 1;
    replacePayload(call, { code: 1, error: failure });
    assert.throws(() => validateOpenCodeReport(broken), /public outcome reason/);
  }
  const wrongArguments = structuredClone(report);
  const call = wrongArguments.results[0].samples[0].calls[0];
  const payload = payloadOf(call);
  call.arguments.oldString = 'different input';
  payload.input = structuredClone(call.arguments);
  const args = JSON.stringify(call.arguments);
  call.argument_bytes = Buffer.byteLength(args);
  call.argument_sha256 = crypto.createHash('sha256').update(args).digest('hex');
  replacePayload(call, payload);
  assert.throws(
    () => validateOpenCodeReport(wrongArguments),
    /planned tool sequence and arguments/,
  );
  const wrongTool = structuredClone(report);
  const toolCall = wrongTool.results[0].samples[0].calls[0];
  const toolPayload = payloadOf(toolCall);
  toolCall.name = toolPayload.tool = 'nonexistent';
  replacePayload(toolCall, toolPayload);
  assert.throws(() => validateOpenCodeReport(wrongTool), /planned tool sequence and arguments/);
  const missingCall = structuredClone(report);
  const calls = missingCall.results.find(
    (row) => row.provider === 'opencode-original' && row.scenario === 'patch-multifile',
  ).samples[0];
  calls.calls.pop();
  calls.tool_calls = calls.calls.length;
  assert.throws(() => validateOpenCodeReport(missingCall), /planned tool sequence and arguments/);
  const emptyReceipt = structuredClone(report);
  const receipt = emptyReceipt.results[0].samples[0].calls[0];
  const empty = payloadOf(receipt);
  empty.result.output = '';
  receipt.output = '';
  replacePayload(receipt, empty);
  assert.throws(() => validateOpenCodeReport(emptyReceipt), /known tool success receipt/);
  const badOriginalBytes = structuredClone(report);
  const originalRow = badOriginalBytes.results.find(
    (row) => row.provider === 'opencode-original' && row.scenario === 'tiny-config',
  );
  const bad = originalRow.samples[0];
  const file = Object.keys(bad.outcomes)[0];
  bad.outcomes[file].actual.sha256 = '0'.repeat(64);
  bad.outcomes[file].byte_exact = false;
  bad.byte_exact = bad.correct = false;
  originalRow.correct_runs--;
  assert.throws(() => validateOpenCodeReport(badOriginalBytes), /exact byte goal/);
  const badWarmup = structuredClone(report);
  const warm = badWarmup.results[0].warmups[0];
  const warmFile = Object.keys(warm.outcomes)[0];
  warm.outcomes[warmFile].actual.sha256 = '0'.repeat(64);
  warm.outcomes[warmFile].byte_exact = false;
  warm.byte_exact = warm.correct = false;
  assert.throws(() => validateOpenCodeReport(badWarmup), /exact byte goal/);
});
