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

export interface KeyValue {
  key: string;
  value: string;
  enabled: boolean;
}

export interface InterfaceFile {
  version: number;
  id: string;
  name: string;
  method: string;
  url: string;
  headers: KeyValue[];
  query: KeyValue[];
  description: string;
}

export interface InterfaceDoc {
  groupPath: string[];
  key: string;
  doc: InterfaceFile;
}

export type TreeNode =
  | { type: "group"; key: string; name: string; children: TreeNode[] }
  | { type: "interface"; key: string; name: string; method: string };

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

  listInterfaceTree: (teamKey: string, projectKey: string) =>
    invoke<TreeNode[]>("list_interface_tree", { teamKey, projectKey }),
  createGroup: (teamKey: string, projectKey: string, groupPath: string[], key: string, name: string) =>
    invoke<void>("create_group", { teamKey, projectKey, groupPath, key, name }),
  renameGroup: (teamKey: string, projectKey: string, groupPath: string[], newKey: string, newName: string) =>
    invoke<void>("rename_group", { teamKey, projectKey, groupPath, newKey, newName }),
  deleteGroup: (teamKey: string, projectKey: string, groupPath: string[]) =>
    invoke<void>("delete_group", { teamKey, projectKey, groupPath }),
  createInterface: (teamKey: string, projectKey: string, groupPath: string[], key: string, name: string) =>
    invoke<InterfaceFile>("create_interface", { teamKey, projectKey, groupPath, key, name }),
  getInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string) =>
    invoke<InterfaceFile>("get_interface", { teamKey, projectKey, groupPath, ifaceKey }),
  saveInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string, iface: InterfaceFile) =>
    invoke<void>("save_interface", { teamKey, projectKey, groupPath, ifaceKey, iface }),
  renameInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string, newName: string) =>
    invoke<void>("rename_interface", { teamKey, projectKey, groupPath, ifaceKey, newName }),
  deleteInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string) =>
    invoke<void>("delete_interface", { teamKey, projectKey, groupPath, ifaceKey }),
};