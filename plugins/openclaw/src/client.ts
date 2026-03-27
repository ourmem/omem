import type { MemoryRecord, MemorySearchResult } from "./types.js";

const DEFAULT_TIMEOUT_MS = 5_000;

interface IngestOptions {
  mode?: "smart" | "raw";
  agentId?: string;
  sessionId?: string;
  entityContext?: string;
}

interface SearchResponse {
  results: MemorySearchResult[];
  trace?: unknown;
}

interface ListResponse {
  memories: MemoryRecord[];
  limit: number;
  offset: number;
}

interface ProfileResponse {
  profile: string;
  [key: string]: unknown;
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

  async healthCheck(): Promise<boolean> {
    const res = await this.request<{ status: string }>("/health");
    return res !== null;
  }

  async createMemory(
    content: string,
    tags?: string[],
    source?: string,
  ): Promise<MemoryRecord | null> {
    return this.post<MemoryRecord>("/v1/memories", { content, tags, source });
  }

  async searchMemories(
    query: string,
    limit = 10,
    scope?: string,
  ): Promise<MemorySearchResult[]> {
    const params = new URLSearchParams({ q: query, limit: String(limit) });
    if (scope) params.set("scope", scope);
    const res = await this.request<SearchResponse>(
      `/v1/memories/search?${params}`,
    );
    return res?.results ?? [];
  }

  async getMemory(id: string): Promise<MemoryRecord | null> {
    return this.request<MemoryRecord>(`/v1/memories/${encodeURIComponent(id)}`);
  }

  async updateMemory(
    id: string,
    content: string,
    tags?: string[],
  ): Promise<MemoryRecord | null> {
    return this.put<MemoryRecord>(
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
    });
  }

  async getProfile(): Promise<string> {
    const res = await this.request<ProfileResponse>("/v1/profile");
    return res?.profile ?? "";
  }

  async listRecent(limit = 20): Promise<MemoryRecord[]> {
    const res = await this.request<ListResponse>(
      `/v1/memories?limit=${limit}&offset=0`,
    );
    return res?.memories ?? [];
  }
}
