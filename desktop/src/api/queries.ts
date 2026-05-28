import { useQuery } from "@tanstack/react-query";
import { client } from "./client";
import type { SearchReq, Tier } from "./types";

export const useMemories = (tier?: Tier[]) =>
  useQuery({ queryKey: ["memories", tier], queryFn: () => client.listMemories({ tier, limit: 100 }) });
export const useMemory = (id: string | null) =>
  useQuery({ queryKey: ["memory", id], queryFn: () => client.getMemory(id!), enabled: !!id });
export const useSearch = (req: SearchReq | null) =>
  useQuery({ queryKey: ["search", req], queryFn: () => client.search(req!), enabled: !!req && !!req.query });
export const useGraph = () => useQuery({ queryKey: ["graph"], queryFn: () => client.graph() });
export const useCommunities = () => useQuery({ queryKey: ["communities"], queryFn: () => client.communities() });
export const usePipelines = () => useQuery({ queryKey: ["pipelines"], queryFn: () => client.pipelines(), refetchInterval: 5000 });
export const useReflections = () => useQuery({ queryKey: ["reflections"], queryFn: () => client.listReflections() });
export const useEntity = (id: string | null) =>
  useQuery({ queryKey: ["entity", id], queryFn: () => client.getEntity(id!), enabled: !!id });
export const useAudit = (id: string | null) =>
  useQuery({ queryKey: ["audit", id], queryFn: () => client.audit(id!), enabled: !!id });
export const useAuditAll = () =>
  useQuery({ queryKey: ["audit-all"], queryFn: () => client.auditAll() });
