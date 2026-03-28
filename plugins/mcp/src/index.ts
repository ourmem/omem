#!/usr/bin/env node
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { createClient } from "./client.js";
import { registerTools, registerResources } from "./tools.js";

const server = new McpServer({
  name: "omem",
  version: "0.2.0",
});

const client = createClient();

registerTools(server, client);
registerResources(server, client);

async function main(): Promise<void> {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error("[omem-mcp] Server running on stdio");
}

main().catch((err) => {
  console.error("[omem-mcp] Fatal:", err);
  process.exit(1);
});
