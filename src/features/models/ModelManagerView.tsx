import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Boxes,
  CheckCircle2,
  FilePlus2,
  FolderSearch,
  LoaderCircle,
  RefreshCw,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { bridge } from "../../services/bridge";
import type {
  ModelRecord,
  ModelScanProgress,
  ModelScanSummary,
  ModelVerificationState,
} from "../../types/domain";
import { formatBytes } from "../../utils/format";
import { modelMetadataLabels } from "./model-format";

const stateLabels: Record<ModelVerificationState, string> = {
  metadata_pending: "Inspecting",
  ready: "Ready",
  invalid: "Invalid",
  missing: "Missing",
};

export function ModelManagerView() {
  const [models, setModels] = useState<ModelRecord[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [operation, setOperation] = useState<"import" | "scan" | null>(null);
  const [workingModelId, setWorkingModelId] = useState<string | null>(null);
  const [progress, setProgress] = useState<ModelScanProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const loadModels = useCallback(async (showLoading = true) => {
    if (showLoading) setLoading(true);
    try {
      setModels(await bridge.listModels());
    } catch (caught) {
      setError(errorMessage(caught, "The model library could not be loaded."));
    } finally {
      if (showLoading) setLoading(false);
    }
  }, []);

  useEffect(() => { void loadModels(); }, [loadModels]);

  const filteredModels = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    if (!needle) return models;
    return models.filter((model) => [
      model.displayName,
      model.path,
      model.family ?? "",
      model.ggufMetadata?.quantization ?? "",
      model.verificationState,
    ].some((value) => value.toLocaleLowerCase().includes(needle)));
  }, [models, query]);

  const readyCount = models.filter((model) => model.verificationState === "ready").length;
  const totalBytes = models.reduce((total, model) => total + model.sizeBytes, 0);

  async function importFile() {
    clearMessages();
    try {
      const path = await bridge.chooseModelFile();
      if (!path) return;
      setOperation("import");
      const outcome = await bridge.importModel(path);
      await loadModels(false);
      if (outcome.model.verificationState === "invalid") {
        setError(outcome.model.verificationError ?? "The selected GGUF file is invalid.");
      } else {
        setNotice(outcome.alreadyIndexed
          ? `${outcome.model.displayName} was already indexed and has been reverified.`
          : `${outcome.model.displayName} was added to the local library.`);
      }
    } catch (caught) {
      setError(errorMessage(caught, "The model could not be imported."));
    } finally {
      setOperation(null);
    }
  }

  async function scanFolder() {
    clearMessages();
    let unlisten: () => void = () => {};
    try {
      const path = await bridge.chooseModelFolder();
      if (!path) return;
      const scanId = crypto.randomUUID();
      setOperation("scan");
      setProgress(emptyProgress(scanId));
      unlisten = await bridge.onModelScanProgress(scanId, setProgress);
      const summary = await bridge.scanModelFolder(scanId, path);
      await loadModels(false);
      setNotice(scanSummary(summary));
      if (summary.issues.length > 0) {
        setError(`${summary.invalid} model file(s) failed verification. Invalid records remain visible for inspection.`);
      }
    } catch (caught) {
      setError(errorMessage(caught, "The model folder scan failed."));
    } finally {
      unlisten();
      setOperation(null);
      setProgress(null);
    }
  }

  async function cancelScan() {
    if (!progress) return;
    await bridge.cancelModelScan(progress.scanId);
  }

  async function reverify(model: ModelRecord) {
    clearMessages();
    setWorkingModelId(model.id);
    try {
      const updated = await bridge.reverifyModel(model.id);
      await loadModels(false);
      setNotice(`${updated.displayName} verification is ${stateLabels[updated.verificationState].toLocaleLowerCase()}.`);
    } catch (caught) {
      setError(errorMessage(caught, "The model could not be reverified."));
    } finally {
      setWorkingModelId(null);
    }
  }

  async function removeRecord(model: ModelRecord) {
    clearMessages();
    if (!await bridge.confirmRemoveModel(model.displayName)) return;
    setWorkingModelId(model.id);
    try {
      await bridge.removeModelRecord(model.id);
      await loadModels(false);
      setNotice(`${model.displayName} was removed from the library. Its GGUF file was not deleted.`);
    } catch (caught) {
      setError(errorMessage(caught, "The model record could not be removed."));
    } finally {
      setWorkingModelId(null);
    }
  }

  function clearMessages() {
    setError(null);
    setNotice(null);
  }

  return (
    <div className="library-workspace model-library">
      <div className="section-toolbar">
        <div><h2>Local model library</h2><p>GGUF files are indexed in place and inspected before any engine can load them.</p></div>
        <div className="toolbar-actions">
          <button className="secondary-button" disabled={operation !== null} onClick={() => void scanFolder()} type="button">
            <FolderSearch size={16} /> Scan folder
          </button>
          <button className="primary-button" disabled={operation !== null} onClick={() => void importFile()} type="button">
            {operation === "import" ? <LoaderCircle className="spin" size={16} /> : <FilePlus2 size={16} />} Import GGUF
          </button>
        </div>
      </div>

      <div className="tab-row">
        <button className="active" type="button">Installed</button>
        <button disabled title="The verified catalog arrives in a later checkpoint" type="button">Catalog</button>
        <button disabled title="Downloads arrive with the verified catalog" type="button">Downloads</button>
      </div>

      {error && <div className="error-banner"><AlertTriangle size={17} /><span>{error}</span><button aria-label="Dismiss error" onClick={() => setError(null)} type="button"><X size={15} /></button></div>}
      {notice && <div className="notice-banner"><CheckCircle2 size={17} /><span>{notice}</span><button aria-label="Dismiss notice" onClick={() => setNotice(null)} type="button"><X size={15} /></button></div>}

      {progress && <ScanProgress progress={progress} onCancel={() => void cancelScan()} />}

      <div className="library-summary">
        <div><span>Indexed</span><strong>{models.length}</strong></div>
        <div><span>Ready</span><strong>{readyCount}</strong></div>
        <div><span>Library size</span><strong>{formatBytes(totalBytes)}</strong></div>
        <label className="model-search"><Search size={15} /><input aria-label="Search models" onChange={(event) => setQuery(event.target.value)} placeholder="Search name, family, quantization, or path" value={query} /></label>
      </div>

      {loading ? <div className="loading-state"><LoaderCircle className="spin" size={18} /> Loading model library...</div> : filteredModels.length > 0 ? (
        <div className="model-table-wrap">
          <table className="model-table">
            <thead><tr><th>Model</th><th>Metadata</th><th>Size</th><th>Verification</th><th><span className="sr-only">Actions</span></th></tr></thead>
            <tbody>{filteredModels.map((model) => {
              const metadata = modelMetadataLabels(model);
              const working = workingModelId === model.id;
              return <tr key={model.id}>
                <td><div className="model-primary"><strong>{model.displayName}</strong><span title={model.path}>{model.path}</span></div></td>
                <td><div className="model-metadata">{metadata.length > 0 ? metadata.map((label) => <span key={label}>{label}</span>) : <em>Metadata unavailable</em>}</div></td>
                <td className="model-size">{formatBytes(model.sizeBytes)}</td>
                <td><div className="verification-cell"><span className={`model-state ${model.verificationState}`}>{stateLabels[model.verificationState]}</span>{model.verificationError && <small title={model.verificationError}>{model.verificationError}</small>}</div></td>
                <td><div className="row-actions">
                  <button className="icon-button" disabled={working} onClick={() => void reverify(model)} title="Reverify model" type="button">{working ? <LoaderCircle className="spin" size={16} /> : <RefreshCw size={16} />}</button>
                  <button className="icon-button danger-action" disabled={working} onClick={() => void removeRecord(model)} title="Remove library record" type="button"><Trash2 size={16} /></button>
                </div></td>
              </tr>;
            })}</tbody>
          </table>
        </div>
      ) : models.length === 0 ? (
        <div className="empty-state"><span><Boxes size={27} /></span><h2>No local models indexed</h2><p>Import one GGUF file or scan a folder. Files remain in their original location.</p><button className="primary-button" onClick={() => void importFile()} type="button"><FilePlus2 size={16} /> Import GGUF</button></div>
      ) : (
        <div className="empty-state"><span><Search size={27} /></span><h2>No matching models</h2><p>Try a different name, family, quantization, or path.</p><button className="secondary-button" onClick={() => setQuery("")} type="button"><X size={16} /> Clear search</button></div>
      )}
    </div>
  );
}

function ScanProgress({ progress, onCancel }: { progress: ModelScanProgress; onCancel: () => void }) {
  const discovering = progress.phase === "discovering";
  const title = discovering ? `Discovering GGUF files (${progress.discovered})` : `Indexing ${progress.processed} of ${progress.discovered}`;
  const detail = progress.currentPath?.split(/[\\/]/).pop() ?? "Preparing scan";
  return <div className="scan-progress"><LoaderCircle className="spin" size={18} /><div><strong>{title}</strong><span title={progress.currentPath ?? undefined}>{detail}</span><progress max={Math.max(progress.discovered, 1)} value={discovering ? 0 : progress.processed} /></div><button className="secondary-button" onClick={onCancel} type="button"><X size={15} /> Cancel</button></div>;
}

function emptyProgress(scanId: string): ModelScanProgress {
  return { scanId, phase: "discovering", currentPath: null, discovered: 0, processed: 0, imported: 0, duplicates: 0, invalid: 0 };
}

function scanSummary(summary: ModelScanSummary): string {
  if (summary.cancelled) return `Scan cancelled after ${summary.processed} of ${summary.discovered} discovered model(s).`;
  return `Scan complete: ${summary.imported} added, ${summary.duplicates} already indexed, ${summary.invalid} invalid.`;
}

function errorMessage(caught: unknown, fallback: string): string {
  if (typeof caught === "string") return caught;
  if (caught && typeof caught === "object" && "message" in caught && typeof caught.message === "string") return caught.message;
  return fallback;
}
