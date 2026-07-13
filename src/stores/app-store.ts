import { create } from "zustand";
import type { NavigationId, Theme } from "../types/domain";

interface AppStore {
  activeView: NavigationId;
  sidebarCollapsed: boolean;
  theme: Theme;
  setActiveView: (view: NavigationId) => void;
  toggleSidebar: () => void;
  setTheme: (theme: Theme) => void;
}

export const useAppStore = create<AppStore>((set) => ({
  activeView: "chat",
  sidebarCollapsed: false,
  theme: "dark",
  setActiveView: (activeView) => set({ activeView }),
  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
  setTheme: (theme) => set({ theme }),
}));
