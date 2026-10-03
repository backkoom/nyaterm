import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
const calls = vi.hoisted(() => ({ host: vi.fn(), backend: vi.fn() }));
vi.mock("@/lib/plugins", () => ({
  pluginApi: { hostCall: calls.host, backendCall: calls.backend },
}));
import { createPluginBridge } from "./pluginBridge";

describe("plugin bridge isolation", () => {
  let frame: HTMLIFrameElement;
  let bridge: ReturnType<typeof createPluginBridge>;
  let post: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    vi.clearAllMocks();
    calls.host.mockResolvedValue({ output: "hello" });
    calls.backend.mockResolvedValue("done");
    frame = document.createElement("iframe");
    document.body.append(frame);
    post = vi.spyOn(frame.contentWindow as Window, "postMessage");
    bridge = createPluginBridge(frame, "fixed-token", () => ({
      pluginId: "example.tools",
      version: "1.0.0",
      theme: {},
    }));
  });
  afterEach(() => {
    bridge.dispose();
    frame.remove();
  });
  const message = (
    frame: HTMLIFrameElement,
    method: string,
    origin = "null",
    source: Window | null = frame.contentWindow,
  ) => {
    window.dispatchEvent(
      new MessageEvent("message", {
        source,
        origin,
        data: {
          type: "nyaterm-plugin-request",
          id: "1",
          method,
          params: { lines: 20, token: "forged-token" },
        },
      }),
    );
  };

  it("rejects other frames and nonopaque origins", () => {
    message(frame, "host/terminal/read", "null", window);
    message(frame, "host/terminal/read", "https://example.com");
    expect(calls.host).not.toHaveBeenCalled();
  });

  it("uses the host-bound token instead of plugin-supplied identity", async () => {
    message(frame, "host/terminal/read");
    await vi.waitFor(() => expect(post).toHaveBeenCalled());
    expect(calls.host).toHaveBeenCalledWith(
      "fixed-token",
      "host/terminal/read",
      { lines: 20, token: "forged-token" },
    );
    expect(post).toHaveBeenCalledWith(
      { type: "nyaterm-plugin-response", id: "1", result: { output: "hello" } },
      "*",
    );
  });

  it("does not expose arbitrary Tauri commands", () => {
    message(frame, "get_otp_secret_value");
    expect(calls.host).not.toHaveBeenCalled();
    expect(calls.backend).not.toHaveBeenCalled();
    expect(post).toHaveBeenCalledWith(
      expect.objectContaining({ error: "Unknown plugin capability" }),
      "*",
    );
  });

  it("rejects duplicate in-flight IDs and drops late responses after disposal", async () => {
    let resolve!: (value: unknown) => void;
    calls.host.mockImplementation(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    );
    message(frame, "host/terminal/read");
    message(frame, "host/terminal/read");
    expect(calls.host).toHaveBeenCalledTimes(1);
    bridge.dispose();
    resolve("late");
    await Promise.resolve();
    await Promise.resolve();
    expect(post).not.toHaveBeenCalled();
  });

  it("rejects oversized payloads before invoking the backend", () => {
    window.dispatchEvent(
      new MessageEvent("message", {
        source: frame.contentWindow,
        origin: "null",
        data: {
          type: "nyaterm-plugin-request",
          id: "large",
          method: "host/storage/set",
          params: "a".repeat(1024 * 1024 + 1),
        },
      }),
    );
    expect(calls.host).not.toHaveBeenCalled();
    expect(post).toHaveBeenCalledWith(
      expect.objectContaining({ error: "Plugin request is too large" }),
      "*",
    );
  });
});
