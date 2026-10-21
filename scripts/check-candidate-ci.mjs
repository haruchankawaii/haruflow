import assert from 'node:assert/strict';
const { GITHUB_REPOSITORY: repository, GITHUB_SHA: sha, GITHUB_TOKEN: token } = process.env;
assert.match(repository || '', /^[\w.-]+\/[\w.-]+$/); assert.match(sha || '', /^[a-f0-9]{40}$/); assert.ok(token);
const response = await fetch(`https://api.github.com/repos/${repository}/actions/workflows/ci.yml/runs?head_sha=${sha}&per_page=30`, {
  headers: { Accept: 'application/vnd.github+json', Authorization: `Bearer ${token}`, 'X-GitHub-Api-Version': '2022-11-28' },
});
assert.ok(response.ok, `Cannot verify CI status (${response.status}).`);
const runs = (await response.json()).workflow_runs;
assert.ok(runs.some(run => run.head_sha === sha && run.status === 'completed' && run.conclusion === 'success'), 'Run Community checks successfully on this exact commit before producing a candidate.');
console.log('Exact-commit Community checks passed. Candidate packaging may proceed.');
