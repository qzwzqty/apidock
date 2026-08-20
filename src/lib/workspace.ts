import { create } from "zustand";
import { api, type AppSession, type TeamInfo, type ProjectInfo, type OpenTab, type WorkspaceState, type ProxyConfig } from "./api";
import { useProject } from "./project";

export const MAIN_TAB_ID = "main";

function tabId(tab: OpenTab) {
  return `project:${tab.teamKey}:${tab.projectKey}`;
}

export function sanitizeKey(raw: string): string {
  const cleaned = raw
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return cleaned;
}

interface WorkspaceStore {
  dataRoot: string | null;
  teams: TeamInfo[];
  selectedTeamKey: string | null;
  projects: ProjectInfo[];
  openTabs: OpenTab[];
  activeTab: string;
  proxy: ProxyConfig;
  bootstrapped: boolean;
  error: string | null;

  init: () => Promise<void>;
  pickDataRoot: () => Promise<void>;
  onSession: (session: AppSession) => void;
  refreshTeam: (teamKey: string) => Promise<void>;
  loadTeamsAndProjects: () => Promise<void>;
  selectTeam: (teamKey: string) => Promise<void>;
  createTeam: (name: string, description?: string) => Promise<void>;
  deleteTeam: (teamKey: string) => Promise<void>;
  renameTeam: (teamKey: string, newKey: string, newName: string) => Promise<void>;
  createProject: (teamKey: string, name: string, description?: string) => Promise<void>;
  deleteProject: (projectKey: string) => Promise<void>;
  renameProject: (teamKey: string, projectKey: string, newKey: string, newName: string) => Promise<void>;
  openProject: (teamKey: string, projectKey: string) => void;
  closeTab: (id: string) => void;
  setActiveTab: (id: string) => void;
  saveProxy: (proxy: ProxyConfig) => Promise<void>;
}

export const useWorkspace = create<WorkspaceStore>()((set, get) => ({
  dataRoot: null,
  teams: [],
  selectedTeamKey: null,
  projects: [],
  openTabs: [],
  activeTab: MAIN_TAB_ID,
  proxy: { enabled: false, kind: "system", url: "" },
  bootstrapped: false,
  error: null,

  init: async () => {
    try {
      const session = await api.getSession();
      get().onSession(session);
    } catch (e) {
      set({ error: String(e) });
    }
    set({ bootstrapped: true });
  },

  pickDataRoot: async () => {
    const path = await api.pickDataRoot();
    if (!path) return;
    const session = await api.setDataRoot(path);
    get().onSession(session);
  },

  onSession: (session) => {
    const tabs = session.workspace.openTabs.filter(
      (t) => session.teams.some((team) => team.key === t.teamKey),
    );
    const hasActive =
      session.workspace.activeTab &&
      (session.workspace.activeTab === MAIN_TAB_ID ||
        tabs.some((t) => tabId(t) === session.workspace.activeTab));
    const firstTeam = session.teams[0]?.key ?? null;
    set({
      dataRoot: session.dataRoot,
      teams: session.teams,
      selectedTeamKey: firstTeam,
      openTabs: tabs,
      activeTab: hasActive ? session.workspace.activeTab! : MAIN_TAB_ID,
      proxy: session.workspace.proxy ?? { enabled: false, kind: "system", url: "" },
    });
    if (firstTeam) {
      void get().refreshTeam(firstTeam);
    }
  },

  refreshTeam: async (teamKey) => {
    const projects = await api.listProjects(teamKey);
    set({ projects });
  },

  loadTeamsAndProjects: async () => {
    const teams = await api.listTeams();
    const selected = get().selectedTeamKey;
    let projects: ProjectInfo[] = [];
    if (selected) {
      projects = await api.listProjects(selected);
    }
    set({ teams, projects });
  },

  selectTeam: async (teamKey) => {
    set({ selectedTeamKey: teamKey });
    await get().refreshTeam(teamKey);
  },

  createTeam: async (name, description) => {
    await api.createTeam(name, description);
    const teams = await api.listTeams();
    set({ teams });
    const first = teams[0]?.key;
    if (first) await get().selectTeam(first);
  },

  deleteTeam: async (teamKey) => {
    await api.deleteTeam(teamKey);
    const teams = await api.listTeams();
    if (teamKey === get().selectedTeamKey) {
      const next = teams[0]?.key ?? null;
      set({ teams, selectedTeamKey: next, projects: [] });
      if (next) await get().refreshTeam(next);
    } else {
      set({ teams });
    }
  },

  renameTeam: async (teamKey, newKey, newName) => {
    // 若改键，需同步把该团队下已打开的标签也迁移（简化：先保持，刷新列表）
    await api.renameTeam(teamKey, sanitizeKey(newKey), newName);
    const teams = await api.listTeams();
    set({ teams });
    if (get().selectedTeamKey === teamKey) {
      set({ selectedTeamKey: teams.find((t) => t.name === newName)?.key ?? teams[0]?.key ?? null });
      const sel = get().selectedTeamKey;
      if (sel) await get().refreshTeam(sel);
    }
  },

  createProject: async (name, description) => {
    const teamKey = get().selectedTeamKey;
    if (!teamKey) return;
    await api.createProject(teamKey, name, description);
    await get().refreshTeam(teamKey);
  },

  deleteProject: async (projectKey) => {
    const teamKey = get().selectedTeamKey;
    if (!teamKey) return;
    await api.deleteProject(teamKey, projectKey);
    get().closeTab(`project:${teamKey}:${projectKey}`);
    await get().refreshTeam(teamKey);
  },

  renameProject: async (teamKey, projectKey, newKey, newName) => {
    await api.renameProject(teamKey, projectKey, sanitizeKey(newKey), newName);
    if (get().openTabs.some((t) => t.teamKey === teamKey && t.projectKey === projectKey)) {
      const { openTabs, activeTab } = get();
      const tabs = openTabs.map((t) =>
        t.teamKey === teamKey && t.projectKey === projectKey ? { ...t, projectKey: sanitizeKey(newKey) } : t,
      );
      const oldId = `project:${teamKey}:${projectKey}`;
      const newId = `project:${teamKey}:${sanitizeKey(newKey)}`;
      const nextActive = activeTab === oldId ? newId : activeTab;
      set({ openTabs: tabs, activeTab: nextActive });
      useProject.getState().dropProject(oldId);
      void persistTabs(tabs, nextActive, get().proxy);
    }
    await get().refreshTeam(teamKey);
  },

  openProject: (teamKey, projectKey) => {
    const { openTabs } = get();
    const id = tabId({ teamKey, projectKey });
    const exists = openTabs.some((t) => tabId(t) === id);
    const nextTabs = exists ? openTabs : [...openTabs, { teamKey, projectKey }];
    set({ openTabs: nextTabs, activeTab: id });
    void persistTabs(nextTabs, id, get().proxy);
  },

  closeTab: (id) => {
    const { openTabs, activeTab } = get();
    const idx = openTabs.findIndex((t) => tabId(t) === id);
    if (idx < 0) return;
    const nextTabs = openTabs.filter((_, i) => i !== idx);
    let nextActive = activeTab;
    if (activeTab === id) {
      nextActive = nextTabs[idx] ? tabId(nextTabs[idx]) : nextTabs[idx - 1] ? tabId(nextTabs[idx - 1]) : MAIN_TAB_ID;
    }
    set({ openTabs: nextTabs, activeTab: nextActive });
    useProject.getState().dropProject(id);
    void persistTabs(nextTabs, nextActive, get().proxy);
  },

  setActiveTab: (id) => {
    set({ activeTab: id });
    void persistTabs(get().openTabs, id, get().proxy);
  },
saveProxy: async (proxy) => {
    set({ proxy });
    const state: WorkspaceState = { version: 1, openTabs: get().openTabs, activeTab: get().activeTab, proxy };
    await api.saveWorkspace(state);
  },
}));

function persistTabs(openTabs: OpenTab[], activeTab: string, proxy: ProxyConfig) {
  const state: WorkspaceState = { version: 1, openTabs, activeTab, proxy };
  return api.saveWorkspace(state);
}