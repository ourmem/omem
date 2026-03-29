import type { PluginConfig, OpenClawPluginApi, ToolFactory } from "./types.js";
import { OmemClient } from "./client.js";
import { registerHooks } from "./hooks.js";
import { buildTools } from "./tools.js";

const DEFAULT_API_URL = "https://api.ourmem.ai";

const toolNames = [
  "memory_store",
  "memory_search",
  "memory_get",
  "memory_update",
  "memory_delete",
];

export default {
  id: "ourmem",
  name: "ourmem",
  description: "Shared persistent memory for AI agents — semantic search + auto-extraction",
  kind: "memory",

  register(api: OpenClawPluginApi) {
    const cfg = (api.pluginConfig ?? {}) as PluginConfig;
    const apiUrl = cfg.apiUrl || process.env.OMEM_API_URL || DEFAULT_API_URL;
    const apiKey = cfg.apiKey || process.env.OMEM_API_KEY || "";

    const client = new OmemClient(apiUrl, apiKey);

    const factory: ToolFactory = () => buildTools(client);
    api.registerTool(factory, { names: toolNames });

    registerHooks(api, client, api.logger);
  },
};
