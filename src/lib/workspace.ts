import { create } from "zustand";
import { api, type AppSession, type TeamInfo, type ProjectInfo, type OpenTab, type WorkspaceState } from "./api";
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
  bootstrapped: boolean;
  error: string | null;

  init: () => Promise<void>;
  pickDataRoot: () => Promise<void>;
  onSession: (session: AppSession) => void;
  refreshTeam: (teamKey: string) => Promise<void>;
  loadTeamsAndProjects: () => Promise<void>;
  selectTeam: (teamKey: string) => Promise<void>;
  createTeam: (key: string, name: string) => Promise<void>;
  deleteTeam: (teamKey: string) => Promise<void>;
  createProject: (key: string, name: string) => Promise<void>;
  deleteProject: (projectKey: string) => Promise<void>;
  openProject: (teamKey: string, projectKey: string) => void;
  closeTab: (id: string) => void;
  setActiveTab: (id: string) => void;
}

export const useWorkspace = create<WorkspaceStore>()((set, get) => ({
  dataRoot: null,
  teams: [],
  selectedTeamKey: null,
  projects: [],
  openTabs: [],
  activeTab: MAIN_TAB_ID,
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

  createTeam: async (key, name) => {
    await api.createTeam(sanitizeKey(key), name);
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

  createProject: async (key, name) => {
    const teamKey = get().selectedTeamKey;
    if (!teamKey) return;
    await api.createProject(teamKey, sanitizeKey(key), name);
    await get().refreshTeam(teamKey);
  },

  deleteProject: async (projectKey) => {
    const teamKey = get().selectedTeamKey;
    if (!teamKey) return;
    await api.deleteProject(teamKey, projectKey);
    get().closeTab(`project:${teamKey}:${projectKey}`);
    await get().refreshTeam(teamKey);
  },

  openProject: (teamKey, projectKey) => {
    const { openTabs } = get();
    const id = tabId({ teamKey, projectKey });
    const exists = openTabs.some((t) => tabId(t) === id);
    const nextTabs = exists ? openTabs : [...openTabs, { teamKey, projectKey }];
    set({ openTabs: nextTabs, activeTab: id });
    void persistTabs(nextTabs, id);
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
    void persistTabs(nextTabs, nextActive);
  },

  setActiveTab: (id) => {
    set({ activeTab: id });
    void persistTabs(get().openTabs, id);
  },
}));

function persistTabs(openTabs: OpenTab[], activeTab: string) {
  const state: WorkspaceState = { version: 1, openTabs, activeTab };
  return api.saveWorkspace(state);
}