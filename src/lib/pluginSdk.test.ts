import sdk from "../../plugins/sdk/nyaterm.js?raw";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createPluginBridge } from "./pluginBridge";
import { pluginApi } from "./plugins";

vi.mock("./plugins", () => ({
  pluginApi: { hostCall: vi.fn(), backendCall: vi.fn() },
}));
afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = "";
});

describe("plugin SDK round trip", () => {
  it("handshakes, applies theme and routes a scoped request through the host bridge", async () => {
    const frame = document.createElement("iframe");
    document.body.append(frame);
    const listeners = new Map<string, (event?: unknown) => void>();
    const parent = {
      postMessage: (data: unknown) =>
        window.dispatchEvent(
          new MessageEvent("message", {
            data,
            source: frame.contentWindow,
            origin: "null",
          }),
        ),
    };
    const pluginWindow = {
      parent,
      addEventListener: (name: string, listener: (event?: unknown) => void) =>
        listeners.set(name, listener),
      NyaTerm: undefined as unknown,
    };
    vi.spyOn(frame.contentWindow!, "postMessage").mockImplementation((data) =>
      listeners.get("message")?.({ source: parent, data }),
    );
    const bridge = createPluginBridge(frame, "fixed-scope", () => ({
      pluginId: "example.tools",
      version: "1.0.0",
      theme: { "--background": "black" },
    }));
    new Function("window", "document", sdk)(pluginWindow, document);
    const api = pluginWindow.NyaTerm as {
      ready: Promise<unknown>;
      session: () => Promise<unknown>;
      storage: { get: (key: string) => Promise<unknown> };
    };
    await expect(api.ready).resolves.toMatchObject({
      pluginId: "example.tools",
    });
    expect(
      document.documentElement.style.getPropertyValue("--background"),
    ).toBe("black");
    vi.mocked(pluginApi.hostCall).mockResolvedValueOnce({ name: "Local" });
    await expect(api.session()).resolves.toEqual({ name: "Local" });
    expect(pluginApi.hostCall).toHaveBeenCalledWith(
      "fixed-scope",
      "host/session",
      null,
    );
    vi.mocked(pluginApi.hostCall).mockRejectedValueOnce(
      new Error("Permission denied"),
    );
    await expect(api.storage.get("setting")).rejects.toThrow(
      "Permission denied",
    );
    bridge.dispose();
    document.documentElement.style.removeProperty("--background");
  });
});
