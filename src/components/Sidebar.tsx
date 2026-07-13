import {
  AudioLines,
  Bot,
  Boxes,
  ChevronLeft,
  ChevronRight,
  Cpu,
  Download,
  FileText,
  Images,
  Library,
  MessageSquare,
  ScrollText,
  Settings,
  Sparkles,
} from "lucide-react";
import { useAppStore } from "../stores/app-store";
import type { NavigationId } from "../types/domain";

const primaryItems: Array<{ id: NavigationId; label: string; icon: typeof Bot }> = [
  { id: "chat", label: "Chat", icon: MessageSquare },
  { id: "images", label: "Images", icon: Sparkles },
  { id: "speech", label: "Speech", icon: AudioLines },
  { id: "tts", label: "Text to Speech", icon: Bot },
];

const libraryItems: Array<{ id: NavigationId; label: string; icon: typeof Bot }> = [
  { id: "models", label: "Model Manager", icon: Boxes },
  { id: "prompts", label: "Prompt Library", icon: FileText },
  { id: "gallery", label: "Gallery", icon: Images },
  { id: "downloads", label: "Downloads", icon: Download },
];

const systemItems: Array<{ id: NavigationId; label: string; icon: typeof Bot }> = [
  { id: "hardware", label: "Hardware", icon: Cpu },
  { id: "logs", label: "Logs", icon: ScrollText },
  { id: "settings", label: "Settings", icon: Settings },
];

function NavGroup({ items, label }: { items: typeof primaryItems; label: string }) {
  const activeView = useAppStore((state) => state.activeView);
  const setActiveView = useAppStore((state) => state.setActiveView);
  const collapsed = useAppStore((state) => state.sidebarCollapsed);

  return (
    <div className="nav-group">
      {!collapsed && <div className="nav-label">{label}</div>}
      {items.map(({ id, label: itemLabel, icon: Icon }) => (
        <button
          className={`nav-item ${activeView === id ? "active" : ""}`}
          key={id}
          onClick={() => setActiveView(id)}
          aria-label={itemLabel}
          title={collapsed ? itemLabel : undefined}
          type="button"
        >
          <Icon size={18} strokeWidth={1.8} />
          {!collapsed && <span>{itemLabel}</span>}
        </button>
      ))}
    </div>
  );
}

export function Sidebar() {
  const collapsed = useAppStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useAppStore((state) => state.toggleSidebar);

  return (
    <aside className={`sidebar ${collapsed ? "collapsed" : ""}`}>
      <div className="brand-row">
        <div className="brand-mark"><Library size={19} /></div>
        {!collapsed && <div><strong>NeuraLoc</strong><span>Core</span></div>}
      </div>
      <nav>
        <NavGroup items={primaryItems} label="Create" />
        <NavGroup items={libraryItems} label="Library" />
        <NavGroup items={systemItems} label="System" />
      </nav>
      <div className="sidebar-footer">
        {!collapsed && <div className="privacy-state"><span className="status-dot" /> Local mode</div>}
        <button className="icon-button" onClick={toggleSidebar} title={collapsed ? "Expand sidebar" : "Collapse sidebar"} type="button">
          {collapsed ? <ChevronRight size={18} /> : <ChevronLeft size={18} />}
        </button>
      </div>
    </aside>
  );
}
