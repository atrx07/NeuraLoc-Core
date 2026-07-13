import {
  ArrowUp,
  AudioLines,
  Boxes,
  Download,
  FilePlus2,
  FileText,
  ImagePlus,
  Mic,
  Plus,
  Search,
  SlidersHorizontal,
  Sparkles,
  Square,
} from "lucide-react";
import { useAppStore } from "../../stores/app-store";
import type { NavigationId } from "../../types/domain";
import { ModelManagerView } from "../models/ModelManagerView";

function EmptyState({ icon: Icon, title, detail, action, onAction }: { icon: typeof Boxes; title: string; detail: string; action: string; onAction?: () => void }) {
  return <div className="empty-state"><span><Icon size={27} /></span><h2>{title}</h2><p>{detail}</p><button className="primary-button" onClick={onAction} type="button"><Plus size={16} />{action}</button></div>;
}

function ChatWorkspace() {
  const setActiveView = useAppStore((state) => state.setActiveView);
  return <div className="chat-layout">
    <aside className="conversation-rail"><button className="new-chat-button" type="button"><Plus size={17} /> New conversation</button><div className="rail-search"><Search size={15} /><input aria-label="Search conversations" placeholder="Search conversations" /></div><div className="conversation-empty">No conversations yet</div></aside>
    <div className="chat-workspace">
      <div className="chat-controls">
        <label>Model<select defaultValue="none"><option value="none">No model selected</option><option>Install another model...</option></select></label>
        <label>System prompt<select defaultValue="default"><option value="default">Default assistant</option><option>Manage prompt library...</option></select></label>
        <button className="icon-button" title="Generation settings" type="button"><SlidersHorizontal size={18} /></button>
      </div>
      <div className="chat-empty"><div className="brand-orbit"><Sparkles size={26} /></div><h2>Start a local conversation</h2><p>Select an installed GGUF model and a system prompt. Messages stay on this device.</p><button className="secondary-button" onClick={() => setActiveView("models")} type="button"><Download size={16} /> Find a model</button></div>
      <div className="composer"><textarea aria-label="Message" disabled placeholder="Install or import a model to begin" /><div className="composer-actions"><button className="icon-button" disabled title="Attach image" type="button"><ImagePlus size={18} /></button><button className="send-button" disabled title="Send message" type="button"><ArrowUp size={18} /></button></div></div>
    </div>
  </div>;
}

function PromptWorkspace() {
  return <div className="library-workspace"><div className="section-toolbar"><div><h2>System prompts</h2><p>Versioned profiles control behavior without changing application permissions.</p></div><div className="toolbar-actions"><button className="secondary-button" type="button"><FilePlus2 size={16} /> Import</button><button className="primary-button" type="button"><Plus size={16} /> New prompt</button></div></div><div className="rail-search wide"><Search size={15} /><input aria-label="Search prompts" placeholder="Search prompts and tags" /></div><EmptyState icon={FileText} title="Your prompt library is empty" detail="Create a profile or import Markdown with optional YAML front matter." action="Create prompt" /></div>;
}

const emptyByView: Partial<Record<NavigationId, { icon: typeof Boxes; title: string; detail: string; action: string }>> = {
  images: { icon: Sparkles, title: "No image model loaded", detail: "Choose a compatible image model before starting a generation.", action: "Choose model" },
  speech: { icon: Mic, title: "Ready for a speech model", detail: "Install Whisper and select a local model to record or import audio.", action: "Set up speech" },
  tts: { icon: AudioLines, title: "No voice runtime installed", detail: "Install a verified Kokoro package to synthesize speech locally.", action: "Set up voices" },
  gallery: { icon: ImagePlus, title: "No generated outputs", detail: "Images, transcripts, and speech files will appear here.", action: "Open Image Studio" },
  downloads: { icon: Download, title: "No active downloads", detail: "Verified model downloads and their progress will appear here.", action: "Browse models" },
  logs: { icon: FileText, title: "No engine logs", detail: "Owned process output and diagnostic events will appear after an engine starts.", action: "Open Hardware" },
};

export function WorkspaceView({ view }: { view: NavigationId }) {
  const setActiveView = useAppStore((state) => state.setActiveView);
  if (view === "chat") return <ChatWorkspace />;
  if (view === "models") return <ModelManagerView />;
  if (view === "prompts") return <PromptWorkspace />;
  const state = emptyByView[view];
  if (!state) return null;
  const destinations: Partial<Record<NavigationId, NavigationId>> = { images: "models", speech: "models", tts: "models", gallery: "images", downloads: "models", logs: "hardware" };
  return <div className="single-workspace"><EmptyState {...state} onAction={() => setActiveView(destinations[view] ?? "chat")} />{view === "images" && <div className="generation-dock"><div><label>Prompt<textarea disabled placeholder="Load a model to unlock generation controls" /></label><label>Negative prompt<input disabled /></label></div><button disabled className="primary-button" type="button"><Square size={15} /> Generate</button></div>}</div>;
}
