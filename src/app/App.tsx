import { useEffect, useState } from "react";
import { Activity, HardDrive, ShieldCheck } from "lucide-react";
import { Sidebar } from "../components/Sidebar";
import { ChatWorkspace } from "../features/chat/ChatWorkspace";
import { HardwareView } from "../features/hardware/HardwareView";
import { SettingsView } from "../features/settings/SettingsView";
import { WorkspaceView } from "../features/workspaces/WorkspaceView";
import { bridge } from "../services/bridge";
import { useAppStore } from "../stores/app-store";
import type { AppSnapshot } from "../types/domain";

const viewTitles = {
  chat: "Chat",
  images: "Image Studio",
  speech: "Speech to Text",
  tts: "Text to Speech",
  models: "Model Manager",
  prompts: "Prompt Library",
  gallery: "Gallery",
  hardware: "Hardware",
  downloads: "Downloads",
  settings: "Settings",
  logs: "Logs",
} as const;

export function App() {
  const activeView = useAppStore((state) => state.activeView);
  const theme = useAppStore((state) => state.theme);
  const setTheme = useAppStore((state) => state.setTheme);
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);

  useEffect(() => {
    void Promise.all([bridge.getAppSnapshot(), bridge.getSettings()]).then(([app, settings]) => {
      setSnapshot(app);
      setTheme(settings.theme);
    });
  }, [setTheme]);

  useEffect(() => {
    let disposed = false;
    let unlisten: () => void = () => {};
    void bridge.onChatStateChanged((event) => {
      setSnapshot((current) => current ? {
        ...current,
        activeJobs: event.state === "started" ? 1 : 0,
      } : current);
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });
    return () => {
      disposed = true;
      unlisten();
    };
  }, []);

  useEffect(() => {
    const resolved = theme === "system"
      ? (matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark")
      : theme;
    document.documentElement.dataset.theme = resolved;
  }, [theme]);

  return (
    <div className="app-shell">
      <Sidebar />
      <main className="main-shell">
        <header className="topbar">
          <div>
            <p className="eyebrow">Local workspace</p>
            <h1>{viewTitles[activeView]}</h1>
          </div>
          <div className="topbar-status">
            <div title="Active inference jobs"><Activity size={15} /><span>{snapshot?.activeJobs ?? 0} jobs</span></div>
            <div title="Metadata database"><HardDrive size={15} /><span>{snapshot?.databaseReady ? "Database ready" : "Starting"}</span></div>
            <div className="private" title="Normal desktop IPC does not expose a network port"><ShieldCheck size={15} /><span>Private</span></div>
          </div>
        </header>
        <section className={`view-container ${activeView === "chat" ? "chat-view" : ""}`}>
          <div className="chat-view-host" hidden={activeView !== "chat"}><ChatWorkspace /></div>
          {activeView !== "chat" && (activeView === "hardware" ? <HardwareView /> : activeView === "settings" ? <SettingsView /> : <WorkspaceView view={activeView} />)}
        </section>
      </main>
    </div>
  );
}
