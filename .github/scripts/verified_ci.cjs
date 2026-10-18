"use strict";

// Reuse only the latest trusted default-branch CI run for this exact commit.
// Artifact builds and publication belong to the caller and never affect this gate.
module.exports = async function verifiedCi({
  github,
  context,
  core,
  sha,
  wait = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds)),
  attempts = 120,
}) {
  core.setOutput("reused", "false");
  const { owner, repo } = context.repo;
  const branch = context.payload.repository.default_branch;
  const repository = `${owner}/${repo}`;
  if (!/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/i.test(sha)) {
    throw new Error("CI reuse requires an exact commit SHA");
  }
  if (typeof branch !== "string" || branch.length === 0) {
    throw new Error("CI reuse requires the repository default branch");
  }
  if (!Number.isInteger(attempts) || attempts < 1) {
    throw new Error("CI reuse attempts must be a positive integer");
  }

  const trusted = (run) =>
    run.head_sha === sha &&
    run.event === "push" &&
    run.head_branch === branch &&
    run.head_repository?.full_name === repository;
  const runs = await github.paginate(github.rest.actions.listWorkflowRuns, {
    owner,
    repo,
    workflow_id: "ci.yml",
    head_sha: sha,
    event: "push",
    branch,
    per_page: 100,
  });
  const candidates = runs.filter(trusted);
  for (const run of candidates) {
    if (!Number.isSafeInteger(run.id) || !Number.isSafeInteger(run.run_number)) {
      throw new Error("CI returned a run without a valid identity");
    }
  }
  // The API normally returns newest first; make the choice explicit.
  candidates.sort((left, right) =>
    right.run_number - left.run_number || right.id - left.id);
  const run = candidates[0];
  if (!run) return false;

  const currentRun = async () => {
    const { data } = await github.rest.actions.getWorkflowRun({
      owner, repo, run_id: run.id,
    });
    if (!trusted(data) || data.id !== run.id ||
        !Number.isSafeInteger(data.run_attempt) || data.run_attempt < 1) {
      throw new Error("CI returned inconsistent run identity or attempt metadata");
    }
    return data;
  };
  const latestSnapshot = async () => {
    const before = await currentRun();
    const jobs = await github.paginate(
      github.rest.actions.listJobsForWorkflowRunAttempt,
      {
        owner, repo, run_id: run.id,
        attempt_number: before.run_attempt, per_page: 100,
      },
    );
    const current = await currentRun();
    if (current.run_attempt !== before.run_attempt) {
      throw new Error("CI run attempt changed while reading quality gates; retry the release");
    }
    return { jobs, current };
  };
  const inspect = (jobs, current) => {
    const gates = jobs.filter((job) => job.name === "ci");
    if (gates.length > 1) {
      throw new Error("CI returned multiple quality gates for the latest attempt");
    }
    const gate = gates[0];
    if (!gate) return { present: false };
    // The attempt-specific endpoint also covers responses omitting run_attempt.
    if ((gate.run_attempt !== undefined && gate.run_attempt !== current.run_attempt) ||
        (gate.run_id !== undefined && gate.run_id !== run.id)) {
      throw new Error("CI gate does not belong to the latest run attempt");
    }
    if (gate.status !== "completed") return { present: true };
    if (gate.conclusion !== "success") {
      throw new Error(`CI gate did not pass: ${gate.html_url || run.html_url}`);
    }
    const markers = (gate.steps || []).filter((step) =>
      step.name === "Full verification");
    if (markers.length > 1) {
      throw new Error("CI returned multiple full-verification markers");
    }
    return {
      present: true,
      complete: true,
      full: markers.length === 1 && markers[0].conclusion === "success",
      url: gate.html_url || run.html_url,
    };
  };
  const finish = (result) => {
    core.setOutput("reused", result.full ? "true" : "false");
    core.info(result.full
      ? `Reusing ${result.url}`
      : "Scoped CI needs full release verification");
    return result.full;
  };

  for (let attempt = 0; attempt < attempts; attempt++) {
    let { jobs, current } = await latestSnapshot();
    let result = inspect(jobs, current);
    if (result.complete) return finish(result);

    if (current.status === "completed") {
      // A gate can become visible between the jobs snapshot and run completion.
      // Recheck before treating an old run with no gate as unverified.
      ({ jobs, current } = await latestSnapshot());
      result = inspect(jobs, current);
      if (result.complete) return finish(result);
      if (current.status === "completed") {
        if (result.present) {
          throw new Error("Completed CI run has an unfinished quality gate");
        }
        core.info("Prior CI has no quality gate; full release verification is required");
        return false;
      }
    }

    if (attempt + 1 < attempts) await wait(15000);
  }
  throw new Error("Timed out waiting for the exact commit CI gate");
};
