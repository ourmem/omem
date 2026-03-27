/**
 * OpenClaw plugin SDK type stubs.
 *
 * These types model the openclaw/plugin-sdk v2026.3.22+ surface.
 * The actual SDK is provided by the OpenClaw host runtime — we only
 * declare the shapes we consume so the plugin compiles stand-alone.
 */

// ---------------------------------------------------------------------------
// Tool definition types
// ---------------------------------------------------------------------------

export interface ToolParameter {
  type: "string" | "number" | "boolean" | "array" | "object";
  description: string;
  required?: boolean;
  items?: { type: string };
}

export interface ToolDefinition {
  name: string;
  description: string;
  parameters: Record<string, ToolParameter>;
  execute: (args: Record<string, unknown>) => Promise<string>;
}

// ---------------------------------------------------------------------------
// Hook types
// ---------------------------------------------------------------------------

export interface PromptBuildContext {
  sessionId: string;
  system: string[];
  messages: Array<{ role: string; content: string }>;
}

export interface AgentEndEvent {
  sessionId: string;
  success: boolean;
  messages: Array<{ role: string; content: string }>;
}

export type HookHandler<TInput = unknown> = (input: TInput) => Promise<void>;

// ---------------------------------------------------------------------------
// ContextEngine interface — 7 lifecycle hooks
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
  /** Connect to backend, verify credentials. */
  bootstrap(): Promise<void>;

  /** Receive a new message for processing. */
  ingest(message: { role: string; content: string }): Promise<void>;

  /** Build context string within token budget. */
  assemble(budget: number): Promise<string>;

  /** Compress / consolidate stored memories. */
  compact(): Promise<void>;

  /** Process a completed conversation turn. */
  afterTurn(turn: Turn): Promise<void>;

  /** Prepare context for a subagent about to spawn. */
  prepareSubagentSpawn(parentContext: SubagentParentContext): Promise<{ memories: unknown[] }>;

  /** Handle results from a completed subagent. */
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
// Memory slot types
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
// PluginAPI — the main handle passed to plugin entry
// ---------------------------------------------------------------------------

export interface PluginAPI {
  /** Register a tool for the agent to use. */
  registerTool(tool: ToolDefinition): void;

  /** Register a lifecycle hook. */
  registerHook<T = unknown>(hookName: string, handler: HookHandler<T>): void;

  /** Register an event listener (alternative to registerHook for some events). */
  on<T = unknown>(eventName: string, handler: HookHandler<T>): void;

  /** Register a memory backend (kind: "memory" slot). */
  registerMemoryBackend(backend: MemoryBackend): void;

  /** Register a ContextEngine factory. */
  registerContextEngine(name: string, factory: ContextEngineFactory): void;
}
