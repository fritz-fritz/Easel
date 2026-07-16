import { readdir } from "node:fs/promises";
import path from "node:path";
import { DefaultArtifactClient } from "@actions/artifact";

const stagedDir = process.env.STAGED_DIR;
const retentionDays = Number.parseInt(process.env.RETENTION_DAYS || "7", 10);

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

for (const filePath of files) {
  const name = path.basename(filePath);
  const result = await client.uploadArtifact(name, [filePath], stagedDir, {
    retentionDays,
    skipArchive: true,
  });
  console.log(
    `ci-visual upload: ${name} → artifact-id=${result.id} size=${result.size}`,
  );
}
