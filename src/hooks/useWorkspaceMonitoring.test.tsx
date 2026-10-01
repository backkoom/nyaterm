import { renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createSessionPane } from "@/lib/workspaceTabs";
import type { RemoteStats, SavedConnection, UiConfig } from "@/types/global";
import { useWorkspaceMonitoring } from "./useWorkspaceMonitoring";

const mocks = vi.hoisted(() => ({
  stats: vi.fn(),
  gpu: vi.fn(),
  npu: vi.fn(),
  network: vi.fn(),
}));
vi.mock("./useRemoteStats", () => ({ useRemoteStats: mocks.stats }));
vi.mock("./useRemoteGpuOverview", () => ({ useRemoteGpuOverview: mocks.gpu }));
vi.mock("./useRemoteNpuOverview", () => ({ useRemoteNpuOverview: mocks.npu }));
vi.mock("./useNetworkHistory", () => ({ useNetworkHistory: mocks.network }));

function monitoringOptions(): Parameters<typeof useWorkspaceMonitoring>[0] {
  return {
    activePane: createSessionPane("Host", "SSH", "conn-1", { sessionId: "ssh-1" }),
    activeConnection: undefined,
    liveSessionIds: new Set(["ssh-1", "ssh-2"]),
    liveSessionsById: null,
    uiConfig: {
      show_remote_stats: true,
      show_gpu_monitor: false,
      show_ascend_npu_monitor: false,
      header_status_visible: true,
      header_status_mode: "gpu",
    } as UiConfig,
    handleAssetMonitoringPatch: vi.fn(),
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.stats.mockReturnValue({ sessionId: null, stats: null });
  mocks.gpu.mockReturnValue({ sessionId: null, overview: null });
  mocks.npu.mockReturnValue({ sessionId: null, overview: null });
});

describe("useWorkspaceMonitoring", () => {
  it("enables the header's accelerator monitor and stops monitoring closed or unsupported sessions", () => {
    const options = monitoringOptions();
    const { result, rerender } = renderHook(useWorkspaceMonitoring, { initialProps: options });
    expect(result.current.activeRemoteStatsEnabled).toBe(true);
    expect(mocks.gpu).toHaveBeenLastCalledWith("ssh-1", true, 3);
    expect(mocks.npu).toHaveBeenLastCalledWith("ssh-1", false, 3);

    rerender({ ...options, liveSessionIds: new Set() });
    expect(result.current.activeStatsSessionId).toBeNull();
    expect(mocks.stats).toHaveBeenLastCalledWith(null, false, 3, new Set());
    expect(mocks.gpu).toHaveBeenLastCalledWith(null, false, 3);

    rerender({
      ...options,
      activeConnection: { ssh_profile: "network_device" } as SavedConnection,
    });
    expect(result.current.activeStatsSessionId).toBeNull();
    expect(result.current.activeRemoteStatsEnabled).toBe(false);
  });

  it("never applies the previous session's snapshot after switching the active pane", () => {
    const stats: RemoteStats = {
      system: { hostname: "host-1", os: "Linux", arch: "x86_64", uptime_sec: 60 },
      load: { load1: 0, load5: 0, load15: 0 },
      cpu: {
        model: "CPU",
        cores: 4,
        usage: 0,
        per_core: [],
        sample_window_ms: 1000,
        usage_source: "aggregate",
      },
      memory: { used: 100, available: 100, cached: 0 },
      networks: [],
      network_summary: { rx_bytes_per_sec: 0, tx_bytes_per_sec: 0 },
      disks: [],
    };
    mocks.stats.mockReturnValue({ sessionId: "ssh-1", stats });
    const options = monitoringOptions();
    const { rerender } = renderHook(useWorkspaceMonitoring, { initialProps: options });
    expect(options.handleAssetMonitoringPatch).toHaveBeenCalledWith(
      "ssh-1",
      "ssh-1",
      expect.objectContaining({ hostname: "host-1" }),
    );
    vi.mocked(options.handleAssetMonitoringPatch).mockClear();

    const next = {
      ...options,
      activePane: createSessionPane("Host 2", "SSH", "conn-2", { sessionId: "ssh-2" }),
    };
    rerender(next);
    expect(options.handleAssetMonitoringPatch).not.toHaveBeenCalled();
    mocks.stats.mockReturnValue({
      sessionId: "ssh-2",
      stats: { ...stats, system: { ...stats.system, hostname: "host-2" } },
    });
    rerender({ ...next });
    expect(options.handleAssetMonitoringPatch).toHaveBeenCalledExactlyOnceWith(
      "ssh-2",
      "ssh-2",
      expect.objectContaining({ hostname: "host-2" }),
    );
  });
});
