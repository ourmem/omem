import { tool } from "@opencode-ai/plugin";
import type { OmemClient } from "./client.js";

export function buildTools(client: OmemClient, containerTags: string[]) {
  return {
    memory_store: tool({
      description:
        "Store a new memory in the user's long-term memory. " +
        "Use when the user explicitly asks to remember something, " +
        "or when you identify important preferences, facts, or decisions worth preserving.",
      args: {
        content: tool.schema.string().describe("The information to remember"),
        tags: tool.schema
          .array(tool.schema.string())
          .optional()
          .describe("Optional categorization tags"),
        source: tool.schema
          .string()
          .optional()
          .describe("Origin context, e.g. 'conversation', 'code-review'"),
      },
      async execute(args) {
        const allTags = [...containerTags, ...(args.tags ?? [])];
        const result = await client.createMemory(
          args.content,
          allTags,
          args.source,
        );
        if (!result) return "Failed to store memory. The omem server may be unavailable.";
        return `Memory stored (id: ${result.id}). Tags: [${result.tags.join(", ")}]`;
      },
    }),

    memory_search: tool({
      description:
        "Search the user's long-term memory by semantic similarity. " +
        "Use to recall previously stored preferences, facts, or context.",
      args: {
        query: tool.schema.string().describe("Natural-language search query"),
        limit: tool.schema
          .number()
          .optional()
          .describe("Max results to return (default 10)"),
        scope: tool.schema
          .string()
          .optional()
          .describe("Optional scope filter"),
      },
      async execute(args) {
        const results = await client.searchMemories(
          args.query,
          args.limit ?? 10,
          args.scope,
          containerTags,
        );
        if (results.length === 0) return "No matching memories found.";
        const lines = results.map(
          (r, i) =>
            `${i + 1}. [${r.score.toFixed(2)}] (id: ${r.memory.id}) ${r.memory.content.slice(0, 200)}`,
        );
        return lines.join("\n");
      },
    }),

    memory_get: tool({
      description: "Retrieve a specific memory by its ID.",
      args: {
        id: tool.schema.string().describe("Memory ID"),
      },
      async execute(args) {
        const memory = await client.getMemory(args.id);
        if (!memory) return `Memory ${args.id} not found.`;
        return JSON.stringify(memory, null, 2);
      },
    }),

    memory_update: tool({
      description:
        "Update the content or tags of an existing memory. " +
        "Use when information needs correction or enrichment.",
      args: {
        id: tool.schema.string().describe("Memory ID to update"),
        content: tool.schema.string().describe("New content"),
        tags: tool.schema
          .array(tool.schema.string())
          .optional()
          .describe("Replacement tags"),
      },
      async execute(args) {
        const result = await client.updateMemory(
          args.id,
          args.content,
          args.tags,
        );
        if (!result) return `Failed to update memory ${args.id}.`;
        return `Memory ${args.id} updated.`;
      },
    }),

    memory_delete: tool({
      description:
        "Delete a memory by ID. Use when the user asks to forget something.",
      args: {
        id: tool.schema.string().describe("Memory ID to delete"),
      },
      async execute(args) {
        await client.deleteMemory(args.id);
        return `Memory ${args.id} deleted.`;
      },
    }),
  };
}
