/**
 * OpenClaw plugin SDK type stubs.
 *
 * These types model the openclaw/plugin-sdk v2026.3.22+ surface.
 * The actual SDK is provided by the OpenClaw host runtime — we only
 * declare the shapes we consume so the plugin compiles stand-alone.
 */

// ---------------------------------------------------------------------------
// Plugin config (from openclaw.json entries)
// ---------------------------------------------------------------------------

export interface PluginConfig {
  apiUrl?: string;
  apiKey?: string;
}

// ---------------------------------------------------------------------------
// Tool definition types (OpenClaw format)
// ---------------------------------------------------------------------------

export interface AnyAgentTool {
  name: string;
  label: string;
  description: string;
  parameters: {
    type: "object";
    properties: Record<string, unknown>;
    required: string[];
  };
  execute: (_id: string, params: unknown) => Promise<unknown>;
}

export interface ToolContext {
  workspaceDir?: string;
  agentId?: string;
  sessionKey?: string;
  messageChannel?: string;
}

export type ToolFactory = (ctx?: ToolContext) => AnyAgentTool | AnyAgentTool[] | null | undefined;

// ---------------------------------------------------------------------------
// Hook types
// ---------------------------------------------------------------------------

export interface HookApi {
  on: (hookName: string, handler: (...args: unknown[]) => unknown, opts?: { priority?: number }) => void;
}

export interface Logger {
  info: (...args: unknown[]) => void;
  error: (...args: unknown[]) => void;
}

// ---------------------------------------------------------------------------
// OpenClawPluginApi — the main handle passed to plugin register()
// ---------------------------------------------------------------------------

export interface OpenClawPluginApi {
  pluginConfig?: unknown;
  logger: Logger;
  registerTool: (
    factory: ToolFactory | (() => AnyAgentTool[]),
    opts: { names: string[] },
  ) => void;
  on: (hookName: string, handler: (...args: unknown[]) => unknown, opts?: { priority?: number }) => void;
}

// ---------------------------------------------------------------------------
// Memory types (used by client + hooks)
// ---------------------------------------------------------------------------

export interface MemoryRecord {
  id: string;
  content: string;
  l2_content?: string;
  category: string;
  memory_type: string;
  state: string;
  tags: string[];
  source?: string;
  tenant_id: string;
  agent_id?: string;
  created_at: string;
  updated_at: string;
}

export interface MemorySearchResult {
  memory: MemoryRecord;
  score: number;
}

// ---------------------------------------------------------------------------
// ContextEngine interface — 7 lifecycle hooks (for future use)
// ---------------------------------------------------------------------------

export interface Turn {
  userMessage: string;
  assistantMessage: string;
}

export interface SubagentParentContext {
  task: string;
  [key: string]: unknown;
}

export interface SubagentResult {
  summary?: string;
  [key: string]: unknown;
}

export interface ContextEngine {
  bootstrap(): Promise<void>;
  ingest(message: { role: string; content: string }): Promise<void>;
  assemble(budget: number): Promise<string>;
  compact(): Promise<void>;
  afterTurn(turn: Turn): Promise<void>;
  prepareSubagentSpawn(parentContext: SubagentParentContext): Promise<{ memories: unknown[] }>;
  onSubagentEnded(result: SubagentResult): Promise<void>;
}

export type ContextEngineFactory = (config: ContextEngineConfig) => ContextEngine;

export interface ContextEngineConfig {
  profileFrequency?: number;
  maxRecallResults?: number;
  tokenBudget?: number;
  [key: string]: unknown;
}

// ---------------------------------------------------------------------------
// Memory slot types (kept for ContextEngine / server-backend)
// ---------------------------------------------------------------------------

export interface MemoryBackend {
  kind: "memory";
  name: string;
  description: string;

  store(content: string, tags?: string[], source?: string): Promise<MemoryRecord | null>;
  search(query: string, limit?: number): Promise<MemorySearchResult[]>;
  get(id: string): Promise<MemoryRecord | null>;
  update(id: string, content: string, tags?: string[]): Promise<MemoryRecord | null>;
  delete(id: string): Promise<void>;
  list(limit?: number, offset?: number): Promise<MemoryRecord[]>;
}
