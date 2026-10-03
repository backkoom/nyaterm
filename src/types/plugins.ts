export type PluginPermission = string;

export interface PluginDiagnosticsSnapshot {
  status: "idle" | "starting" | "running" | "stopped" | "error";
  version: string;
  lastError: string | null;
  logs: {
    timestamp: number;
    version: string;
    level: string;
    source: string;
    message: string;
  }[];
}

export interface PluginPanelContribution {
  id: string;
  title: string;
  entry: string;
}

export interface PluginCommandContribution {
  id: string;
  title: string;
  panel?: string | null;
  method?: string | null;
  menus: ("terminal" | "connection")[];
}

export interface PluginManifest {
  manifestVersion: number;
  id: string;
  name: string;
  version: string;
  description: string;
  publisher: string;
  engine: string;
  permissions: PluginPermission[];
  contributions: {
    panels: PluginPanelContribution[];
    commands: PluginCommandContribution[];
    probes?: { id: string; title: string; entry: string; timeoutMs: number }[];
    monitors?: { id: string; title: string; schema: "gpu.v1"; method: string; panel: string }[];
  };
  backend?: {
    transport: "stdio-jsonl" | "stdio-framed";
    executables: Record<string, string>;
  } | null;
}

export interface InstalledPlugin {
  id: string;
  activeVersion: string;
  enabled: boolean;
  grantedPermissions: PluginPermission[];
  versions: Record<string, { manifest: PluginManifest; digest: string }>;
}

export interface PluginPackagePreview {
  manifest: PluginManifest;
  digest: string;
  expandedBytes: number;
  probeScripts?: Record<string, string>;
}

export interface PluginMonitorSnapshot {
  revision: number;
  sessionId: string;
  overview: import("@/types/global").RemoteGpuOverview | null;
  error: boolean;
  refreshing: boolean;
  paused: boolean;
}

export interface PluginMonitorSubscription {
  subscriptionId: string;
  snapshot: PluginMonitorSnapshot;
}

export interface PluginScope {
  token: string;
  pluginId: string;
  version: string;
}

export interface PluginApprovalRequest {
  requestId: string;
  pluginId: string;
  pluginName: string;
  capability: string;
  sessionName: string;
  summary: string;
  risk: string;
}
