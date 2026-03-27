import type { PluginAPI, ContextEngineConfig } from "./types.js";
import { OmemClient } from "./client.js";
import { OmemMemoryBackend } from "./server-backend.js";
import { OmemContextEngine } from "./context-engine.js";
import { autoRecallHook, autoCaptureHook } from "./hooks.js";
import { buildTools } from "./tools.js";

export default function omemPlugin(api: PluginAPI): void {
  const client = new OmemClient(
    process.env.OMEM_API_URL || "http://localhost:8080",
    process.env.OMEM_API_KEY || "",
  );

  api.registerMemoryBackend(new OmemMemoryBackend(client));

  for (const tool of buildTools(client)) {
    api.registerTool(tool);
  }

  api.registerHook("before_prompt_build", autoRecallHook(client));
  api.on("agent_end", autoCaptureHook(client));

  api.registerContextEngine(
    "omem",
    (config: ContextEngineConfig) => new OmemContextEngine(client, config),
  );
}
