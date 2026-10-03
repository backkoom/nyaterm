/* NyaTerm Plugin UI SDK v1. Include this as a classic script before plugin code. */
(() => {
  "use strict";
  const pending = new Map();
  const contextListeners = new Set();
  let context = null;
  let nextId = 0;
  let resolveReady;
  const ready = new Promise((resolve) => {
    resolveReady = resolve;
  });
  function call(method, params = null) {
    if (pending.size >= 32)
      return Promise.reject(new Error("Too many pending requests"));
    const id = String(++nextId);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error("Plugin host request timed out"));
      }, 185000);
      pending.set(id, { resolve, reject, timer });
      window.parent.postMessage(
        { type: "nyaterm-plugin-request", id, method, params },
        "*",
      );
    });
  }
  window.addEventListener("message", (event) => {
    if (event.source !== window.parent) return;
    const message = event.data;
    if (!message || typeof message !== "object") return;
    if (message.type === "nyaterm-plugin-context") {
      context = message.context;
      for (const [key, value] of Object.entries(context.theme || {})) {
        document.documentElement.style.setProperty(key, value);
      }
      resolveReady(context);
      for (const listener of contextListeners) listener(context);
    } else if (message.type === "nyaterm-plugin-response") {
      const request = pending.get(message.id);
      if (!request) return;
      pending.delete(message.id);
      clearTimeout(request.timer);
      if (message.error) request.reject(new Error(message.error));
      else request.resolve(message.result);
    }
  });
  window.addEventListener("pagehide", () => {
    for (const request of pending.values()) {
      clearTimeout(request.timer);
      request.reject(new Error("Plugin view closed"));
    }
    pending.clear();
  });
  window.NyaTerm = Object.freeze({
    ready,
    call,
    get context() {
      return context;
    },
    onContextChange(listener) {
      contextListeners.add(listener);
      return () => contextListeners.delete(listener);
    },
    session: () => call("host/session"),
    terminal: Object.freeze({
      read: (lines = 100) => call("host/terminal/read", { lines }),
      execute: (command, timeoutMs = 30000) =>
        call("host/terminal/execute", { command, timeoutMs }),
    }),
    filesystem: Object.freeze({
      read: (path) => call("host/filesystem/read", { path }),
    }),
    storage: Object.freeze({
      get: (key) => call("host/storage/get", { key }),
      set: (key, value) => call("host/storage/set", { key, value }),
    }),
    network: Object.freeze({
      request: (url, options = {}) =>
        call("host/network/request", { ...options, url }),
    }),
    backend: (method, input = null) => call(method, input),
  });
  window.parent.postMessage({ type: "nyaterm-plugin-ready" }, "*");
})();
