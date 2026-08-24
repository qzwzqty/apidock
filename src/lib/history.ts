import { create } from "zustand";
import { api, type HistoryRecord, type HistorySummary } from "./api";

function summaryOf(rec: HistoryRecord): HistorySummary {
  const { doc: _doc, env: _env, globalVariables: _gv, globalParams: _gp, response: _res, error: _err, ...rest } = rec;
  return rest;
}

interface HistoryStore {
  entries: HistorySummary[];
  loaded: boolean;
  loading: boolean;
  selectedId: number | null;
  detail: HistoryRecord | null;
  detailLoading: boolean;
  /** 重发中（右栏控制再次发送按钮与加载态） */
  resending: boolean;

  load: () => Promise<void>;
  select: (id: number) => Promise<void>;
  resend: (id: number) => Promise<void>;
  remove: (id: number) => Promise<void>;
  clear: () => Promise<void>;
}

export const useHistory = create<HistoryStore>()((set, get) => ({
  entries: [],
  loaded: false,
  loading: false,
  selectedId: null,
  detail: null,
  detailLoading: false,
  resending: false,

  load: async () => {
    set({ loading: true });
    try {
      const entries = await api.listRequestHistory();
      // 保持当前选中（若记录仍存在）；否则自动选最新一条
      const sel = get().selectedId;
      const nextSel = entries.some((e) => e.id === sel)
        ? sel
        : entries[0]?.id ?? null;
      set({ entries, selectedId: nextSel, loaded: true });
      if (nextSel != null) {
        const detail = await api.getRequestHistory(nextSel);
        set({ detail });
      } else {
        set({ detail: null });
      }
    } finally {
      set({ loading: false });
    }
  },

  select: async (id) => {
    if (get().selectedId === id && get().detail?.id === id) return;
    set({ selectedId: id, detailLoading: true });
    try {
      const detail = await api.getRequestHistory(id);
      set({ detail });
    } finally {
      set({ detailLoading: false });
    }
  },

  resend: async (id) => {
    set({ resending: true });
    try {
      // 后端按快照重发并记为新历史
      const rec = await api.resendHistory(id);
      const entries = [
        summaryOf(rec),
        ...get().entries.filter((e) => e.id !== rec.id),
      ];
      set({ entries, selectedId: rec.id, detail: rec });
    } finally {
      set({ resending: false });
    }
  },

  remove: async (id) => {
    await api.deleteRequestHistory(id);
    const entries = get().entries.filter((e) => e.id !== id);
    const selectedId =
      get().selectedId === id ? (entries[0]?.id ?? null) : get().selectedId;
    set({ entries, selectedId });
    if (selectedId != null) {
      const detail = await api.getRequestHistory(selectedId);
      set({ detail });
    } else {
      set({ detail: null });
    }
  },

  clear: async () => {
    await api.clearRequestHistory();
    set({ entries: [], selectedId: null, detail: null, loaded: true });
  },
}));