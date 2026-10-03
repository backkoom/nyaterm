import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { usePlugins } from "@/context/PluginContext";
import { getErrorMessage } from "@/lib/errors";
import { createPluginBridge } from "@/lib/pluginBridge";
import { activeManifest, parsePluginPanelId, pluginApi } from "@/lib/plugins";
import type { PluginScope } from "@/types/plugins";

const THEME_VARIABLES = [
  "--background",
  "--foreground",
  "--primary",
  "--primary-foreground",
  "--muted",
  "--muted-foreground",
  "--border",
  "--destructive",
  "--df-bg",
  "--df-fg",
];

function theme() {
  const style = getComputedStyle(document.documentElement);
  return Object.fromEntries(
    THEME_VARIABLES.map((key) => [key, style.getPropertyValue(key).trim()]),
  );
}

export function PluginPanel({
  activityId,
  sessionId,
}: {
  activityId: string;
  sessionId: string | null;
}) {
  const { t } = useTranslation();
  const { plugins, generation, locked, loaded, openIntent } = usePlugins();
  const panelId = parsePluginPanelId(activityId);
  const plugin = plugins.find((plugin) => plugin.id === panelId?.pluginId);
  const manifest = plugin && activeManifest(plugin);
  const panel = manifest?.contributions.panels.find(
    (panel) => panel.id === panelId?.panelId,
  );
  const intentPanelId =
    openIntent?.panelId ??
    manifest?.contributions.commands.find(
      (command) => command.id === openIntent?.commandId,
    )?.panel;
  const scopedSessionId =
    openIntent?.pluginId === plugin?.id &&
    intentPanelId === panel?.id &&
    openIntent?.sessionId !== undefined
      ? openIntent.sessionId
      : sessionId;
  const [scope, setScope] = useState<PluginScope | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [retry, setRetry] = useState(0);
  const frame = useRef<HTMLIFrameElement>(null);
  const enabled = Boolean(plugin?.enabled && panel && !locked);
  const pluginId = plugin?.id;
  const version = plugin?.activeVersion;

  // Permission changes and renewal must replace the backend-issued scope.
  // biome-ignore lint/correctness/useExhaustiveDependencies: version, generation and retry invalidate scopes even if the plugin remains enabled.
  useEffect(() => {
    setScope(null);
    setError(null);
    if (!enabled || !pluginId) return;
    let disposed = false;
    let token: string | undefined;
    void pluginApi
      .createScope(pluginId, scopedSessionId)
      .then((scope) => {
        token = scope.token;
        if (disposed) void pluginApi.closeScope(scope.token).catch(() => {});
        else setScope(scope);
      })
      .catch((error) => {
        if (!disposed) setError(getErrorMessage(error));
      });
    const refreshTimer = window.setTimeout(
      () => setRetry((value) => value + 1),
      25 * 60 * 1000,
    );
    return () => {
      disposed = true;
      window.clearTimeout(refreshTimer);
      if (token) void pluginApi.closeScope(token).catch(() => {});
    };
  }, [enabled, pluginId, version, generation, scopedSessionId, retry]);

  const entry = panel?.entry;
  useEffect(() => {
    const iframe = frame.current;
    if (!scope || !iframe || !entry) return;
    const bridge = createPluginBridge(iframe, scope.token, () => ({
      pluginId: scope.pluginId,
      version: scope.version,
      theme: theme(),
    }));
    const observer = new MutationObserver(() => bridge.updateContext());
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class", "style", "data-theme"],
    });
    // Register the bridge before the document starts loading and sends its ready message.
    iframe.src = `${convertFileSrc("", "nyaterm-plugin")}${scope.token}/${entry.split("/").map(encodeURIComponent).join("/")}`;
    return () => {
      bridge.dispose();
      observer.disconnect();
    };
  }, [scope, entry]);

  if (!enabled)
    return (
      <output className="block p-4 text-sm text-muted-foreground">
        {!loaded
          ? t("plugins.loading")
          : locked
            ? t("plugins.locked")
            : t("plugins.unavailable")}
      </output>
    );
  if (error)
    return (
      <div className="space-y-3 p-4" role="alert">
        <p className="break-words text-sm text-destructive">{error}</p>
        <Button
          variant="outline"
          onClick={() => setRetry((value) => value + 1)}
        >
          {t("plugins.retry")}
        </Button>
      </div>
    );
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="border-b px-3 py-2 text-sm font-medium">
        {panel?.title}
      </div>
      {scope ? (
        <iframe
          ref={frame}
          title={panel?.title}
          sandbox="allow-scripts"
          referrerPolicy="no-referrer"
          className="min-h-0 flex-1 w-full border-0"
        />
      ) : (
        <div className="p-4 text-sm text-muted-foreground">
          {t("plugins.loading")}
        </div>
      )}
    </div>
  );
}
