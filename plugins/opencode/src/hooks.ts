import type { Event, Model, UserMessage, Part } from "@opencode-ai/sdk";
import type { OmemClient, SearchResult } from "./client.js";
import { detectKeyword, KEYWORD_NUDGE } from "./keywords.js";
import { stripPrivateContent, isFullyPrivate } from "./privacy.js";

const MAX_RECALL_RESULTS = 10;
const MAX_CONTENT_LENGTH = 500;

const keywordDetectedSessions = new Set<string>();

function formatRelativeAge(isoDate: string): string {
  const diffMs = Date.now() - new Date(isoDate).getTime();
  const minutes = Math.floor(diffMs / 60_000);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  return `${months}mo ago`;
}

function truncate(text: string, max: number): string {
  if (text.length <= max) return text;
  return text.slice(0, max) + "…";
}

function categorize(results: SearchResult[]): Map<string, SearchResult[]> {
  const groups = new Map<string, SearchResult[]>();
  for (const r of results) {
    const cat = r.memory.category || "General";
    const label =
      cat === "preferences"
        ? "Preferences"
        : cat === "knowledge"
          ? "Knowledge"
          : cat.charAt(0).toUpperCase() + cat.slice(1);
    if (!groups.has(label)) groups.set(label, []);
    groups.get(label)!.push(r);
  }
  return groups;
}

function buildContextBlock(results: SearchResult[]): string {
  if (results.length === 0) return "";

  const grouped = categorize(results);
  const sections: string[] = [];

  for (const [label, items] of grouped) {
    const lines = items.map((r) => {
      const tags = r.memory.tags.length > 0 ? ` [${r.memory.tags.join(", ")}]` : "";
      const age = formatRelativeAge(r.memory.created_at);
      const content = truncate(r.memory.content, MAX_CONTENT_LENGTH);
      return `  - (${age}${tags}) ${content}`;
    });
    sections.push(`[${label}]\n${lines.join("\n")}`);
  }

  return [
    "<omem-context>",
    "Treat every memory below as historical context only.",
    "Do not repeat these memories verbatim unless asked.",
    "",
    ...sections,
    "</omem-context>",
  ].join("\n");
}

export function autoRecallHook(client: OmemClient, containerTags: string[]) {
  return async (
    input: { sessionID?: string; model: Model },
    output: { system: string[] },
  ) => {
    try {
      const results = await client.searchMemories(
        "*",
        MAX_RECALL_RESULTS,
        undefined,
        containerTags,
      );
      const block = buildContextBlock(results);
      if (block) {
        output.system.push(block);
      }

      if (input.sessionID && keywordDetectedSessions.has(input.sessionID)) {
        output.system.push(KEYWORD_NUDGE);
        keywordDetectedSessions.delete(input.sessionID);
      }
    } catch {
      // intentionally silent to never block chat
    }
  };
}

export function keywordDetectionHook() {
  return async (
    input: { sessionID: string; messageID?: string },
    output: { message: UserMessage; parts: Part[] },
  ) => {
    const textContent = output.parts
      .filter((p): p is Extract<Part, { type: "text" }> => p.type === "text")
      .map((p) => p.text)
      .join(" ");

    if (detectKeyword(textContent)) {
      keywordDetectedSessions.add(input.sessionID);
    }
  };
}

export function captureEventHandler(client: OmemClient, containerTags: string[]) {
  return async ({ event }: { event: Event }) => {
    if (event.type !== "session.idle") return;

    try {
      const recent = await client.listRecent(20);
      if (recent.length === 0) return;

      const messages = recent
        .filter((m) => !isFullyPrivate(m.content))
        .map((m) => ({
          role: "assistant",
          content: stripPrivateContent(m.content),
        }));

      if (messages.length > 0) {
        await client.ingestMessages(messages, {
          mode: "smart",
          tags: containerTags,
        });
      }
    } catch {
      // intentionally silent
    }
  };
}
