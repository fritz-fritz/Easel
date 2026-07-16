import { readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { DefaultArtifactClient } from "@actions/artifact";

const stagedDir = process.env.STAGED_DIR;
const retentionDays = Number.parseInt(process.env.RETENTION_DAYS || "7", 10);
const stage = process.env.INPUT_STAGE || "unknown";
const runnerOs = process.env.INPUT_RUNNER_OS || "unknown";
const serverUrl = process.env.GITHUB_SERVER_URL || "https://github.com";
const repository = process.env.GITHUB_REPOSITORY || "";
const runId = process.env.GITHUB_RUN_ID || "";
const runAttempt = process.env.GITHUB_RUN_ATTEMPT || "1";
const sha = process.env.GITHUB_SHA || "";

if (!stagedDir) {
  console.error("STAGED_DIR is required");
  process.exit(1);
}

const entries = await readdir(stagedDir, { withFileTypes: true });
const files = entries
  .filter((entry) => entry.isFile())
  .map((entry) => path.join(stagedDir, entry.name));

if (files.length === 0) {
  console.log("ci-visual upload: nothing to upload");
  process.exit(0);
}

const client = new DefaultArtifactClient();
const images = [];

for (const filePath of files) {
  const filename = path.basename(filePath);
  const result = await client.uploadArtifact(filename, [filePath], stagedDir, {
    retentionDays,
    skipArchive: true,
  });
  const artifactUrl = repository
    ? `${serverUrl}/${repository}/actions/runs/${runId}/artifacts/${result.id}`
    : "";
  images.push({
    filename,
    stem: path.parse(filename).name,
    artifact_id: result.id,
    artifact_url: artifactUrl,
  });
  console.log(
    `ci-visual upload: ${filename} → artifact-id=${result.id} size=${result.size}`,
  );
}

const manifest = {
  stage,
  os: runnerOs,
  sha,
  run_id: runId,
  run_attempt: runAttempt,
  repository,
  images,
};

const manifestName = `ci-visual-manifest-${stage}-${runnerOs}.json`;
const manifestPath = path.join(stagedDir, manifestName);
await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");

const manifestResult = await client.uploadArtifact(
  manifestName,
  [manifestPath],
  stagedDir,
  {
    retentionDays,
    skipArchive: true,
  },
);
console.log(
  `ci-visual upload: ${manifestName} → artifact-id=${manifestResult.id}`,
);
