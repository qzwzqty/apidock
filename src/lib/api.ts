import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export interface TeamInfo {
  key: string;
  name: string;
}

export interface ProjectInfo {
  key: string;
  name: string;
}

export interface OpenTab {
  teamKey: string;
  projectKey: string;
}

export interface WorkspaceState {
  version: number;
  openTabs: OpenTab[];
  activeTab: string | null;
}

export interface AppSession {
  dataRoot: string | null;
  teams: TeamInfo[];
  workspace: WorkspaceState;
}

export const api = {
  getSession: () => invoke<AppSession>("get_session"),
  pickDataRoot: async (): Promise<string | null> => {
    const picked = await open({ directory: true, multiple: false, title: "选择数据根目录" });
    if (typeof picked !== "string" || !picked) return null;
    await invoke<AppSession>("set_data_root", { path: picked });
    return picked;
  },
  setDataRoot: (path: string) => invoke<AppSession>("set_data_root", { path }),
  listTeams: () => invoke<TeamInfo[]>("list_teams"),
  listProjects: (teamKey: string) => invoke<ProjectInfo[]>("list_projects", { teamKey }),
  createTeam: (key: string, name: string) =>
    invoke<TeamInfo>("create_team", { key, name }),
  createProject: (teamKey: string, key: string, name: string) =>
    invoke<ProjectInfo>("create_project", { teamKey, key, name }),
  deleteTeam: (teamKey: string) => invoke<void>("delete_team", { teamKey }),
  deleteProject: (teamKey: string, projectKey: string) =>
    invoke<void>("delete_project", { teamKey, projectKey }),
  saveWorkspace: (workspace: WorkspaceState) =>
    invoke<void>("save_workspace", { workspace }),
};