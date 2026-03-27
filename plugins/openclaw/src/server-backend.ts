import type { OmemClient } from "./client.js";
import type { MemoryBackend, MemoryRecord, MemorySearchResult } from "./types.js";

export class OmemMemoryBackend implements MemoryBackend {
  readonly kind = "memory" as const;
  readonly name = "omem";
  readonly description = "Long-term memory powered by omem-server (semantic search + auto-extraction)";

  constructor(private client: OmemClient) {}

  async store(
    content: string,
    tags?: string[],
    source?: string,
  ): Promise<MemoryRecord | null> {
    return this.client.createMemory(content, tags, source);
  }

  async search(query: string, limit = 10): Promise<MemorySearchResult[]> {
    return this.client.searchMemories(query, limit);
  }

  async get(id: string): Promise<MemoryRecord | null> {
    return this.client.getMemory(id);
  }

  async update(
    id: string,
    content: string,
    tags?: string[],
  ): Promise<MemoryRecord | null> {
    return this.client.updateMemory(id, content, tags);
  }

  async delete(id: string): Promise<void> {
    await this.client.deleteMemory(id);
  }

  async list(limit = 20, _offset = 0): Promise<MemoryRecord[]> {
    return this.client.listRecent(limit);
  }
}
