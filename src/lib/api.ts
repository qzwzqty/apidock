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

/** 文档化参数（查询参数/请求头/表单字段）：示例值作为发送时的实际值 */
export interface ApiParam {
  key: string;
  /** 示例值（磁盘字段名 value，兼容旧数据） */
  example: string;
  required: boolean;
  /** 参数类型：string | integer | number | boolean | object | array | file */
  type: string;
  description: string;
  enabled: boolean;
}

/** JSON 请求体结构树字段 */
export interface BodyField {
  key: string;
  /** 中文名（字段标题，用于文档展示） */
  name: string;
  required: boolean;
  /** object | array | string | integer | number | boolean | null */
  type: string;
  example: string;
  description: string;
  children: BodyField[];
  items: BodyField | null;
}

/** JSON 请求体结构树：根节点与子节点同构（同一字段类型） */
export interface JsonBody {
  /** 根节点字段（key 固定为空，作为载荷本身） */
  root: BodyField;
}

export interface Body {
  mode: string;
  content: string;
  contentType: string;
  /** json 模式的结构化字段树 */
  json: JsonBody;
  form: ApiParam[];
  filePath: string | null;
}

export const EMPTY_JSON_BODY: JsonBody = newJsonBody("object");
export const EMPTY_BODY: Body = {
  mode: "none",
  content: "",
  contentType: "",
  json: EMPTY_JSON_BODY,
  form: [],
  filePath: null,
};
export function newApiParam(): ApiParam {
  return { key: "", example: "", required: false, type: "string", description: "", enabled: true };
}
export function newBodyField(key = ""): BodyField {
  return { key, name: "", required: false, type: "string", example: "", description: "", children: [], items: null };
}
export function newJsonBody(rootType: "object" | "array" = "object"): JsonBody {
  return { root: { ...newBodyField(""), type: rootType } };
}

/** 由结构树生成示例 JSON（含 {{var}} 原样保留），用于预览 */
export function jsonBodyToValue(json: JsonBody): unknown {
  return bodyFieldToValue(json.root);
}

/** 结构树是否"无内容"（与后端 JsonBody::is_empty 对齐） */
export function isJsonBodyEmpty(json: JsonBody): boolean {
  const r = json.root;
  return (
    r.type === "" ||
    (r.type === "object" && !r.name && !r.example && !r.description && !r.required && r.children.length === 0 && !r.items)
  );
}

function bodyFieldToValue(f: BodyField): unknown {
  switch (f.type) {
    case "object": {
      const obj: Record<string, unknown> = {};
      for (const c of f.children) {
        if (c.key.trim()) obj[c.key] = bodyFieldToValue(c);
      }
      return obj;
    }
    case "array":
      return f.items ? [bodyFieldToValue(f.items)] : [];
    case "integer": {
      const n = Number.parseInt(f.example.trim(), 10);
      return Number.isNaN(n) ? 0 : n;
    }
    case "number": {
      const n = Number(f.example.trim());
      return Number.isNaN(n) ? 0 : n;
    }
    case "boolean": {
      const e = f.example.trim().toLowerCase();
      if (["true", "1", "yes", "y", "on"].includes(e)) return true;
      if (["false", "0", "no", "n", "off"].includes(e)) return false;
      return false;
    }
    case "null":
      return null;
    default:
      return f.example;
  }
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
  headers: ApiParam[];
  query: ApiParam[];
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
  renameTeam: (teamKey: string, newName: string) =>
    invoke<void>("rename_team", { teamKey, newName }),
  deleteProject: (teamKey: string, projectKey: string) =>
    invoke<void>("delete_project", { teamKey, projectKey }),
  renameProject: (teamKey: string, projectKey: string, newName: string) =>
    invoke<void>("rename_project", { teamKey, projectKey, newName }),
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
  renameGroup: (teamKey: string, projectKey: string, groupPath: string[], newName: string) =>
    invoke<void>("rename_group", { teamKey, projectKey, groupPath, newName }),
  deleteGroup: (teamKey: string, projectKey: string, groupPath: string[]) =>
    invoke<void>("delete_group", { teamKey, projectKey, groupPath }),
  createInterface: (teamKey: string, projectKey: string, groupPath: string[], name: string, description?: string) =>
    invoke<CreatedInterface>("create_interface", { teamKey, projectKey, groupPath, name, description: description ?? null }),
  copyInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string) =>
    invoke<CreatedInterface>("copy_interface", { teamKey, projectKey, groupPath, ifaceKey }),
  getInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string) =>
    invoke<InterfaceFile>("get_interface", { teamKey, projectKey, groupPath, ifaceKey }),
  saveInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string, iface: InterfaceFile) =>
    invoke<void>("save_interface", { teamKey, projectKey, groupPath, ifaceKey, iface }),
  renameInterface: (teamKey: string, projectKey: string, groupPath: string[], ifaceKey: string, newName: string) =>
    invoke<string>("rename_interface", { teamKey, projectKey, groupPath, ifaceKey, newName }),
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
  exportInterfaceOpenapiFile: (
    path: string,
    teamKey: string,
    projectKey: string,
    groupPath: string[],
    ifaceKey: string,
    yaml: boolean,
  ) =>
    invoke<string[]>("export_interface_openapi_file", { path, teamKey, projectKey, groupPath, ifaceKey, yaml }),
};