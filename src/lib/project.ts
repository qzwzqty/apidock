import { create } from "zustand";
import {
  api,
  type TreeNode,
  type InterfaceFile,
  type InterfaceDoc,
  type QuickTree,
} from "./api";

export function ifaceId(groupPath: string[], key: string): string {
  return [...groupPath, key].join("/");
}

/** 快捷请求标签页 id（接口键不允许包含冒号，因此不会与接口标签冲突） */
export function quickTabId(id: number): string {
  return `quick:${id}`;
}

export interface InterfaceTab {
  id: string;
  groupPath: string[];
  key: string;
  name: string;
  /** 标签来源：项目接口 或 快捷请求（与环境无关） */
  kind: "iface" | "quick";
}

function emptyQuickTree(): QuickTree {
  return { groups: [], requests: [] };
}

interface ProjState {
  tree: TreeNode[];
  /** 快捷请求树（分组 + 请求；与环境无关，URL 由用户填写完整地址） */
  quick: QuickTree;
  loaded: boolean;
  openTabs: InterfaceTab[];
  activeTab: string | null;
  docs: Record<string, InterfaceFile>;
  /** 新建接口后新开标签的期望初始模式（打开即进入编辑） */
  openInEditId: string | null;
}

function emptyProj(): ProjState {
  return {
    tree: [],
    quick: emptyQuickTree(),
    loaded: false,
    openTabs: [],
    activeTab: null,
    docs: {},
    openInEditId: null,
  };
}

interface ProjectStore {
  states: Record<string, ProjState>;
  loadTree: (tabId: string, teamKey: string, projectKey: string) => Promise<void>;
  openInterface: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], key: string) => Promise<void>;
  closeInterface: (tabId: string, id: string) => void;
  setActive: (tabId: string, id: string) => void;
  refresh: (tabId: string, teamKey: string, projectKey: string) => Promise<void>;
  createGroup: (tabId: string, teamKey: string, projectKey: string, parentPath: string[], name: string, description?: string) => Promise<void>;
  moveGroup: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], targetGroupPath: string[], beforeKey?: string | null) => Promise<void>;
  renameGroup: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], newName: string) => Promise<void>;
  deleteGroup: (tabId: string, teamKey: string, projectKey: string, groupPath: string[]) => Promise<void>;
  createInterface: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], name: string, description?: string) => Promise<void>;
  moveInterface: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], key: string, targetGroupPath: string[], beforeKey?: string | null) => Promise<void>;
  renameInterface: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], key: string, name: string) => Promise<void>;
  deleteInterface: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], key: string) => Promise<void>;
  saveDoc: (tabId: string, teamKey: string, projectKey: string, groupPath: string[], key: string, doc: InterfaceFile) => Promise<void>;
  openQuickRequest: (tabId: string, teamKey: string, projectKey: string, id: number) => Promise<void>;
  createQuickGroup: (tabId: string, teamKey: string, projectKey: string, parentId: number | null, name?: string) => Promise<void>;
  moveQuickGroup: (tabId: string, teamKey: string, projectKey: string, id: number, parentId: number | null, beforeId?: number | null) => Promise<void>;
  renameQuickGroup: (tabId: string, teamKey: string, projectKey: string, id: number, name: string) => Promise<void>;
  deleteQuickGroup: (tabId: string, teamKey: string, projectKey: string, id: number) => Promise<void>;
  createQuickRequest: (tabId: string, teamKey: string, projectKey: string, groupId?: number | null, name?: string) => Promise<void>;
  copyQuickRequest: (tabId: string, teamKey: string, projectKey: string, id: number) => Promise<void>;
  moveQuickRequest: (tabId: string, teamKey: string, projectKey: string, id: number, groupId: number | null, beforeId?: number | null) => Promise<void>;
  renameQuickRequest: (tabId: string, teamKey: string, projectKey: string, id: number, name: string) => Promise<void>;
  deleteQuickRequest: (tabId: string, teamKey: string, projectKey: string, id: number) => Promise<void>;
  saveQuickDoc: (tabId: string, teamKey: string, projectKey: string, id: number, doc: InterfaceFile) => Promise<void>;
  dropProject: (tabId: string) => void;
}

export const useProject = create<ProjectStore>()((set, get) => ({
  states: {},

  loadTree: async (tabId, teamKey, projectKey) => {
    const [tree, quick] = await Promise.all([
      api.listInterfaceTree(teamKey, projectKey),
      api.listQuickTree(teamKey, projectKey),
    ]);
    set((s) => {
      const st = s.states[tabId] ?? emptyProj();
      // 快捷请求可能已被删除（本端或其它入口）：回收其标签页，避免留下打不开的标签
      const alive = new Set(quick.requests.map((q) => quickTabId(q.id)));
      const dropped = st.openTabs.filter((t) => t.kind === "quick" && !alive.has(t.id));
      const openTabs = dropped.length
        ? st.openTabs.filter((t) => !dropped.some((d) => d.id === t.id))
        : st.openTabs;
      let docs = st.docs;
      if (dropped.length) {
        docs = { ...docs };
        for (const d of dropped) delete docs[d.id];
      }
      const activeTab =
        openTabs.some((t) => t.id === st.activeTab) ? st.activeTab : (openTabs[openTabs.length - 1]?.id ?? null);
      return {
        states: {
          ...s.states,
          [tabId]: { ...st, tree, quick, openTabs, docs, activeTab, loaded: true },
        },
      };
    });
  },

  openInterface: async (tabId, teamKey, projectKey, groupPath, key) => {
    const id = ifaceId(groupPath, key);
    const cur = get().states[tabId];
    if (!cur?.openTabs.some((t) => t.id === id)) {
      const doc = await api.getInterface(teamKey, projectKey, groupPath, key);
      const tab: InterfaceTab = { id, groupPath, key, name: doc.name, kind: "iface" };
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

  /**
   * 移动分组（同层排序 / 改父）。整棵子树的路径都会变，因此已打开标签页的
   * `groupPath` 需要按前缀同步，否则再保存会指向旧路径。
   */
  moveGroup: async (tabId, teamKey, projectKey, groupPath, targetGroupPath, beforeKey = null) => {
    await api.moveGroup(teamKey, projectKey, groupPath, targetGroupPath, beforeKey);
    const prefix = groupPath.length;
    set((s) => {
      const st = s.states[tabId];
      if (!st) return {};
      const key = (p: string[]) => p.join("/");
      const openTabs = st.openTabs.map((t) => {
        if (t.kind !== "iface" || t.groupPath.length < prefix) return t;
        if (key(t.groupPath.slice(0, prefix)) !== key(groupPath)) return t;
        const rest = t.groupPath.slice(prefix);
        const groupPath2 = [...targetGroupPath, ...rest];
        return { ...t, groupPath: groupPath2, id: ifaceId(groupPath2, t.key) };
      });
      const docs: typeof st.docs = {};
      st.openTabs.forEach((t, i) => {
        const doc = st.docs[t.id];
        if (doc) docs[openTabs[i].id] = doc;
      });
      const active = st.openTabs.findIndex((t) => t.id === st.activeTab);
      return {
        states: {
          ...s.states,
          [tabId]: {
            ...st,
            openTabs,
            docs,
            activeTab: active >= 0 ? openTabs[active].id : st.activeTab,
            openInEditId:
              active >= 0 && st.openInEditId === st.openTabs[active].id
                ? openTabs[active].id
                : st.openInEditId,
          },
        },
      };
    });
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

  /**
   * 移动接口到目标分组；`beforeKey` 指定插入到同组某接口之前（null = 末尾），
   * 因此同分组调用即实现拖拽排序。已打开的标签页会跟着改路径（键不变）。
   */
  moveInterface: async (tabId, teamKey, projectKey, groupPath, key, targetGroupPath, beforeKey = null) => {
    const newKey = await api.moveInterface(teamKey, projectKey, groupPath, key, targetGroupPath, beforeKey);
    set((s) => {
      const st = s.states[tabId];
      if (!st) return {};
      const oldId = ifaceId(groupPath, key);
      const newId = ifaceId(targetGroupPath, newKey);
      if (oldId === newId) return {};
      const openTabs = st.openTabs.map((t) =>
        t.id === oldId ? { ...t, id: newId, key: newKey, groupPath: targetGroupPath } : t,
      );
      const docs = { ...st.docs };
      if (docs[oldId]) {
        docs[newId] = docs[oldId];
        delete docs[oldId];
      }
      return {
        states: {
          ...s.states,
          [tabId]: {
            ...st,
            openTabs,
            docs,
            activeTab: st.activeTab === oldId ? newId : st.activeTab,
            openInEditId: st.openInEditId === oldId ? newId : st.openInEditId,
          },
        },
      };
    });
    await get().loadTree(tabId, teamKey, projectKey);
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

  openQuickRequest: async (tabId, teamKey, projectKey, id) => {
    const tid = quickTabId(id);
    const cur = get().states[tabId];
    if (cur?.openTabs.some((t) => t.id === tid)) {
      set((s) => ({
        states: { ...s.states, [tabId]: { ...s.states[tabId]!, activeTab: tid } },
      }));
      return;
    }
    const doc = await api.getQuickRequest(teamKey, projectKey, id);
    const summary = get().states[tabId]?.quick.requests.find((q) => q.id === id);
    const tab: InterfaceTab = {
      id: tid,
      groupPath: [],
      key: String(id),
      name: summary?.name || doc.name,
      kind: "quick",
    };
    set((s) => ({
      states: {
        ...s.states,
        [tabId]: {
          ...(s.states[tabId] ?? emptyProj()),
          openTabs: [...(s.states[tabId]?.openTabs ?? []), tab],
          activeTab: tid,
          docs: { ...(s.states[tabId]?.docs ?? {}), [tid]: doc },
        },
      },
    }));
  },

  createQuickGroup: async (tabId, teamKey, projectKey, parentId, name = "") => {
    await api.createQuickGroup(teamKey, projectKey, parentId, name);
    await get().loadTree(tabId, teamKey, projectKey);
  },

  /** 移动快捷请求分组（排序 / 改父）；标签页按请求 id 定位，不受分组变动影响 */
  moveQuickGroup: async (tabId, teamKey, projectKey, id, parentId, beforeId = null) => {
    await api.moveQuickGroup(teamKey, projectKey, id, parentId, beforeId);
    await get().loadTree(tabId, teamKey, projectKey);
  },

  renameQuickGroup: async (tabId, teamKey, projectKey, id, name) => {
    await api.renameQuickGroup(teamKey, projectKey, id, name);
    await get().loadTree(tabId, teamKey, projectKey);
  },

  deleteQuickGroup: async (tabId, teamKey, projectKey, id) => {
    await api.deleteQuickGroup(teamKey, projectKey, id);
    await get().loadTree(tabId, teamKey, projectKey);
  },

  createQuickRequest: async (tabId, teamKey, projectKey, groupId = null, name = "") => {
    const created = await api.createQuickRequest(teamKey, projectKey, groupId, name);
    await get().loadTree(tabId, teamKey, projectKey);
    await get().openQuickRequest(tabId, teamKey, projectKey, created.id);
  },

  copyQuickRequest: async (tabId, teamKey, projectKey, id) => {
    await api.copyQuickRequest(teamKey, projectKey, id);
    await get().loadTree(tabId, teamKey, projectKey);
  },

  moveQuickRequest: async (tabId, teamKey, projectKey, id, groupId, beforeId = null) => {
    await api.moveQuickRequest(teamKey, projectKey, id, groupId, beforeId);
    await get().loadTree(tabId, teamKey, projectKey);
  },

  renameQuickRequest: async (tabId, teamKey, projectKey, id, name) => {
    const newName = await api.renameQuickRequest(teamKey, projectKey, id, name);
    const tid = quickTabId(id);
    set((s) => {
      const st = s.states[tabId];
      if (!st) return {};
      const doc = st.docs[tid];
      return {
        states: {
          ...s.states,
          [tabId]: {
            ...st,
            quick: {
              ...st.quick,
              requests: st.quick.requests.map((q) => (q.id === id ? { ...q, name: newName } : q)),
            },
            openTabs: st.openTabs.map((t) => (t.id === tid ? { ...t, name: newName } : t)),
            docs: doc ? { ...st.docs, [tid]: { ...doc, name: newName } } : st.docs,
          },
        },
      };
    });
  },

  deleteQuickRequest: async (tabId, teamKey, projectKey, id) => {
    await api.deleteQuickRequest(teamKey, projectKey, id);
    // loadTree 会回收已消失的快捷请求标签页（与分组级联删除共用同一条回收逻辑）
    await get().loadTree(tabId, teamKey, projectKey);
  },

  saveQuickDoc: async (tabId, teamKey, projectKey, id, doc) => {
    await api.saveQuickRequest(teamKey, projectKey, id, doc);
    const tid = quickTabId(id);
    set((s) => {
      const st = s.states[tabId];
      if (!st) return {};
      return {
        states: {
          ...s.states,
          [tabId]: {
            ...st,
            docs: { ...st.docs, [tid]: doc },
            quick: {
              ...st.quick,
              requests: st.quick.requests.map((q) =>
                q.id === id ? { ...q, method: doc.method, url: doc.url } : q,
              ),
            },
          },
        },
      };
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