const DEFAULT_TIMEOUT_MS = 5_000;

const MAX_QUERY_LENGTH = 200; // CJK URL-encodes ~9x; 200 chars → ~1800 bytes < nginx 4K
const MAX_CONTENT_CHARS = 30_000;

function sanitizeContent(text: string, maxLen: number): string {
  let clean = text.replace(/<[\w-]+[^>]*>[\s\S]*?<\/[\w-]+>/g, "");
  clean = clean.replace(/<[\w-]+[^>]*\/>/g, "");
  clean = clean.replace(/\s+/g, " ").trim();
  if (clean.length <= maxLen) return clean;
  return clean.slice(0, maxLen) + "…[truncated]";
}

function truncateQuery(query: string): string {
  if (query.length <= MAX_QUERY_LENGTH) return query;
  return query.slice(0, MAX_QUERY_LENGTH);
}

export interface IngestOptions {
  mode?: "smart" | "raw";
  agentId?: string;
  sessionId?: string;
  entityContext?: string;
  tags?: string[];
}

export interface SearchResult {
  memory: MemoryDto;
  score: number;
}

export interface SearchResponse {
  results: SearchResult[];
  trace?: unknown;
}

export interface ListResponse {
  memories: MemoryDto[];
  limit: number;
  offset: number;
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

export class OmemClient {
  constructor(
    private baseUrl: string,
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
    const timeout = setTimeout(
      () => controller.abort(),
      DEFAULT_TIMEOUT_MS,
    );

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
        console.warn(
          `[omem] ${init.method ?? "GET"} ${path} → ${res.status} ${res.statusText}`,
        );
        return null;
      }

      if (res.status === 204) return null;

      return (await res.json()) as T;
    } catch (err) {
      if ((err as Error).name === "AbortError") {
        console.warn(`[omem] ${init.method ?? "GET"} ${path} timed out`);
      } else {
        console.warn(`[omem] ${init.method ?? "GET"} ${path} failed:`, err);
      }
      return null;
    } finally {
      clearTimeout(timeout);
    }
  }

  private post<T>(path: string, body: unknown): Promise<T | null> {
    return this.request<T>(path, {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  private put<T>(path: string, body: unknown): Promise<T | null> {
    return this.request<T>(path, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  private del<T>(path: string): Promise<T | null> {
    return this.request<T>(path, { method: "DELETE" });
  }

  async createMemory(
    content: string,
    tags?: string[],
    source?: string,
  ): Promise<MemoryDto | null> {
    const safeContent = sanitizeContent(content, MAX_CONTENT_CHARS);
    return this.post<MemoryDto>("/v1/memories", { content: safeContent, tags, source });
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

  async getMemory(id: string): Promise<MemoryDto | null> {
    return this.request<MemoryDto>(`/v1/memories/${encodeURIComponent(id)}`);
  }

  async updateMemory(
    id: string,
    content: string,
    tags?: string[],
  ): Promise<MemoryDto | null> {
    return this.put<MemoryDto>(
      `/v1/memories/${encodeURIComponent(id)}`,
      { content, tags },
    );
  }

  async deleteMemory(id: string): Promise<void> {
    await this.del(`/v1/memories/${encodeURIComponent(id)}`);
  }

  async ingestMessages(
    messages: Array<{ role: string; content: string }>,
    opts: IngestOptions = {},
  ): Promise<unknown> {
    const safeMessages = messages.map(m => ({
      role: m.role,
      content: sanitizeContent(m.content, MAX_CONTENT_CHARS),
    }));
    return this.post("/v1/memories", {
      messages: safeMessages,
      mode: opts.mode ?? "smart",
      agent_id: opts.agentId,
      session_id: opts.sessionId,
      entity_context: opts.entityContext,
      tags: opts.tags,
    });
  }

  async getProfile(_query?: string): Promise<unknown> {
    return this.request("/v1/profile");
  }

  async listRecent(limit = 20): Promise<MemoryDto[]> {
    const res = await this.request<ListResponse>(
      `/v1/memories?limit=${limit}&offset=0`,
    );
    return res?.memories ?? [];
  }

  async createSpace(
    name: string,
    spaceType: string,
    members?: Array<{ user_id: string; role: string }>,
  ): Promise<unknown> {
    return this.post("/v1/spaces", { name, space_type: spaceType, members });
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
    return this.post(
      `/v1/spaces/${encodeURIComponent(spaceId)}/members`,
      { user_id: userId, role },
    );
  }

  async shareMemory(
    memoryId: string,
    targetSpace: string,
  ): Promise<unknown> {
    return this.post(
      `/v1/memories/${encodeURIComponent(memoryId)}/share`,
      { target_space: targetSpace },
    );
  }

  async pullMemory(
    memoryId: string,
    sourceSpace: string,
    visibility?: string,
  ): Promise<unknown> {
    return this.post(
      `/v1/memories/${encodeURIComponent(memoryId)}/pull`,
      { source_space: sourceSpace, visibility },
    );
  }

  async reshareMemory(
    memoryId: string,
    targetSpace?: string,
  ): Promise<unknown> {
    return this.post(
      `/v1/memories/${encodeURIComponent(memoryId)}/reshare`,
      { target_space: targetSpace },
    );
  }
}
