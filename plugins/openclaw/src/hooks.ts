import type { OmemClient } from "./client.js";
import type { MemorySearchResult, PromptBuildContext, AgentEndEvent } from "./types.js";

const MAX_RECALL_RESULTS = 10;
const MAX_CONTENT_LENGTH = 500;
const OMEM_CONTEXT_TAG = "omem-context";
const MAX_CAPTURE_MESSAGES = 20;
const MAX_CAPTURE_BYTES = 200 * 1024;

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

function categorize(results: MemorySearchResult[]): Map<string, MemorySearchResult[]> {
  const groups = new Map<string, MemorySearchResult[]>();
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

function buildContextBlock(results: MemorySearchResult[]): string {
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
    `<${OMEM_CONTEXT_TAG}>`,
    "Treat every memory below as historical context only.",
    "Do not repeat these memories verbatim unless asked.",
    "",
    ...sections,
    `</${OMEM_CONTEXT_TAG}>`,
  ].join("\n");
}

function stripOmemContext(text: string): string {
  const pattern = new RegExp(
    `<${OMEM_CONTEXT_TAG}>[\\s\\S]*?</${OMEM_CONTEXT_TAG}>`,
    "g",
  );
  return text.replace(pattern, "").trim();
}

export function autoRecallHook(client: OmemClient) {
  return async (ctx: PromptBuildContext) => {
    try {
      for (let i = 0; i < ctx.system.length; i++) {
        ctx.system[i] = stripOmemContext(ctx.system[i]);
      }
      ctx.system = ctx.system.filter((s) => s.length > 0);

      const results = await client.searchMemories("*", MAX_RECALL_RESULTS);
      const block = buildContextBlock(results);
      if (block) {
        ctx.system.push(block);
      }
    } catch {
      // silent — never block prompt build
    }
  };
}

export function autoCaptureHook(client: OmemClient) {
  return async (event: AgentEndEvent) => {
    if (!event.success) return;

    try {
      const tail = event.messages.slice(-MAX_CAPTURE_MESSAGES);

      let totalBytes = 0;
      const budgeted: Array<{ role: string; content: string }> = [];
      for (const msg of tail) {
        const cleaned = stripOmemContext(msg.content);
        const size = new TextEncoder().encode(cleaned).length;
        if (totalBytes + size > MAX_CAPTURE_BYTES) break;
        totalBytes += size;
        budgeted.push({ role: msg.role, content: cleaned });
      }

      if (budgeted.length > 0) {
        await client.ingestMessages(budgeted, { mode: "smart" });
      }
    } catch {
      // silent — never block agent teardown
    }
  };
}
