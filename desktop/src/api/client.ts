import type {
  AuditEntry, Entity, EntityDetail, Graph, Memory, PipelineStatus, RecallHit, SearchReq, Tier,
} from "./types";
import { getToken } from "./token";

export interface ConnectorEdit { path: string; present: boolean }
export interface Connector {
  id: string;
  display_name: string;
  kind: "detectable" | "manual";
  deprecated: string | null;
  installed: boolean;
  connected: "full" | "partial" | "none";
  manual_snippet: { target: string; snippet: string } | null;
  edits: ConnectorEdit[];
}
export interface ConnectorPreview {
  id: string;
  edits: { path: string; before: string; after: string; already_present: boolean }[];
}

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
  async getDoctor() {
    return this.req<{
      checks: { name: string; status: "ok" | "warn" | "fail"; detail: string }[];
      report: { files_scanned: number; db_rows: number; issues: unknown[] };
      migration_hint:
        | { from_kind: string; from_model: string; from_dim: number; to_kind: string }
        | null;
    }>("GET", "/v1/doctor");
  }

  async getEmbedRebuildStatus() {
    return this.req<
      | { status: "idle" }
      | { status: "running"; processed: number; total: number }
      | {
          status: "completed";
          processed: number;
          skipped: number;
          total: number;
          swapped: boolean;
        }
      | { status: "failed"; error: string; processed: number }
    >("GET", "/v1/embed-rebuild/status");
  }

  async startEmbedRebuild(target_kind: string, target_model: string, target_dim: number) {
    return this.req<{ started: boolean }>("POST", "/v1/embed-rebuild/start", {
      target_kind,
      target_model,
      target_dim,
    });
  }

  async abortEmbedRebuild() {
    return this.req<{ aborted: boolean }>("POST", "/v1/embed-rebuild/abort");
  }

  async getFirstRun() {
    return this.req<{ completed_at: string | null }>("GET", "/v1/first-run");
  }
  async completeFirstRun() {
    return this.req<{ completed: true }>("POST", "/v1/first-run/complete");
  }

  async listConnectors(): Promise<Connector[]> {
    return (await this.req<{ connectors: Connector[] }>("GET", "/v1/connectors")).connectors;
  }
  previewConnector(id: string): Promise<ConnectorPreview> {
    return this.req<ConnectorPreview>("POST", `/v1/connectors/${id}/preview`);
  }
  connectConnector(id: string): Promise<{ id: string; connected: string }> {
    return this.req<{ id: string; connected: string }>("POST", `/v1/connectors/${id}/connect`);
  }
  disconnectConnector(id: string): Promise<{ id: string; connected: string }> {
    return this.req<{ id: string; connected: string }>("POST", `/v1/connectors/${id}/disconnect`);
  }

  async vaultExport(): Promise<Blob> {
    const token = await this.tokenFn();
    const res = await fetch(`${this.baseUrl}/v1/vault/export`, {
      method: "POST",
      headers: { authorization: `Bearer ${token}` },
    });
    if (!res.ok) throw new ApiError(res.status, res.statusText);
    return res.blob();
  }

  async vaultImport(zip: Blob): Promise<{ files_imported: number }> {
    const token = await this.tokenFn();
    const res = await fetch(`${this.baseUrl}/v1/vault/import`, {
      method: "POST",
      headers: { authorization: `Bearer ${token}`, "content-type": "application/zip" },
      body: zip,
    });
    if (!res.ok) throw new ApiError(res.status, res.statusText);
    return (await res.json()) as { files_imported: number };
  }
}

export const client = new MnemosClient(import.meta.env.VITE_MNEMOS_URL ?? "http://localhost:7423", getToken);
