"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const verifiedCi = require("../.github/scripts/verified_ci.cjs");

const SHA = "a".repeat(40);
const CONTEXT = {
  repo: { owner: "example", repo: "xnip" },
  payload: { repository: { default_branch: "master" } },
};

function workflowRun(overrides = {}) {
  return {
    id: 700,
    run_number: 70,
    run_attempt: 2,
    head_sha: SHA,
    event: "push",
    head_branch: "master",
    head_repository: { full_name: "example/xnip" },
    status: "in_progress",
    html_url: "https://github.com/example/xnip/actions/runs/700",
    ...overrides,
  };
}

function gate(overrides = {}) {
  return {
    name: "ci",
    run_id: 700,
    run_attempt: 2,
    status: "completed",
    conclusion: "success",
    html_url: "https://github.com/example/xnip/actions/runs/700/job/800",
    steps: [{ name: "Full verification", conclusion: "success" }],
    ...overrides,
  };
}

function harness({
  runs = [workflowRun()],
  jobSnapshots = [[gate()]],
  runSnapshots = [workflowRun()],
  attempts = 3,
  sha = SHA,
  context = CONTEXT,
} = {}) {
  const calls = [];
  const outputs = [];
  const infos = [];
  const waits = [];
  let jobIndex = 0;
  let runIndex = 0;
  const sample = (snapshots, index) => {
    const value = snapshots[Math.min(index, snapshots.length - 1)];
    if (value instanceof Error) throw value;
    return value;
  };
  const github = {
    rest: {
      actions: {
        listWorkflowRuns: "runs",
        listJobsForWorkflowRunAttempt: "jobs",
        getWorkflowRun: async (parameters) => {
          calls.push({ endpoint: "get", parameters });
          return { data: sample(runSnapshots, runIndex++) };
        },
      },
    },
    paginate: async (endpoint, parameters) => {
      calls.push({ endpoint, parameters });
      if (endpoint === "runs") {
        if (runs instanceof Error) throw runs;
        return runs;
      }
      assert.equal(endpoint, "jobs");
      return sample(jobSnapshots, jobIndex++);
    },
  };
  return {
    calls, outputs, infos, waits,
    run: () => verifiedCi({
      github, context, sha, attempts,
      core: {
        setOutput: (name, value) => outputs.push([name, value]),
        info: (message) => infos.push(message),
      },
      wait: async (milliseconds) => waits.push(milliseconds),
    }),
  };
}

test("exact-commit full gate is reused through the fixed CI workflow query", async () => {
  const fixture = harness();
  assert.equal(await fixture.run(), true);
  assert.deepEqual(fixture.calls[0], {
    endpoint: "runs",
    parameters: {
      owner: "example", repo: "xnip", workflow_id: "ci.yml",
      head_sha: SHA, event: "push", branch: "master", per_page: 100,
    },
  });
  assert.deepEqual(fixture.outputs.at(-1), ["reused", "true"]);
  for (const call of fixture.calls.filter((item) => item.endpoint === "jobs")) {
    assert.equal(call.parameters.attempt_number, 2);
    assert.equal(call.parameters.run_id, 700);
  }
});

for (const [name, override] of [
  ["fork", { head_repository: { full_name: "other/xnip" } }],
  ["wrong branch", { head_branch: "feature" }],
  ["pull request", { event: "pull_request" }],
  ["wrong SHA", { head_sha: "b".repeat(40) }],
  ["missing repository identity", { head_repository: undefined }],
]) {
  test(`untrusted ${name} runs cannot satisfy release verification`, async () => {
    const fixture = harness({ runs: [workflowRun(override)] });
    assert.equal(await fixture.run(), false);
    assert.deepEqual(fixture.outputs, [["reused", "false"]]);
    assert.deepEqual(fixture.calls.map((call) => call.endpoint), ["runs"]);
  });
}

test("missing trusted run requests complete verification", async () => {
  const fixture = harness({ runs: [] });
  assert.equal(await fixture.run(), false);
  assert.equal(fixture.calls.length, 1);
});

for (const [name, steps] of [
  ["missing", []],
  ["skipped", [{ name: "Full verification", conclusion: "skipped" }]],
  ["failed", [{ name: "Full verification", conclusion: "failure" }]],
]) {
  test(`a ${name} full marker falls back to complete verification`, async () => {
    const fixture = harness({ jobSnapshots: [[gate({ steps })]] });
    assert.equal(await fixture.run(), false);
    assert.deepEqual(fixture.outputs.at(-1), ["reused", "false"]);
    assert.deepEqual(fixture.waits, []);
  });
}

for (const conclusion of ["failure", "cancelled", "timed_out", "skipped"]) {
  test(`${conclusion} quality gate blocks release instead of falling back`, async () => {
    const fixture = harness({ jobSnapshots: [[gate({ conclusion })]] });
    await assert.rejects(fixture.run(), /CI gate did not pass/);
    assert.deepEqual(fixture.outputs, [["reused", "false"]]);
    assert.deepEqual(fixture.waits, []);
  });
}

test("development builds in progress never delay a successful quality gate", async () => {
  const fixture = harness({
    jobSnapshots: [[
      gate(),
      { name: "build (windows)", status: "in_progress", conclusion: null },
    ]],
    runSnapshots: [workflowRun({ status: "in_progress" })],
  });
  assert.equal(await fixture.run(), true);
  assert.deepEqual(fixture.waits, []);
});

test("failed development delivery does not invalidate a successful full gate", async () => {
  const fixture = harness({
    runSnapshots: [workflowRun({ status: "completed", conclusion: "failure" })],
  });
  assert.equal(await fixture.run(), true);
  assert.deepEqual(fixture.waits, []);
});

test("in-progress gate is polled until the quality checks complete", async () => {
  const fixture = harness({
    jobSnapshots: [
      [gate({ status: "in_progress", conclusion: null })],
      [gate()],
    ],
  });
  assert.equal(await fixture.run(), true);
  assert.deepEqual(fixture.waits, [15000]);
  assert.equal(fixture.calls.filter((call) => call.endpoint === "jobs").length, 2);
});

test("an old completed workflow without a gate requests full checks after rechecking", async () => {
  const fixture = harness({
    jobSnapshots: [[], []],
    runSnapshots: [workflowRun({ status: "completed" })],
  });
  assert.equal(await fixture.run(), false);
  assert.equal(fixture.calls.filter((call) => call.endpoint === "jobs").length, 2);
  assert.deepEqual(fixture.outputs, [["reused", "false"]]);
});

for (const conclusion of ["failure", "cancelled"]) {
  test(`a late visible ${conclusion} gate is not mistaken for an old missing gate`, async () => {
    const fixture = harness({
      jobSnapshots: [[], [gate({ conclusion })]],
      runSnapshots: [workflowRun({ status: "completed" })],
    });
    await assert.rejects(fixture.run(), /CI gate did not pass/);
    assert.deepEqual(fixture.outputs, [["reused", "false"]]);
  });
}

test("a successful gate that appears at run completion can still be reused", async () => {
  const fixture = harness({
    jobSnapshots: [[], [gate()]],
    runSnapshots: [workflowRun({ status: "completed" })],
  });
  assert.equal(await fixture.run(), true);
  assert.deepEqual(fixture.waits, []);
});

test("a rerun starting during the terminal-state recheck requires a safe retry", async () => {
  const fixture = harness({
    jobSnapshots: [[], []],
    runSnapshots: [
      workflowRun({ status: "completed" }),
      workflowRun({ status: "completed" }),
      workflowRun({ status: "completed" }),
      workflowRun({ run_attempt: 3 }),
    ],
  });
  await assert.rejects(fixture.run(), /attempt changed/);
  assert.deepEqual(fixture.outputs, [["reused", "false"]]);
});

test("attempt-specific jobs remain usable when the job omits run_attempt", async () => {
  const fixture = harness({ jobSnapshots: [[gate({ run_attempt: undefined })]] });
  assert.equal(await fixture.run(), true);
  const call = fixture.calls.find((item) => item.endpoint === "jobs");
  assert.equal(call.parameters.attempt_number, 2);
});

test("a rerun between a successful jobs snapshot and the run snapshot cannot be reused", async () => {
  const fixture = harness({
    runSnapshots: [workflowRun(), workflowRun({ run_attempt: 3 })],
  });
  await assert.rejects(fixture.run(), /attempt changed/);
  assert.deepEqual(fixture.outputs, [["reused", "false"]]);
});

test("newest run wins even when an older run has successful full verification", async () => {
  const newer = workflowRun({ id: 701, run_number: 71, run_attempt: 1 });
  const fixture = harness({
    runs: [workflowRun(), newer],
    jobSnapshots: [[gate({ run_id: 701, run_attempt: 1, steps: [] })]],
    runSnapshots: [newer],
  });
  assert.equal(await fixture.run(), false);
  assert.equal(fixture.calls.find((call) => call.endpoint === "jobs").parameters.run_id, 701);
});

test("a successful gate from a previous attempt cannot be reused", async () => {
  const fixture = harness({
    jobSnapshots: [[gate({ run_attempt: 1 })]],
    runSnapshots: [workflowRun({ run_attempt: 2 })],
  });
  await assert.rejects(fixture.run(), /latest run attempt/);
  assert.deepEqual(fixture.outputs, [["reused", "false"]]);
});

test("multiple latest-attempt gates are rejected instead of selecting a successful one", async () => {
  const fixture = harness({
    jobSnapshots: [[gate(), gate({ conclusion: "failure" })]],
  });
  await assert.rejects(fixture.run(), /multiple quality gates/);
  assert.deepEqual(fixture.outputs, [["reused", "false"]]);
});

test("a completed run with an unfinished gate fails closed", async () => {
  const fixture = harness({
    jobSnapshots: [[gate({ status: "in_progress", conclusion: null })]],
    runSnapshots: [workflowRun({ status: "completed" })],
  });
  await assert.rejects(fixture.run(), /unfinished quality gate/);
});

for (const [endpoint, options] of [
  ["workflow listing", { runs: new Error("API unavailable") }],
  ["jobs listing", { jobSnapshots: [new Error("API unavailable")] }],
  ["run lookup", { runSnapshots: [new Error("API unavailable")] }],
]) {
  test(`${endpoint} errors block release`, async () => {
    const fixture = harness(options);
    await assert.rejects(fixture.run(), /API unavailable/);
    assert.deepEqual(fixture.outputs, [["reused", "false"]]);
  });
}

test("a gate that never completes times out without claiming verification", async () => {
  const fixture = harness({
    attempts: 2,
    jobSnapshots: [[gate({ status: "queued", conclusion: null })]],
  });
  await assert.rejects(fixture.run(), /Timed out/);
  assert.deepEqual(fixture.waits, [15000]);
  assert.deepEqual(fixture.outputs, [["reused", "false"]]);
});

test("run metadata must retain the exact trusted commit and attempt identity", async () => {
  const fixture = harness({
    runSnapshots: [workflowRun({ head_sha: "b".repeat(40) })],
  });
  await assert.rejects(fixture.run(), /inconsistent run identity/);
});

test("duplicate full-verification markers are rejected", async () => {
  const fixture = harness({
    jobSnapshots: [[gate({
      steps: [
        { name: "Full verification", conclusion: "success" },
        { name: "Full verification", conclusion: "failure" },
      ],
    })]],
  });
  await assert.rejects(fixture.run(), /multiple full-verification markers/);
});

test("a symbolic ref is never accepted as a commit identity", async () => {
  const fixture = harness({ sha: "HEAD" });
  await assert.rejects(fixture.run(), /exact commit SHA/);
  assert.equal(fixture.calls.length, 0);
});
