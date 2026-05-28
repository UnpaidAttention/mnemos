import type {
  AuditEntry, Entity, EntityDetail, Graph, Memory, PipelineStatus, RecallHit, SearchReq, Tier,
} from "./types";
import { getToken } from "./token";

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
    this.name = "ApiError";
  }
}

export class MnemosClient {
  constructor(
    private baseUrl = "http://localhost:7423",
    private tokenFn: () => Promise<string> = async () => "dev-token",
  ) {}

  private async req<T>(method: string, path: string, body?: unknown): Promise<T> {
    const token = await this.tokenFn();
    const res = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers: {
        authorization: `Bearer ${token}`,
        ...(body !== undefined ? { "content-type": "application/json" } : {}),
      },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    if (!res.ok) {
      let msg = res.statusText;
      try {
        const j = await res.json();
        msg = (j as { error?: string }).error ?? msg;
      } catch { /* ignore */ }
      throw new ApiError(res.status, msg);
    }
    return (await res.json()) as T;
  }

  async listMemories(q: { tier?: Tier[]; workspace?: string; include_invalid?: boolean; limit?: number } = {}): Promise<Memory[]> {
    const p = new URLSearchParams();
    q.tier?.forEach((t) => p.append("tier", t));
    if (q.workspace) p.set("workspace", q.workspace);
    if (q.include_invalid) p.set("include_invalid", "true");
    p.set("limit", String(q.limit ?? 50));
    return (await this.req<{ memories: Memory[] }>("GET", `/v1/memories?${p}`)).memories;
  }
  getMemory(id: string) { return this.req<Memory>("GET", `/v1/memories/${id}`); }
  createMemory(m: { body: string; title?: string; tier?: Tier; kind?: string; tags?: string[]; importance?: number; workspace?: string }) {
    return this.req<{ id: string }>("POST", "/v1/memories", m);
  }
  patchMemory(id: string, patch: { tags?: string[]; importance?: number }) {
    return this.req<Memory>("PATCH", `/v1/memories/${id}`, patch);
  }
  promoteMemory(id: string, tier: Tier) {
    return this.req<Memory>("POST", `/v1/memories/${id}/promote`, { tier });
  }
  forgetMemory(id: string, reason?: string) {
    return this.req<{ id: string; status: string }>("DELETE", `/v1/memories/${id}${reason ? `?reason=${encodeURIComponent(reason)}` : ""}`);
  }
  async search(req: SearchReq): Promise<RecallHit[]> {
    return (await this.req<{ hits: RecallHit[] }>("POST", "/v1/memories/search", req)).hits;
  }
  async timeTravel(query: string, as_of: string, k = 10): Promise<Memory[]> {
    return (await this.req<{ memories: Memory[] }>("POST", "/v1/memories/time-travel", { query, as_of, k })).memories;
  }
  async audit(id: string): Promise<AuditEntry[]> {
    return (await this.req<{ entries: AuditEntry[] }>("GET", `/v1/memories/${id}/audit`)).entries;
  }
  async listReflections(limit = 50): Promise<Memory[]> {
    return (await this.req<{ reflections: Memory[] }>("GET", `/v1/reflections?limit=${limit}`)).reflections;
  }
  async reflect(): Promise<string[]> {
    return (await this.req<{ created: string[] }>("POST", "/v1/reflections", {})).created;
  }
  pipelines() { return this.req<PipelineStatus>("GET", "/v1/pipelines"); }
  runDecay() { return this.req<{ scanned: number; decayed: number; invalidated: number }>("POST", "/v1/maintenance/decay", {}); }
  runCommunities() { return this.req<{ summaries: string[] }>("POST", "/v1/maintenance/communities", {}); }
  async listEntities(limit = 100): Promise<Entity[]> {
    return (await this.req<{ entities: Entity[] }>("GET", `/v1/entities?limit=${limit}`)).entities;
  }
  getEntity(id: string) { return this.req<EntityDetail>("GET", `/v1/entities/${id}`); }
  mergeEntities(source: string, target: string) {
    return this.req<{ source: string; target: string; status: string }>("POST", "/v1/entities/merge", { source, target });
  }
  entityGraph(id: string) { return this.req<Graph>("GET", `/v1/entities/${id}/graph`); }
  graph() { return this.req<Graph>("GET", "/v1/graph"); }
  async graphPpr(query: string): Promise<Record<string, number>> {
    return (await this.req<{ scores: Record<string, number> }>("POST", "/v1/graph/ppr", { query })).scores;
  }
  communities() { return this.req<{ communities: { community_id: number; members: Entity[] }[]; summaries: Memory[] }>("GET", "/v1/communities"); }
  async auditAll(limit = 200): Promise<AuditEntry[]> {
    return (await this.req<{ entries: AuditEntry[] }>("GET", `/v1/audit?limit=${limit}`)).entries;
  }
  async working(): Promise<Memory[]> {
    return (await this.req<{ memories: Memory[] }>("GET", "/v1/working")).memories;
  }
  getConfig() { return this.req<Record<string, unknown>>("GET", "/v1/config"); }
  putConfig(patch: Record<string, unknown>) {
    return this.req<{ saved: boolean; path: string; restart_required_for: string[] }>(
      "PUT", "/v1/config", patch,
    );
  }
  getSyncStatus() {
    return this.req<{
      backend: string;
      ready: boolean;
      detail: string;
      last_pushed_at: string | null;
      last_pulled_at: string | null;
      last_error: string | null;
    }>("GET", "/v1/sync/status");
  }
  runSyncPull() {
    return this.req<{ files_changed: number; message: string; conflicts: string[] }>(
      "POST", "/v1/sync/pull",
    );
  }
}

export const client = new MnemosClient(import.meta.env.VITE_MNEMOS_URL ?? "http://localhost:7423", getToken);
