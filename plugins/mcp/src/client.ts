const DEFAULT_TIMEOUT_MS = 8_000;

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
        const body = await res.text().catch(() => "");
        throw new Error(`${res.status} ${res.statusText}: ${body}`);
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
    const result = await this.request<MemoryDto>("/v1/memories", {
      method: "POST",
      body: JSON.stringify({ content, tags, source }),
    });
    if (!result) throw new Error("Failed to create memory");
    return result;
  }

  async searchMemories(
    query: string,
    limit = 10,
    scope?: string,
  ): Promise<SearchResult[]> {
    const params = new URLSearchParams({ q: query, limit: String(limit) });
    if (scope) params.set("scope", scope);
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
}

export function createClient(): OmemClient {
  const baseUrl = process.env.OMEM_API_URL ?? "http://localhost:8080";
  const apiKey = process.env.OMEM_API_KEY ?? "";

  if (!apiKey) {
    throw new Error("OMEM_API_KEY environment variable is required");
  }

  return new OmemClient(baseUrl, apiKey);
}
