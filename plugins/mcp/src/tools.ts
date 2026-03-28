import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { OmemClient } from "./client.js";

export function registerTools(server: McpServer, client: OmemClient): void {
  server.registerTool(
    "memory_store",
    {
      title: "Store Memory",
      description:
        "Store a new memory in omem. Use this to save important information, decisions, preferences, or context for future reference.",
      inputSchema: {
        content: z.string().describe("The content to remember"),
        tags: z
          .array(z.string())
          .optional()
          .describe("Tags to categorize the memory"),
        source: z
          .string()
          .optional()
          .describe("Source identifier (e.g. 'chat', 'code-review')"),
      },
    },
    async ({ content, tags, source }) => {
      try {
        const memory = await client.createMemory(
          content,
          tags ?? [],
          source ?? "mcp",
        );
        return {
          content: [
            {
              type: "text" as const,
              text: `Memory stored (id: ${memory.id}):\n${memory.content}`,
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to store memory: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "memory_search",
    {
      title: "Search Memories",
      description:
        "Search stored memories by semantic query. Returns the most relevant memories ranked by similarity.",
      inputSchema: {
        query: z.string().describe("Search query"),
        limit: z
          .number()
          .int()
          .min(1)
          .max(50)
          .optional()
          .describe("Max results to return (default: 10)"),
        scope: z
          .string()
          .optional()
          .describe("Scope filter for the search"),
        tags: z
          .array(z.string())
          .optional()
          .describe("Filter by tags"),
      },
    },
    async ({ query, limit, scope, tags }) => {
      try {
        const results = await client.searchMemories(
          query,
          limit ?? 10,
          scope,
          tags,
        );

        if (results.length === 0) {
          return {
            content: [
              { type: "text" as const, text: "No memories found." },
            ],
          };
        }

        const formatted = results
          .map((r, i) => {
            const tags =
              r.memory.tags.length > 0
                ? ` [${r.memory.tags.join(", ")}]`
                : "";
            return `${i + 1}. (score: ${r.score.toFixed(2)})${tags}\n   ${r.memory.content}`;
          })
          .join("\n\n");

        return {
          content: [{ type: "text" as const, text: formatted }],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Search failed: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "memory_forget",
    {
      title: "Forget Memory",
      description: "Delete a specific memory by its ID.",
      inputSchema: {
        id: z.string().describe("The memory ID to delete"),
      },
    },
    async ({ id }) => {
      try {
        await client.deleteMemory(id);
        return {
          content: [
            {
              type: "text" as const,
              text: `Memory ${id} deleted.`,
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to delete memory: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "memory_get",
    {
      title: "Get Memory",
      description: "Retrieve a specific memory by its ID.",
      inputSchema: {
        id: z.string().describe("The memory ID to retrieve"),
      },
    },
    async ({ id }) => {
      try {
        const memory = await client.getMemory(id);
        if (!memory) {
          return {
            content: [
              {
                type: "text" as const,
                text: `Memory ${id} not found.`,
              },
            ],
          };
        }
        return {
          content: [
            {
              type: "text" as const,
              text: JSON.stringify(memory, null, 2),
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to get memory: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "memory_update",
    {
      title: "Update Memory",
      description:
        "Update the content or tags of an existing memory. Use when information needs correction or enrichment.",
      inputSchema: {
        id: z.string().describe("The memory ID to update"),
        content: z.string().describe("New content for the memory"),
        tags: z
          .array(z.string())
          .optional()
          .describe("Replacement tags for the memory"),
      },
    },
    async ({ id, content, tags }) => {
      try {
        const memory = await client.updateMemory(id, content, tags);
        if (!memory) {
          return {
            content: [
              {
                type: "text" as const,
                text: `Failed to update memory ${id}.`,
              },
            ],
          };
        }
        return {
          content: [
            {
              type: "text" as const,
              text: `Memory ${id} updated.`,
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to update memory: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "memory_profile",
    {
      title: "User Profile",
      description:
        "Get the user profile synthesized from stored memories. Shows preferences, patterns, and key information.",
      inputSchema: {},
    },
    async () => {
      try {
        const profile = await client.getProfile();
        return {
          content: [
            {
              type: "text" as const,
              text: JSON.stringify(profile, null, 2),
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to get profile: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "memory_list",
    {
      title: "List Recent Memories",
      description:
        "List the most recent memories. Use to browse what's been remembered without a search query.",
      inputSchema: {
        limit: z
          .number()
          .int()
          .min(1)
          .max(100)
          .optional()
          .describe("Max memories to return (default: 20)"),
      },
    },
    async ({ limit }) => {
      try {
        const memories = await client.listRecent(limit ?? 20);
        if (memories.length === 0) {
          return {
            content: [
              { type: "text" as const, text: "No memories stored yet." },
            ],
          };
        }
        const formatted = memories
          .map((m, i) => {
            const tags =
              m.tags.length > 0 ? ` [${m.tags.join(", ")}]` : "";
            return `${i + 1}. (${m.category})${tags} ${m.content.slice(0, 120)}`;
          })
          .join("\n");
        return {
          content: [{ type: "text" as const, text: formatted }],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to list memories: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "memory_ingest",
    {
      title: "Ingest Conversation",
      description:
        "Ingest conversation messages for intelligent extraction. The system extracts atomic facts, deduplicates, and reconciles with existing memories.",
      inputSchema: {
        messages: z
          .array(
            z.object({
              role: z
                .string()
                .describe("Message role: user, assistant, or system"),
              content: z.string().describe("Message content"),
            }),
          )
          .describe("Conversation messages to ingest"),
        mode: z
          .enum(["smart", "raw"])
          .optional()
          .describe(
            "Extraction mode: 'smart' (LLM extraction, default) or 'raw' (store as-is)",
          ),
        tags: z
          .array(z.string())
          .optional()
          .describe("Tags to apply to extracted memories"),
      },
    },
    async ({ messages, mode, tags }) => {
      try {
        const result = await client.ingestMessages(messages, {
          mode: mode ?? "smart",
          tags,
        });
        return {
          content: [
            {
              type: "text" as const,
              text: `Ingestion complete: ${JSON.stringify(result)}`,
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Ingestion failed: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "memory_stats",
    {
      title: "Memory Statistics",
      description:
        "Get statistics about stored memories — counts by category, type, tier, and timeline.",
      inputSchema: {},
    },
    async () => {
      try {
        const stats = await client.getStats();
        return {
          content: [
            {
              type: "text" as const,
              text: JSON.stringify(stats, null, 2),
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to get stats: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );
}

export function registerResources(
  server: McpServer,
  client: OmemClient,
): void {
  server.registerResource(
    "user-profile",
    "omem://profile",
    {
      title: "User Profile",
      description:
        "User profile synthesized from stored memories — preferences, patterns, and key information.",
      mimeType: "application/json",
    },
    async () => {
      try {
        const profile = await client.getProfile();
        return {
          contents: [
            {
              uri: "omem://profile",
              mimeType: "application/json",
              text: JSON.stringify(profile, null, 2),
            },
          ],
        };
      } catch {
        return {
          contents: [
            {
              uri: "omem://profile",
              mimeType: "application/json",
              text: JSON.stringify({ error: "Failed to load profile" }),
            },
          ],
        };
      }
    },
  );
}
