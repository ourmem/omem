import type { OmemClient } from "./client.js";
import type {
  ContextEngine,
  ContextEngineConfig,
  MemorySearchResult,
  Turn,
  SubagentParentContext,
  SubagentResult,
} from "./types.js";

const DEFAULT_MAX_RECALL = 10;
const DEFAULT_TOKEN_BUDGET = 4096;
const CHARS_PER_TOKEN = 4;

function estimateTokens(text: string): number {
  return Math.ceil(text.length / CHARS_PER_TOKEN);
}

function formatContext(
  profile: string,
  memories: MemorySearchResult[],
  budget: number,
): string {
  const sections: string[] = [];
  let remaining = budget;

  if (profile) {
    const profileBlock = `<user-profile>\n${profile}\n</user-profile>`;
    const cost = estimateTokens(profileBlock);
    if (cost <= remaining) {
      sections.push(profileBlock);
      remaining -= cost;
    }
  }

  if (memories.length > 0) {
    const memLines: string[] = [];
    for (const r of memories) {
      const line = `- [${r.memory.category}] ${r.memory.content}`;
      const cost = estimateTokens(line);
      if (cost > remaining) break;
      memLines.push(line);
      remaining -= cost;
    }
    if (memLines.length > 0) {
      sections.push(`<memories>\n${memLines.join("\n")}\n</memories>`);
    }
  }

  return sections.join("\n\n");
}

export class OmemContextEngine implements ContextEngine {
  private maxRecall: number;
  private tokenBudget: number;

  constructor(
    private client: OmemClient,
    config: ContextEngineConfig,
  ) {
    this.maxRecall = config.maxRecallResults ?? DEFAULT_MAX_RECALL;
    this.tokenBudget = config.tokenBudget ?? DEFAULT_TOKEN_BUDGET;
  }

  async bootstrap(): Promise<void> {
    const ok = await this.client.healthCheck();
    if (!ok) {
      throw new Error("[omem] Failed to connect to omem-server during bootstrap");
    }
  }

  async ingest(message: { role: string; content: string }): Promise<void> {
    await this.client.ingestMessages(
      [{ role: message.role, content: message.content }],
      { mode: "smart" },
    );
  }

  async assemble(budget: number): Promise<string> {
    const effectiveBudget = budget > 0 ? budget : this.tokenBudget;
    const [profile, results] = await Promise.all([
      this.client.getProfile(),
      this.client.searchMemories("", this.maxRecall),
    ]);
    return formatContext(profile, results, effectiveBudget);
  }

  async compact(): Promise<void> {
    // no-op: server-side compaction not yet implemented
  }

  async afterTurn(turn: Turn): Promise<void> {
    const messages: Array<{ role: string; content: string }> = [];
    if (turn.userMessage) {
      messages.push({ role: "user", content: turn.userMessage });
    }
    if (turn.assistantMessage) {
      messages.push({ role: "assistant", content: turn.assistantMessage });
    }
    if (messages.length > 0) {
      await this.client.ingestMessages(messages, { mode: "smart" });
    }
  }

  async prepareSubagentSpawn(
    parentContext: SubagentParentContext,
  ): Promise<{ memories: unknown[] }> {
    const results = await this.client.searchMemories(parentContext.task, 5);
    return { memories: results };
  }

  async onSubagentEnded(result: SubagentResult): Promise<void> {
    if (!result.summary) return;
    await this.client.ingestMessages(
      [{ role: "assistant", content: `[Subagent result] ${result.summary}` }],
      { mode: "smart" },
    );
  }
}
