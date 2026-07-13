import { useEffect, useState } from "react";
import { AlertTriangle, Cpu, Gauge, MemoryStick, RefreshCw, Thermometer } from "lucide-react";
import { bridge } from "../../services/bridge";
import type { CapabilityStatus, HardwareSnapshot } from "../../types/domain";
import { formatBytes } from "../../utils/format";

const statusLabels: Record<CapabilityStatus, string> = {
  available: "Ready",
  unavailable: "Unavailable",
  unknown: "Needs probe",
  experimental: "Experimental",
};

export function HardwareView() {
  const [snapshot, setSnapshot] = useState<HardwareSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  async function load(refresh = false) {
    setLoading(true);
    setError(null);
    try {
      setSnapshot(await bridge.getHardwareSnapshot(refresh));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Hardware discovery failed.");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void load(); }, []);

  return (
    <div className="hardware-view">
      <div className="section-toolbar">
        <div>
          <h2>Capability matrix</h2>
          <p>Detected devices, runtime readiness, and current resource headroom.</p>
        </div>
        <button className="secondary-button" onClick={() => void load(true)} disabled={loading} type="button">
          <RefreshCw size={16} className={loading ? "spin" : ""} /> Refresh
        </button>
      </div>

      {error && <div className="error-banner"><AlertTriangle size={17} />{error}</div>}
      {snapshot?.warnings.map((warning) => <div className="warning-banner" key={warning}><AlertTriangle size={17} />{warning}</div>)}

      <div className="metrics-strip">
        <div><Cpu size={18} /><span>Processor</span><strong>{snapshot?.cpu.name ?? "Detecting..."}</strong><small>{snapshot ? `${snapshot.cpu.logicalCores} logical cores` : ""}</small></div>
        <div><MemoryStick size={18} /><span>Available RAM</span><strong>{formatBytes(snapshot?.memory.availableBytes ?? null)}</strong><small>{snapshot ? `${formatBytes(snapshot.memory.totalBytes)} installed` : ""}</small></div>
        <div><Gauge size={18} /><span>Active engines</span><strong>0</strong><small>No model loaded</small></div>
      </div>

      <div className="hardware-grid">
        {snapshot?.devices.map((device) => (
          <article className="device-card" key={device.id}>
            <div className="device-heading">
              <span className={`device-icon ${device.kind}`}><Cpu size={19} /></span>
              <div><small>{device.kind.toUpperCase()}</small><h3>{device.name}</h3></div>
              <span className="device-state">Detected</span>
            </div>
            <dl className="device-stats">
              <div><dt>Memory</dt><dd>{formatBytes(device.memoryTotalBytes)}</dd></div>
              <div><dt>Available</dt><dd>{formatBytes(device.memoryAvailableBytes)}</dd></div>
              <div><dt>Utilization</dt><dd>{device.utilizationPercent === null ? "Not reported" : `${device.utilizationPercent}%`}</dd></div>
              <div><dt><Thermometer size={14} /> Temperature</dt><dd>{device.temperatureCelsius === null ? "Not reported" : `${device.temperatureCelsius} C`}</dd></div>
            </dl>
          </article>
        ))}
      </div>

      <div className="capability-table-wrap">
        <div className="table-title"><h2>Inference routes</h2><span>{snapshot?.source === "demo" ? "Preview data" : "Native probes"}</span></div>
        <table className="capability-table">
          <thead><tr><th>Route</th><th>Status</th><th>Evidence</th></tr></thead>
          <tbody>
            {snapshot?.capabilities.map((capability) => (
              <tr key={capability.id}>
                <td>{capability.label}</td>
                <td><span className={`status-pill ${capability.status}`}>{statusLabels[capability.status]}</span></td>
                <td>{capability.evidence}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
