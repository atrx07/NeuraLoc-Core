import { useEffect, useState } from "react";
import { Check, ShieldAlert } from "lucide-react";
import { bridge } from "../../services/bridge";
import { useAppStore } from "../../stores/app-store";
import type { AppSettings, PerformanceProfile, Theme } from "../../types/domain";

export function SettingsView() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [saved, setSaved] = useState(false);
  const setTheme = useAppStore((state) => state.setTheme);

  useEffect(() => { void bridge.getSettings().then(setSettings); }, []);

  async function update(patch: Partial<AppSettings>) {
    const next = await bridge.updateSettings(patch);
    setSettings(next);
    if (patch.theme) setTheme(patch.theme);
    setSaved(true);
    window.setTimeout(() => setSaved(false), 1400);
  }

  if (!settings) return <div className="loading-state">Loading settings...</div>;

  return (
    <div className="settings-layout">
      <div className="settings-intro"><h2>Application settings</h2><span className={saved ? "save-state visible" : "save-state"}><Check size={15} /> Saved</span></div>
      <section className="settings-section">
        <div><h3>Appearance</h3><p>Choose how NeuraLoc-Core appears on this device.</p></div>
        <div className="setting-control">
          <label htmlFor="theme">Theme</label>
          <div className="segmented-control" id="theme">
            {(["dark", "light", "system"] as Theme[]).map((theme) => <button className={settings.theme === theme ? "active" : ""} key={theme} onClick={() => void update({ theme })} type="button">{theme}</button>)}
          </div>
        </div>
      </section>
      <section className="settings-section">
        <div><h3>Performance</h3><p>Controls device preference, power use, and model retention.</p></div>
        <div className="setting-stack">
          <label>Performance profile
            <select value={settings.performanceProfile} onChange={(event) => void update({ performanceProfile: event.target.value as PerformanceProfile })}>
              <option value="maximum">Maximum performance</option><option value="balanced">Balanced</option><option value="low_power">Low power</option><option value="quiet">Quiet</option><option value="manual">Manual</option>
            </select>
          </label>
          <label className="toggle-row"><span><strong>Keep models loaded</strong><small>Retain compatible models between workspace changes.</small></span><input type="checkbox" checked={settings.keepModelsLoaded} onChange={(event) => void update({ keepModelsLoaded: event.target.checked })} /></label>
          <label>Idle unload timeout <input type="number" min={1} max={240} value={settings.idleUnloadMinutes} onChange={(event) => void update({ idleUnloadMinutes: Number(event.target.value) })} /><span className="input-suffix">minutes</span></label>
        </div>
      </section>
      <section className="settings-section">
        <div><h3>Privacy and network</h3><p>Internet features remain off until explicitly enabled.</p></div>
        <div className="setting-stack">
          <label className="toggle-row"><span><strong>Internet access</strong><small>Allow catalog refreshes and approved downloads.</small></span><input type="checkbox" checked={settings.internetAccess} onChange={(event) => void update({ internetAccess: event.target.checked, webSearch: event.target.checked ? settings.webSearch : false })} /></label>
          <label className="toggle-row"><span><strong>Web search</strong><small>Send search queries only when enabled in Chat.</small></span><input type="checkbox" disabled={!settings.internetAccess} checked={settings.webSearch} onChange={(event) => void update({ webSearch: event.target.checked })} /></label>
          <label className="toggle-row"><span><strong>Local API</strong><small>Bind an optional OpenAI-compatible API to loopback.</small></span><input type="checkbox" checked={settings.apiEnabled} onChange={(event) => void update({ apiEnabled: event.target.checked })} /></label>
          {settings.apiEnabled && <div className="security-note"><ShieldAlert size={17} /><span>The API will bind to <code>127.0.0.1:{settings.apiPort}</code>. LAN access remains disabled.</span></div>}
        </div>
      </section>
    </div>
  );
}
