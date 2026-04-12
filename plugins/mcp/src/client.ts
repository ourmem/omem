const DEFAULT_TIMEOUT_MS = 8_000;

// ── Safety limits (Qwen3-Embedding-0.6B max context ~32K tokens) ──
// NOTE: CJK chars URL-encode ~9x, so 200 chars → ~1800 bytes (safe under nginx 4K buffer)
const MAX_QUERY_LENGTH = 200;       // search query — lowered to prevent 414
const MAX_CONTENT_CHARS = 30_000;   // ~15K tokens, safe margin under 32K
const MAX_ERROR_BODY_LENGTH = 200;  // prevent flooding OpenCode window

/** Strip XML-like tags and their content, then truncate to maxLen chars. */
function sanitizeContent(text: string, maxLen: number): string {
  // Remove XML-tag blocks: <tag ...>...</tag> or <tag .../> (greedy, multi-line)
  let clean = text.replace(/<[\w-]+[^>]*>[\s\S]*?<\/[\w-]+>/g, "");
  // Remove self-closing tags
  clean = clean.replace(/<[\w-]+[^>]*\/>/g, "");
  // Collapse whitespace
  clean = clean.replace(/\s+/g, " ").trim();
  if (clean.length <= maxLen) return clean;
  return clean.slice(0, maxLen) + "…[truncated]";
}

/** Truncate a search query to prevent 414 errors. */
function truncateQuery(query: string): string {
  if (query.length <= MAX_QUERY_LENGTH) return query;
  return query.slice(0, MAX_QUERY_LENGTH);
}

/** Build a short, safe error message from HTTP response body. */
async function safeErrorMessage(status: number, statusText: string, res: Response): Promise<string> {
  const body = await res.text().catch(() => "");
  const snippet = body.slice(0, MAX_ERROR_BODY_LENGTH);
  return `${status} ${statusText}${snippet ? ": " + snippet : ""}`;
}

export interface MemoryDto {
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

export interface SearchResult {
  memory: MemoryDto;
  score: number;
}

interface SearchResponse {
  results: SearchResult[];
}

export class OmemClient {
  private baseUrl: string;

  constructor(
    baseUrl: string,
    private apiKey: string,
  ) {
    this.baseUrl = baseUrl.replace(/\/+$/, "");
  }

  private async request<T>(
    path: string,
    init: RequestInit = {},
  ): Promise<T | null> {
    const url = `${this.baseUrl}${path}`;
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), DEFAULT_TIMEOUT_MS);

    try {
      const res = await fetch(url, {
        ...init,
        signal: controller.signal,
        headers: {
          "Content-Type": "application/json",
          "X-API-Key": this.apiKey,
          ...(init.headers as Record<string, string>),
        },
      });

      if (!res.ok) {
        throw new Error(await safeErrorMessage(res.status, res.statusText, res));
      }

      if (res.status === 204) return null;
      return (await res.json()) as T;
    } finally {
      clearTimeout(timeout);
    }
  }

  async createMemory(
    content: string,
    tags?: string[],
    source?: string,
  ): Promise<MemoryDto> {
    const safeContent = sanitizeContent(content, MAX_CONTENT_CHARS);
    const result = await this.request<MemoryDto>("/v1/memories", {
      method: "POST",
      body: JSON.stringify({ content: safeContent, tags, source }),
    });
    if (!result) throw new Error("Failed to create memory");
    return result;
  }

  async searchMemories(
    query: string,
    limit = 10,
    scope?: string,
    tags?: string[],
  ): Promise<SearchResult[]> {
    const safeQ = truncateQuery(query);
    const params = new URLSearchParams({ q: safeQ, limit: String(limit) });
    if (scope) params.set("scope", scope);
    if (tags && tags.length > 0) params.set("tags", tags.join(","));
    const res = await this.request<SearchResponse>(
      `/v1/memories/search?${params}`,
    );
    return res?.results ?? [];
  }

  async deleteMemory(id: string): Promise<void> {
    await this.request(`/v1/memories/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  async getMemory(id: string): Promise<MemoryDto | null> {
    return this.request<MemoryDto>(
      `/v1/memories/${encodeURIComponent(id)}`,
    );
  }

  async updateMemory(
    id: string,
    content: string,
    tags?: string[],
  ): Promise<MemoryDto | null> {
    return this.request<MemoryDto>(
      `/v1/memories/${encodeURIComponent(id)}`,
      {
        method: "PUT",
        body: JSON.stringify({ content, tags }),
      },
    );
  }

  async getProfile(): Promise<unknown> {
    return this.request("/v1/profile");
  }

  async listRecent(limit = 20): Promise<MemoryDto[]> {
    const res = await this.request<{ memories: MemoryDto[] }>(
      `/v1/memories?limit=${limit}&offset=0`,
    );
    return res?.memories ?? [];
  }

  async ingestMessages(
    messages: Array<{ role: string; content: string }>,
    opts: { mode?: string; agentId?: string; sessionId?: string; tags?: string[] } = {},
  ): Promise<unknown> {
    const safeMessages = messages.map(m => ({
      role: m.role,
      content: sanitizeContent(m.content, MAX_CONTENT_CHARS),
    }));
    return this.request("/v1/memories", {
      method: "POST",
      body: JSON.stringify({
        messages: safeMessages,
        mode: opts.mode ?? "smart",
        agent_id: opts.agentId,
        session_id: opts.sessionId,
        tags: opts.tags,
      }),
    });
  }

  async getStats(): Promise<unknown> {
    return this.request("/v1/stats");
  }

  // ── Sharing ──────────────────────────────────────────────

  async createSpace(
    name: string,
    spaceType: string,
    members?: Array<{ user_id: string; role: string }>,
  ): Promise<unknown> {
    const result = await this.request("/v1/spaces", {
      method: "POST",
      body: JSON.stringify({ name, space_type: spaceType, members }),
    });
    if (!result) throw new Error("Failed to create space");
    return result;
  }

  async listSpaces(): Promise<unknown[]> {
    const res = await this.request<{ spaces: unknown[] }>("/v1/spaces");
    return res?.spaces ?? [];
  }

  async addSpaceMember(
    spaceId: string,
    userId: string,
    role: string,
  ): Promise<unknown> {
    const result = await this.request(
      `/v1/spaces/${encodeURIComponent(spaceId)}/members`,
      {
        method: "POST",
        body: JSON.stringify({ user_id: userId, role }),
      },
    );
    if (!result) throw new Error("Failed to add member");
    return result;
  }

  async shareMemory(
    memoryId: string,
    targetSpace: string,
  ): Promise<unknown> {
    const result = await this.request(
      `/v1/memories/${encodeURIComponent(memoryId)}/share`,
      {
        method: "POST",
        body: JSON.stringify({ target_space: targetSpace }),
      },
    );
    if (!result) throw new Error("Failed to share memory");
    return result;
  }

  async pullMemory(
    memoryId: string,
    sourceSpace: string,
    visibility?: string,
  ): Promise<unknown> {
    const result = await this.request(
      `/v1/memories/${encodeURIComponent(memoryId)}/pull`,
      {
        method: "POST",
        body: JSON.stringify({ source_space: sourceSpace, visibility }),
      },
    );
    if (!result) throw new Error("Failed to pull memory");
    return result;
  }

  async reshareMemory(
    memoryId: string,
    targetSpace?: string,
  ): Promise<unknown> {
    const result = await this.request(
      `/v1/memories/${encodeURIComponent(memoryId)}/reshare`,
      {
        method: "POST",
        body: JSON.stringify({ target_space: targetSpace }),
      },
    );
    if (!result) throw new Error("Failed to reshare memory");
    return result;
  }
}

export function createClient(): OmemClient {
  const baseUrl = process.env.OMEM_API_URL ?? "http://localhost:8080";
  const apiKey = process.env.OMEM_API_KEY ?? "";

  if (!apiKey) {
    throw new Error("OMEM_API_KEY environment variable is required");
  }

  return new OmemClient(baseUrl, apiKey);
}
