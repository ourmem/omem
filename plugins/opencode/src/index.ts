import type { Plugin } from "@opencode-ai/plugin";
import { OmemClient } from "./client.js";
import { autoRecallHook, compactingHook, keywordDetectionHook } from "./hooks.js";
import { getUserTag, getProjectTag } from "./tags.js";
import { buildTools } from "./tools.js";

export const OmemPlugin: Plugin = async ({ directory }) => {
  const omemClient = new OmemClient(
    process.env.OMEM_API_URL || "http://localhost:8080",
    process.env.OMEM_API_KEY || "",
  );

  const email = process.env.GIT_AUTHOR_EMAIL || process.env.USER || "unknown";
  const cwd = directory || process.cwd();
  const containerTags = [getUserTag(email), getProjectTag(cwd)];

  return {
    "experimental.chat.system.transform": autoRecallHook(omemClient, containerTags),
    "chat.message": keywordDetectionHook(),
    "experimental.session.compacting": compactingHook(omemClient, containerTags),
    tool: buildTools(omemClient, containerTags),
    "shell.env": async (_input, output) => {
      if (directory) {
        output.env.OMEM_PROJECT_DIR = directory;
      }
    },
  };
};
