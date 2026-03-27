const DEFAULT_TIMEOUT_MS = 5_000;

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
    return this.post<MemoryDto>("/v1/memories", { content, tags, source });
  }

  async searchMemories(
    query: string,
    limit = 10,
    scope?: string,
    tags?: string[],
  ): Promise<SearchResult[]> {
    const params = new URLSearchParams({ q: query, limit: String(limit) });
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
    return this.post("/v1/memories", {
      messages,
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
}
