import { createHash } from "crypto";

export function getUserTag(email: string): string {
  const hash = createHash("sha256").update(email).digest("hex").slice(0, 16);
  return `omem_user_${hash}`;
}

export function getProjectTag(directory: string): string {
  const hash = createHash("sha256")
    .update(directory)
    .digest("hex")
    .slice(0, 16);
  return `omem_project_${hash}`;
}
