import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { OmemClient } from "./client.js";

export function registerTools(server: McpServer, client: OmemClient): void {
  server.registerTool(
    "memory_store",
    {
      title: "Store Memory",
      description:
        "Store a new memory in omem. Use this to save important information, decisions, preferences, or context for future reference. Pass `replaces` to atomically supersede one or more existing memories with this new consolidated one (e.g. when merging chunked fragments into a single record).",
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
        replaces: z
          .array(z.string())
          .optional()
          .describe(
            "Memory IDs to mark as superseded by this new one. Use when consolidating multiple fragments into a single memory. Default search hides superseded entries; get-by-id still returns them for history.",
          ),
      },
    },
    async ({ content, tags, source, replaces }) => {
      try {
        const memory = await client.createMemory(
          content,
          tags ?? [],
          source ?? "mcp",
          replaces,
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
        "Search stored memories by semantic query. Returns the most relevant memories ranked by similarity. By default searches across all spaces the caller has access to (personal + team + organization). Pass `space` to restrict to specific spaces — useful when you want a query scoped to one context (e.g. work-only) or to peek into a non-default space (e.g. 'this question is about something in our team space').",
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
        space: z
          .string()
          .optional()
          .describe(
            "Restrict search to specific spaces. Comma-separated space IDs (e.g. 'personal:doc,team:synchresis-work'), a single ID, or 'all' for every accessible space. Omitted = all accessible spaces.",
          ),
      },
    },
    async ({ query, limit, scope, tags, space }) => {
      try {
        const results = await client.searchMemories(
          query,
          limit ?? 10,
          scope,
          tags,
          space,
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

  server.registerTool(
    "space_create",
    {
      title: "Create Space",
      description:
        "Create a shared space (team or organization) for sharing memories across users and agents.",
      inputSchema: {
        name: z.string().describe("Name of the space"),
        space_type: z
          .enum(["team", "organization"])
          .describe("Type of space: 'team' or 'organization'"),
        members: z
          .array(
            z.object({
              user_id: z.string().describe("User/tenant ID to add"),
              role: z
                .enum(["admin", "member", "reader"])
                .describe("Member role"),
            }),
          )
          .optional()
          .describe("Initial members to add to the space"),
      },
    },
    async ({ name, space_type, members }) => {
      try {
        const space = await client.createSpace(name, space_type, members);
        return {
          content: [
            {
              type: "text" as const,
              text: `Space created:\n${JSON.stringify(space, null, 2)}`,
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to create space: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "space_list",
    {
      title: "List Spaces",
      description:
        "List all spaces you own or are a member of, including personal, team, and organization spaces.",
      inputSchema: {},
    },
    async () => {
      try {
        const spaces = await client.listSpaces();
        if (spaces.length === 0) {
          return {
            content: [
              { type: "text" as const, text: "No spaces found." },
            ],
          };
        }
        return {
          content: [
            {
              type: "text" as const,
              text: JSON.stringify(spaces, null, 2),
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to list spaces: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "space_add_member",
    {
      title: "Add Space Member",
      description:
        "Add a user to an existing shared space with a specified role.",
      inputSchema: {
        space_id: z.string().describe("The space ID to add the member to"),
        user_id: z.string().describe("The user/tenant ID to add"),
        role: z
          .enum(["admin", "member", "reader"])
          .describe("Role for the new member"),
      },
    },
    async ({ space_id, user_id, role }) => {
      try {
        const result = await client.addSpaceMember(space_id, user_id, role);
        return {
          content: [
            {
              type: "text" as const,
              text: `Member added:\n${JSON.stringify(result, null, 2)}`,
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to add member: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "memory_share",
    {
      title: "Share Memory",
      description:
        "Share a memory to a team or organization space. Creates a copy with full provenance tracking and vector embedding.",
      inputSchema: {
        memory_id: z.string().describe("The memory ID to share"),
        target_space: z
          .string()
          .describe("The target space ID to share the memory to"),
      },
    },
    async ({ memory_id, target_space }) => {
      try {
        const result = await client.shareMemory(memory_id, target_space);
        return {
          content: [
            {
              type: "text" as const,
              text: `Memory shared:\n${JSON.stringify(result, null, 2)}`,
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to share memory: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "memory_pull",
    {
      title: "Pull Memory",
      description:
        "Pull a shared memory from a team/organization space into your personal space.",
      inputSchema: {
        memory_id: z.string().describe("The memory ID to pull"),
        source_space: z
          .string()
          .describe("The source space ID to pull the memory from"),
        visibility: z
          .string()
          .optional()
          .describe("Visibility of the pulled copy (optional)"),
      },
    },
    async ({ memory_id, source_space, visibility }) => {
      try {
        const result = await client.pullMemory(
          memory_id,
          source_space,
          visibility,
        );
        return {
          content: [
            {
              type: "text" as const,
              text: `Memory pulled:\n${JSON.stringify(result, null, 2)}`,
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to pull memory: ${(err as Error).message}`,
            },
          ],
          isError: true,
        };
      }
    },
  );

  server.registerTool(
    "memory_reshare",
    {
      title: "Reshare Memory",
      description:
        "Refresh a stale shared copy with the latest content and vector from the source memory.",
      inputSchema: {
        memory_id: z
          .string()
          .describe("The shared copy memory ID to refresh"),
        target_space: z
          .string()
          .optional()
          .describe(
            "Target space containing the copy (optional — searches all spaces if omitted)",
          ),
      },
    },
    async ({ memory_id, target_space }) => {
      try {
        const result = await client.reshareMemory(memory_id, target_space);
        return {
          content: [
            {
              type: "text" as const,
              text: `Memory reshared:\n${JSON.stringify(result, null, 2)}`,
            },
          ],
        };
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to reshare memory: ${(err as Error).message}`,
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
