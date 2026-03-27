import type { OmemClient } from "./client.js";
import type { ToolDefinition } from "./types.js";

export function buildTools(client: OmemClient): ToolDefinition[] {
  return [
    {
      name: "memory_store",
      description:
        "Store a new memory in the user's long-term memory. " +
        "Use when the user explicitly asks to remember something, " +
        "or when you identify important preferences, facts, or decisions worth preserving.",
      parameters: {
        content: { type: "string", description: "The information to remember", required: true },
        tags: { type: "array", description: "Optional categorization tags", items: { type: "string" } },
        source: { type: "string", description: "Origin context, e.g. 'conversation', 'code-review'" },
      },
      async execute(args) {
        const result = await client.createMemory(
          args.content as string,
          args.tags as string[] | undefined,
          args.source as string | undefined,
        );
        if (!result) return "Failed to store memory. The omem server may be unavailable.";
        return `Memory stored (id: ${result.id}). Tags: [${result.tags.join(", ")}]`;
      },
    },

    {
      name: "memory_search",
      description:
        "Search the user's long-term memory by semantic similarity. " +
        "Use to recall previously stored preferences, facts, or context.",
      parameters: {
        query: { type: "string", description: "Natural-language search query", required: true },
        limit: { type: "number", description: "Max results to return (default 10)" },
        scope: { type: "string", description: "Optional scope filter" },
      },
      async execute(args) {
        const results = await client.searchMemories(
          args.query as string,
          (args.limit as number) ?? 10,
          args.scope as string | undefined,
        );
        if (results.length === 0) return "No matching memories found.";
        const lines = results.map(
          (r, i) =>
            `${i + 1}. [${r.score.toFixed(2)}] (id: ${r.memory.id}) ${r.memory.content.slice(0, 200)}`,
        );
        return lines.join("\n");
      },
    },

    {
      name: "memory_get",
      description: "Retrieve a specific memory by its ID.",
      parameters: {
        id: { type: "string", description: "Memory ID", required: true },
      },
      async execute(args) {
        const memory = await client.getMemory(args.id as string);
        if (!memory) return `Memory ${args.id} not found.`;
        return JSON.stringify(memory, null, 2);
      },
    },

    {
      name: "memory_update",
      description:
        "Update the content or tags of an existing memory. " +
        "Use when information needs correction or enrichment.",
      parameters: {
        id: { type: "string", description: "Memory ID to update", required: true },
        content: { type: "string", description: "New content", required: true },
        tags: { type: "array", description: "Replacement tags", items: { type: "string" } },
      },
      async execute(args) {
        const result = await client.updateMemory(
          args.id as string,
          args.content as string,
          args.tags as string[] | undefined,
        );
        if (!result) return `Failed to update memory ${args.id}.`;
        return `Memory ${args.id} updated.`;
      },
    },

    {
      name: "memory_delete",
      description: "Delete a memory by ID. Use when the user asks to forget something.",
      parameters: {
        id: { type: "string", description: "Memory ID to delete", required: true },
      },
      async execute(args) {
        await client.deleteMemory(args.id as string);
        return `Memory ${args.id} deleted.`;
      },
    },
  ];
}
