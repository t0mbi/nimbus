import {
  ButtonItem,
  PanelSection,
  PanelSectionRow,
  TextField,
  ToggleField,
  SliderField,
  staticClasses,
} from "@decky/ui";
import { addEventListener, removeEventListener, callable, definePlugin, toaster } from "@decky/api";
import { useEffect, useState } from "react";
import { FaCloud } from "react-icons/fa";

interface NimbusSettings {
  sync_path: string | null;
  format: string;
  full_limit: number;
  paused: boolean;
}

interface SyncResult {
  ok: boolean;
  message?: string;
  pushed?: string[];
  pulled?: string[];
  failed?: string[];
  checked?: number;
}

interface LudusaviStatus {
  found: boolean;
  version: string | null;
}

const getSettings = callable<[], NimbusSettings>("get_settings");
const setSyncPath = callable<[path: string], void>("set_sync_path");
const setFormat = callable<[fmt: string], void>("set_format");
const setFullLimit = callable<[n: number], void>("set_full_limit");
const togglePause = callable<[], boolean>("toggle_pause");
const ludusaviStatus = callable<[], LudusaviStatus>("ludusavi_status");
const syncNow = callable<[], SyncResult>("sync_now");

function summarize(result: SyncResult | null): string {
  if (!result) return "";
  if (!result.ok) return result.message ?? "Couldn't sync.";
  const pushed = result.pushed?.length ?? 0;
  const pulled = result.pulled?.length ?? 0;
  const failed = result.failed?.length ?? 0;
  if (pushed === 0 && pulled === 0 && failed === 0) {
    return `Checked ${result.checked ?? 0} game(s) - already up to date.`;
  }
  const parts: string[] = [];
  if (pushed) parts.push(`pushed ${pushed}`);
  if (pulled) parts.push(`pulled ${pulled}`);
  if (failed) parts.push(`${failed} failed`);
  return parts.join(", ");
}

function Content() {
  const [settings, setSettings] = useState<NimbusSettings | null>(null);
  const [pathInput, setPathInput] = useState("");
  const [status, setStatus] = useState<LudusaviStatus | null>(null);
  const [lastResult, setLastResult] = useState<SyncResult | null>(null);
  const [syncing, setSyncing] = useState(false);

  useEffect(() => {
    (async () => {
      const s = await getSettings();
      setSettings(s);
      setPathInput(s.sync_path ?? "");
      setStatus(await ludusaviStatus());
    })();

    // One event per sync pass, not one per game - see main.py's
    // _emit_summary for why (a first run against a share with a lot of
    // pre-existing history used to fire a toast per game, all at once).
    const listener = addEventListener<[pushed: string[], pulled: string[], failed: string[]]>(
      "nimbus_sync_summary",
      (pushed, pulled, failed) => {
        if (pushed.length === 0 && pulled.length === 0 && failed.length === 0) {
          toaster.toast({ title: "Nimbus", body: "Already up to date." });
          return;
        }
        const parts: string[] = [];
        if (pushed.length) parts.push(`pushed ${pushed.join(", ")}`);
        if (pulled.length) parts.push(`pulled ${pulled.join(", ")}`);
        if (failed.length) parts.push(`failed: ${failed.join(", ")}`);
        toaster.toast({ title: "Nimbus", body: parts.join(" · ") });
      },
    );
    return () => removeEventListener("nimbus_sync_summary", listener);
  }, []);

  const savePath = async () => {
    await setSyncPath(pathInput);
    setSettings((s) => (s ? { ...s, sync_path: pathInput } : s));
  };

  const onTogglePause = async () => {
    const paused = await togglePause();
    setSettings((s) => (s ? { ...s, paused } : s));
  };

  const onSyncNow = async () => {
    setSyncing(true);
    try {
      setLastResult(await syncNow());
    } finally {
      setSyncing(false);
    }
  };

  return (
    <PanelSection title="Nimbus">
      <PanelSectionRow>
        {status === null
          ? "Checking for Ludusavi…"
          : status.found
            ? `Ludusavi ${status.version} found`
            : "Ludusavi not found - install it and make sure it's on PATH."}
      </PanelSectionRow>

      <PanelSectionRow>
        <TextField
          label="Sync folder"
          description="A mounted SMB share or other local path."
          value={pathInput}
          onChange={(e) => setPathInput(e.target.value)}
        />
      </PanelSectionRow>
      <PanelSectionRow>
        <ButtonItem layout="below" onClick={savePath}>
          Save sync folder
        </ButtonItem>
      </PanelSectionRow>

      {settings && (
        <>
          <PanelSectionRow>
            <ToggleField
              label="Background sync"
              description="Watches your games and syncs automatically."
              checked={!settings.paused}
              onChange={onTogglePause}
            />
          </PanelSectionRow>

          <PanelSectionRow>
            <ToggleField
              label="Zip format"
              description="Keeps version history. Off = overwrite in place, no history."
              checked={settings.format === "zip"}
              onChange={(checked) => {
                const fmt = checked ? "zip" : "simple";
                setFormat(fmt);
                setSettings((s) => (s ? { ...s, format: fmt } : s));
              }}
            />
          </PanelSectionRow>

          <PanelSectionRow>
            <SliderField
              label="Versions to keep"
              value={settings.full_limit}
              min={1}
              max={20}
              step={1}
              showValue
              onChange={(n) => {
                setFullLimit(n);
                setSettings((s) => (s ? { ...s, full_limit: n } : s));
              }}
            />
          </PanelSectionRow>
        </>
      )}

      <PanelSectionRow>
        <ButtonItem layout="below" onClick={onSyncNow} disabled={syncing}>
          {syncing ? "Syncing…" : "Sync now"}
        </ButtonItem>
      </PanelSectionRow>
      {lastResult && <PanelSectionRow>{summarize(lastResult)}</PanelSectionRow>}
    </PanelSection>
  );
}

export default definePlugin(() => {
  return {
    name: "Nimbus",
    titleView: <div className={staticClasses.Title}>Nimbus</div>,
    content: <Content />,
    icon: <FaCloud />,
    onDismount() {},
  };
});
