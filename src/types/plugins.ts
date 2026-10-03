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
