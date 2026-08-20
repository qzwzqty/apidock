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

export interface ProxyConfig {
  enabled: boolean;
  kind: string; // system | custom | none
  url: string;
}

export interface WorkspaceState {
  version: number;
  openTabs: OpenTab[];
  activeTab: string | null;
  proxy?: ProxyConfig;
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

export interface Body {
  mode: string;
  content: string;
  contentType: string;
  form: KeyValue[];
  filePath: string | null;
}

export interface Auth {
  kind: string;
  token: string;
  username: string;
  password: string;
  apiKeyName: string;
  apiKeyIn: string;
  apiKeyValue: string;
}

export type Assertion =
  | { type: "statusCode"; op: string; expected: number }
  | { type: "header"; key: string; op: string; expected: string }
  | { type: "time"; op: string; expectedMs: number }
  | { type: "jsonPath"; path: string; op: string; expected: string };

export interface InterfaceFile {
  version: number;
  id: string;
  name: string;
  method: string;
  url: string;
  headers: KeyValue[];
  query: KeyValue[];
  body: Body;
  auth: Auth;
  variables: KeyValue[];
  assertions: Assertion[];
  description: string;
  timeoutMs?: number | null;
  redirectLimit?: number | null;
  tlsVerify?: boolean | null;
  caCertPath?: string | null;
}

export interface AssertionResult {
  passed: boolean;
  message: string;
}

export interface RunItem {
  groupPath: string[];
  key: string;
  name: string;
  method: string;
  url: string;
  status: number | null;
  timeMs: number | null;
  ok: boolean;
  error: string | null;
  assertionResults: AssertionResult[];
}

export interface RunReport {
  total: number;
  passed: number;
  failed: number;
  items: RunItem[];
}

export interface ImportReport {
  total: number;
  skipped: number;
  warnings: string[];
}

export interface EnvironmentFile {
  version: number;
  id: string;
  file: string;
  name: string;
  host: string;
  builtin: boolean;
  variables: KeyValue[];
}

export interface EnvironmentSummary {
  id: string;
  file: string;
  name: string;
  host: string;
  builtin: boolean;
  active: boolean;
}

export interface GlobalParams {
  headers: KeyValue[];
  cookies: KeyValue[];
  query: KeyValue[];
}

export interface ProjectSettings {
  name: string;
  activeEnvironmentId: string | null;
  globalVariables: KeyValue[];
  globalParams: GlobalParams;
}

export interface SendResponse {
  status: number;
  statusText: string;
  headers: KeyValue[];
  body: string;
  timeMs: number;
  sizeBytes: number;
  truncated: boolean;
  resolvedUrl: string;
}

export interface SendErrorInfo {
  kind: string;
  message: string;
}

export type SendOutcome = { ok: true; res: SendResponse } | { ok: false; err: SendErrorInfo };

export interface InterfaceDoc {
  groupPath: string[];
  key: string;
  doc: InterfaceFile;
}

export type TreeNode =
  | { type: "group"; key: string; name: string; children: TreeNode[] }
  | { type: "interface"; key: string; name: string; method: string };

export interface CreatedInterface {
  key: string;
  file: InterfaceFile;
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
  createTeam: (name: string, description?: string) =>
    invoke<TeamInfo>("create_team", { name, description: description ?? null }),
  createProject: (teamKey: string, name: string, description?: string) =>
    invoke<ProjectInfo>("create_project", { teamKey, name, description: description ?? null }),
  deleteTeam: (teamKey: string) => invoke<void>("delete_team", { teamKey }),
  renameTeam: (teamKey: string, newKey: string, newName: string) =>
    invoke<void>("rename_team", { teamKey, newKey, newName }),
  deleteProject: (teamKey: string, projectKey: string) =>
    invoke<void>("delete_project", { teamKey, projectKey }),
  renameProject: (teamKey: string, projectKey: string, newKey: string, newName: string) =>
    invoke<void>("rename_project", { teamKey, projectKey, newKey, newName }),
  moveInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string, targetGroupPath: string[]) =>
    invoke<string>("move_interface", { teamKey, projectKey, groupPath, ifaceKey, targetGroupPath }),
  moveGroup: (teamKey: string, projectKey: string, groupPath: string[], targetGroupPath: string[]) =>
    invoke<void>("move_group", { teamKey, projectKey, groupPath, targetGroupPath }),
  saveWorkspace: (workspace: WorkspaceState) =>
    invoke<void>("save_workspace", { workspace }),

  listInterfaceTree: (teamKey: string, projectKey: string) =>
    invoke<TreeNode[]>("list_interface_tree", { teamKey, projectKey }),
  createGroup: (teamKey: string, projectKey: string, groupPath: string[], name: string, description?: string) =>
    invoke<void>("create_group", { teamKey, projectKey, groupPath, name, description: description ?? null }),
  renameGroup: (teamKey: string, projectKey: string, groupPath: string[], newKey: string, newName: string) =>
    invoke<void>("rename_group", { teamKey, projectKey, groupPath, newKey, newName }),
  deleteGroup: (teamKey: string, projectKey: string, groupPath: string[]) =>
    invoke<void>("delete_group", { teamKey, projectKey, groupPath }),
  createInterface: (teamKey: string, projectKey: string, groupPath: string[], name: string, description?: string) =>
    invoke<CreatedInterface>("create_interface", { teamKey, projectKey, groupPath, name, description: description ?? null }),
  getInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string) =>
    invoke<InterfaceFile>("get_interface", { teamKey, projectKey, groupPath, ifaceKey }),
  saveInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string, iface: InterfaceFile) =>
    invoke<void>("save_interface", { teamKey, projectKey, groupPath, ifaceKey, iface }),
  renameInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string, newName: string) =>
    invoke<void>("rename_interface", { teamKey, projectKey, groupPath, ifaceKey, newName }),
  deleteInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string) =>
    invoke<void>("delete_interface", { teamKey, projectKey, groupPath, ifaceKey }),

  listEnvironments: (teamKey: string, projectKey: string) =>
    invoke<EnvironmentSummary[]>("list_environments", { teamKey, projectKey }),
  getEnvironment: (teamKey: string, projectKey: string, envId: string) =>
    invoke<EnvironmentFile>("get_environment", { teamKey, projectKey, envId }),
  saveEnvironment: (teamKey: string, projectKey: string, env: EnvironmentFile) =>
    invoke<void>("save_environment", { teamKey, projectKey, env }),
  deleteEnvironment: (teamKey: string, projectKey: string, envId: string) =>
    invoke<void>("delete_environment", { teamKey, projectKey, envId }),
  setActiveEnvironment: (teamKey: string, projectKey: string, envId: string) =>
    invoke<void>("set_active_environment", { teamKey, projectKey, envId }),
  getProjectSettings: (teamKey: string, projectKey: string) =>
    invoke<ProjectSettings>("get_project_settings", { teamKey, projectKey }),
  saveProjectSettings: (teamKey: string, projectKey: string, settings: ProjectSettings) =>
    invoke<void>("save_project_settings", { teamKey, projectKey, settings }),

  sendRequest: async (
    teamKey: string,
    projectKey: string,
    envId: string,
    iface: InterfaceFile,
  ): Promise<SendOutcome> => {
    try {
      const res = await invoke<SendResponse>("send_request", { teamKey, projectKey, envId, iface });
      return { ok: true, res };
    } catch (e) {
      return { ok: false, err: e as SendErrorInfo };
    }
  },

  runInterfaces: (teamKey: string, projectKey: string, groupPath: string[]) =>
    invoke<RunReport>("run_interfaces", { teamKey, projectKey, groupPath }),

  importSpecIntoProject: (path: string, teamKey: string, projectKey: string) =>
    invoke<[ImportReport, string]>("import_spec_into_project", { path, teamKey, projectKey }),
  importSpecNewProject: (path: string, teamKey: string) =>
    invoke<[ImportReport, string]>("import_spec_new_project", { path, teamKey }),
  exportOpenapiFile: (path: string, teamKey: string, projectKey: string, yaml: boolean) =>
    invoke<string[]>("export_openapi_file", { path, teamKey, projectKey, yaml }),
};