import { create } from "zustand";
import { api, type TreeNode, type InterfaceFile, type InterfaceDoc } from "./api";

export function ifaceId(groupPath: string[], key: string): string {
  return [...groupPath, key].join("/");
}

export interface InterfaceTab {
  id: string;
  groupPath: string[];
  key: string;
  name: string;
}

interface ProjState {
  tree: TreeNode[];
  loaded: boolean;
  openTabs: InterfaceTab[];
  activeTab: string | null;
  docs: Record<string, InterfaceFile>;
  /** 新建接口后新开标签的期望初始模式（打开即进入编辑） */
  openInEditId: string | null;
}

function emptyProj(): ProjState {
  return { tree: [], loaded: false, openTabs: [], activeTab: null, docs: {}, openInEditId: null };
}

interface ProjectStore {
  states: Record<string, ProjState>;
  loadTree: (tabId: string, teamKey: string, projectKey: string) => Promise<void>;
  openInterface: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], key: string) => Promise<void>;
  closeInterface: (tabId: string, id: string) => void;
  setActive: (tabId: string, id: string) => void;
  refresh: (tabId: string, teamKey: string, projectKey: string) => Promise<void>;
  createGroup: (tabId: string, teamKey: string, projectKey: string, parentPath: string[], name: string, description?: string) => Promise<void>;
  renameGroup: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], newName: string) => Promise<void>;
  deleteGroup: (tabId: string, teamKey: string, projectKey: string, groupPath: string[]) => Promise<void>;
  createInterface: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], name: string, description?: string) => Promise<void>;
  renameInterface: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], key: string, name: string) => Promise<void>;
  deleteInterface: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], key: string) => Promise<void>;
  saveDoc: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], key: string, doc: InterfaceFile) => Promise<void>;
  dropProject: (tabId: string) => void;
}

export const useProject = create<ProjectStore>()((set, get) => ({
  states: {},

  loadTree: async (tabId, teamKey, projectKey) => {
    const tree = await api.listInterfaceTree(teamKey, projectKey);
    set((s) => ({
      states: {
        ...s.states,
        [tabId]: { ...(s.states[tabId] ?? emptyProj()), tree, loaded: true },
      },
    }));
  },

  openInterface: async (tabId, teamKey, projectKey, groupPath, key) => {
    const id = ifaceId(groupPath, key);
    const cur = get().states[tabId];
    if (!cur?.openTabs.some((t) => t.id === id)) {
      const doc = await api.getInterface(teamKey, projectKey, groupPath, key);
      const tab: InterfaceTab = { id, groupPath, key, name: doc.name };
      set((s) => ({
        states: {
          ...s.states,
          [tabId]: {
            ...(s.states[tabId] ?? emptyProj()),
            openTabs: [...(s.states[tabId]?.openTabs ?? []), tab],
            activeTab: id,
            docs: { ...(s.states[tabId]?.docs ?? {}), [id]: doc },
          },
        },
      }));
    } else {
      set((s) => ({
        states: {
          ...s.states,
          [tabId]: { ...s.states[tabId]!, activeTab: id },
        },
      }));
    }
  },

  closeInterface: (tabId, id) => {
    set((s) => {
      const st = s.states[tabId];
      if (!st) return {};
      const idx = st.openTabs.findIndex((t) => t.id === id);
      if (idx < 0) return {};
      const tabs = st.openTabs.filter((_, i) => i !== idx);
      let active = st.activeTab;
      if (active === id) {
        active = tabs[idx]?.id ?? tabs[idx - 1]?.id ?? null;
      }
      const docs = { ...st.docs };
      delete docs[id];
      return { states: { ...s.states, [tabId]: { ...st, openTabs: tabs, activeTab: active, docs } } };
    });
  },

  setActive: (tabId, id) => {
    set((s) => {
      const st = s.states[tabId];
      if (!st) return {};
      return { states: { ...s.states, [tabId]: { ...st, activeTab: id } } };
    });
  },

  refresh: async (tabId, teamKey, projectKey) => {
    await get().loadTree(tabId, teamKey, projectKey);
  },

  createGroup: async (tabId, teamKey, projectKey, parentPath, name, description) => {
    await api.createGroup(teamKey, projectKey, parentPath, name, description);
    await get().loadTree(tabId, teamKey, projectKey);
  },

  renameGroup: async (tabId, teamKey, projectKey, groupPath, newName) => {
    await api.renameGroup(teamKey, projectKey, groupPath, newName);
    await get().loadTree(tabId, teamKey, projectKey);
  },

  deleteGroup: async (tabId, teamKey, projectKey, groupPath) => {
    await api.deleteGroup(teamKey, projectKey, groupPath);
    set((s) => {
      const st = s.states[tabId];
      if (!st) return {};
      const prefix = ifaceId(groupPath, "");
      const openTabs = st.openTabs.filter((t) => !t.id.startsWith(prefix));
      const docs = { ...st.docs };
      for (const t of st.openTabs) if (!openTabs.includes(t)) delete docs[t.id];
      let active = st.activeTab;
      if (active && !openTabs.some((t) => t.id === active)) active = null;
      return { states: { ...s.states, [tabId]: { ...st, openTabs, docs, activeTab: active } } };
    });
    await get().loadTree(tabId, teamKey, projectKey);
  },

  createInterface: async (tabId, teamKey, projectKey, groupPath, name, description) => {
    const created = await api.createInterface(teamKey, projectKey, groupPath, name, description);
    await get().loadTree(tabId, teamKey, projectKey);
    await get().openInterface(tabId, teamKey, projectKey, groupPath, created.key);
    const id = ifaceId(groupPath, created.key);
    set((s) => {
      const st = s.states[tabId];
      if (!st) return {};
      return { states: { ...s.states, [tabId]: { ...st, openInEditId: id } } };
    });
  },

  renameInterface: async (tabId, teamKey, projectKey, groupPath, key, name) => {
    const newKey = await api.renameInterface(teamKey, projectKey, groupPath, key, name);
    set((s) => {
      const st = s.states[tabId];
      if (!st) return {};
      const oldId = ifaceId(groupPath, key);
      const newId = ifaceId(groupPath, newKey);
      const openTabs = st.openTabs.map((t) => (t.id === oldId ? { ...t, id: newId, key: newKey, name } : t));
      const docs = { ...st.docs };
      if (docs[oldId]) {
        docs[newId] = { ...docs[oldId], name };
        delete docs[oldId];
      }
      const activeTab = st.activeTab === oldId ? newId : st.activeTab;
      return { states: { ...s.states, [tabId]: { ...st, openTabs, docs, activeTab } } };
    });
    await get().loadTree(tabId, teamKey, projectKey);
  },

  deleteInterface: async (tabId, teamKey, projectKey, groupPath, key) => {
    await api.deleteInterface(teamKey, projectKey, groupPath, key);
    const id = ifaceId(groupPath, key);
    get().closeInterface(tabId, id);
    await get().loadTree(tabId, teamKey, projectKey);
  },

  saveDoc: async (tabId, teamKey, projectKey, groupPath, key, doc) => {
    await api.saveInterface(teamKey, projectKey, groupPath, key, doc);
    const id = ifaceId(groupPath, key);
    set((s) => {
      const st = s.states[tabId];
      if (!st) return {};
      const docs = { ...st.docs, [id]: doc };
      const openTabs = st.openTabs.map((t) => (t.id === id ? { ...t, name: doc.name } : t));
      return { states: { ...s.states, [tabId]: { ...st, docs, openTabs } } };
    });
  },

  dropProject: (tabId) => {
    set((s) => {
      const states = { ...s.states };
      delete states[tabId];
      return { states };
    });
  },
}));

export type { InterfaceDoc, TreeNode };