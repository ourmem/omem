import type { OmemClient } from "./client.js";
import type { MemorySearchResult, HookApi, Logger } from "./types.js";

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

function extractTextContent(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .filter((block: any) => block?.type === "text" && typeof block?.text === "string")
      .map((block: any) => block.text)
      .join("\n");
  }
  return String(content ?? "");
}

export function registerHooks(api: HookApi, client: OmemClient, logger: Logger): void {

  // before_prompt_build — semantic recall using last user message
  api.on(
    "before_prompt_build",
    async (event: unknown) => {
      try {
        const evt = event as { prompt?: string };
        const query = evt?.prompt?.slice(0, 500) || "*";

        const results = await client.searchMemories(query, MAX_RECALL_RESULTS);
        const block = buildContextBlock(results);
        if (!block) return;

        logger.info(`[ourmem] Injecting ${results.length} memories into prompt context`);
        return { prependContext: block };
      } catch {
        // silent — never block prompt build
      }
    },
    { priority: 50 },
  );

  // agent_end — auto-capture with Claude content block handling
  api.on("agent_end", async (event: unknown, context: unknown) => {
    try {
      const evt = event as {
        success?: boolean;
        messages?: unknown[];
        sessionId?: string;
        agentId?: string;
      };
      const hookCtx = (context ?? {}) as { agentId?: string; sessionId?: string; sessionKey?: string };
      if (!evt?.success || !evt.messages || evt.messages.length === 0) return;

      const tail = evt.messages.slice(-MAX_CAPTURE_MESSAGES);
      let totalBytes = 0;
      const budgeted: Array<{ role: string; content: string }> = [];

      for (const msg of tail) {
        if (!msg || typeof msg !== "object") continue;
        const m = msg as Record<string, unknown>;
        const role = typeof m.role === "string" ? m.role : "";
        if (!role) continue;

        const cleaned = stripOmemContext(extractTextContent(m.content));
        if (!cleaned) continue;

        const size = new TextEncoder().encode(cleaned).byteLength;
        if (totalBytes + size > MAX_CAPTURE_BYTES && budgeted.length > 0) break;
        totalBytes += size;
        budgeted.push({ role, content: cleaned });
      }

      if (budgeted.length === 0) return;

      const sessionId = evt.sessionId ?? hookCtx.sessionId ?? hookCtx.sessionKey ?? `ses_${Date.now()}`;
      const agentId = evt.agentId ?? hookCtx.agentId ?? "openclaw-auto";

      await client.ingestMessages(budgeted, {
        mode: "smart",
        sessionId,
        agentId,
      });

      logger.info(`[ourmem] Captured ${budgeted.length} messages from session`);
    } catch {
      // silent — never block agent teardown
    }
  });

  // before_reset — save key user messages before context wipe
  api.on("before_reset", async (event: unknown) => {
    try {
      const evt = event as { messages?: unknown[]; reason?: string };
      const messages = evt?.messages;
      if (!messages || messages.length === 0) return;

      const userTexts: string[] = [];
      for (const msg of messages) {
        if (!msg || typeof msg !== "object") continue;
        const m = msg as Record<string, unknown>;
        if (m.role !== "user") continue;
        const text = extractTextContent(m.content);
        if (text.length > 10) userTexts.push(text.slice(0, 300));
      }

      if (userTexts.length === 0) return;

      await client.ingestMessages(
        userTexts.slice(-3).map(t => ({ role: "user", content: t })),
        { mode: "smart" },
      );

      logger.info("[ourmem] Session context saved before reset");
    } catch {
      // silent
    }
  });
}
