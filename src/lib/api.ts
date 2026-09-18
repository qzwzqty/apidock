import { invoke } from "@tauri-apps/api/core";

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
  responses: ResponseDef[];
  timeoutMs?: number | null;
  redirectLimit?: number | null;
  tlsVerify?: boolean | null;
  caCertPath?: string | null;
}

/** 接口响应定义（文档化：状态码 + 说明 + 响应体结构） */
export interface ResponseDef {
  statusCode: string;
  description: string;
  /** 响应体 Media Type（空 = 无响应体） */
  contentType: string;
  /** json 模式的响应体结构树 */
  json: JsonBody;
  /** 非 json 模式的响应体示例文本 */
  content: string;
}

export function newResponseDef(statusCode = "200"): ResponseDef {
  return { statusCode, description: "", contentType: "application/json", json: newJsonBody("object"), content: "" };
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

/** 请求历史总结（列表用） */
export interface HistorySummary {
  id: number;
  teamKey: string;
  projectKey: string;
  projectName: string;
  envId: string;
  envName: string;
  ifaceKey: string;
  ifaceName: string;
  /** 引用主键（对应团队/项目/分组/接口已删除或未知时为 null） */
  teamId: number | null;
  projectId: number | null;
  groupId: number | null;
  ifaceId: number | null;
  method: string;
  url: string;
  status: number | null;
  ok: boolean;
  timeMs: number;
  /** Unix 毫秒时间戳 */
  createdAtMs: number;
}

/** 请求历史完整记录：请求定义 + 环境/全局快照 + 响应/错误，可独立重发 */
export interface HistoryRecord extends HistorySummary {
  doc: InterfaceFile;
  env: EnvironmentFile;
  globalVariables: KeyValue[];
  globalParams: GlobalParams;
  response: SendResponse | null;
  error: SendErrorInfo | null;
}

/** 快捷请求（列表用）：与环境无关的临时接口，url 为用户填写的完整地址（含 host），名称可重名 */
export interface QuickRequestSummary {
  id: number;
  name: string;
  method: string;
  url: string;
  /** 所属快捷请求分组（null = 未分组，位于快捷请求根层） */
  groupId: number | null;
}

/** 快捷请求分组：parentId 为 null 即顶层分组（名称无唯一键，故用 id 定位） */
export interface QuickGroupSummary {
  id: number;
  name: string;
  parentId: number | null;
}

/** 快捷请求整棵树：分组与请求均为平铺列表，由前端组树 */
export interface QuickTree {
  groups: QuickGroupSummary[];
  requests: QuickRequestSummary[];
}

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
  /**
   * 移动接口到目标分组；`beforeKey` 指定插入到同组某接口之前（null = 末尾），
   * 因此同分组调用即实现拖拽排序。
   */
  moveInterface: (
    teamKey: string,
    projectKey: string,
    groupPath: string[],
    ifaceKey: string,
    targetGroupPath: string[],
    beforeKey: string | null,
  ) =>
    invoke<string>("move_interface", { teamKey, projectKey, groupPath, ifaceKey, targetGroupPath, beforeKey }),
  /**
   * 移动分组到目标分组下；`beforeKey` 指定插入到同层某分组之前（null = 同层末尾），
   * 因此同层调用即实现分组拖拽排序。
   */
  moveGroup: (
    teamKey: string,
    projectKey: string,
    groupPath: string[],
    targetGroupPath: string[],
    beforeKey: string | null = null,
  ) =>
    invoke<void>("move_group", { teamKey, projectKey, groupPath, targetGroupPath, beforeKey }),
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

  listQuickTree: (teamKey: string, projectKey: string) =>
    invoke<QuickTree>("list_quick_tree", { teamKey, projectKey }),
  /** 名称留空则由后端自动取名（「新建分组 N」），分组仅作显示、无唯一性要求 */
  createQuickGroup: (teamKey: string, projectKey: string, parentId: number | null, name: string) =>
    invoke<QuickGroupSummary>("create_quick_group", { teamKey, projectKey, parentId, name }),
  renameQuickGroup: (teamKey: string, projectKey: string, id: number, newName: string) =>
    invoke<string>("rename_quick_group", { teamKey, projectKey, id, newName }),
  /** 删除分组：其下子分组与快捷请求一并级联删除 */
  deleteQuickGroup: (teamKey: string, projectKey: string, id: number) =>
    invoke<void>("delete_quick_group", { teamKey, projectKey, id }),
  /**
   * 移动快捷请求分组：`parentId` 为目标父分组（null = 顶层），`beforeId` 指定插入到
   * 同层某分组之前（null = 同层末尾），同层调用即实现分组拖拽排序。
   */
  moveQuickGroup: (
    teamKey: string,
    projectKey: string,
    id: number,
    parentId: number | null,
    beforeId: number | null = null,
  ) => invoke<void>("move_quick_group", { teamKey, projectKey, id, parentId, beforeId }),
  /** 名称留空则由后端自动取名（「快捷请求 N」），无唯一性要求；groupId 为 null 即未分组 */
  createQuickRequest: (teamKey: string, projectKey: string, groupId: number | null, name: string) =>
    invoke<QuickRequestSummary>("create_quick_request", { teamKey, projectKey, groupId, name }),
  getQuickRequest: (teamKey: string, projectKey: string, id: number) =>
    invoke<InterfaceFile>("get_quick_request", { teamKey, projectKey, id }),
  saveQuickRequest: (teamKey: string, projectKey: string, id: number, iface: InterfaceFile) =>
    invoke<void>("save_quick_request", { teamKey, projectKey, id, iface }),
  /** 复制快捷请求：内容同源、文档 id 独立，落在同一分组（名称后接 -copy） */
  copyQuickRequest: (teamKey: string, projectKey: string, id: number) =>
    invoke<QuickRequestSummary>("copy_quick_request", { teamKey, projectKey, id }),
  /**
   * 移动快捷请求到目标分组（groupId 为 null = 回到未分组根层）；
   * `beforeId` 指定插入到同组某快捷请求之前（null = 末尾），同分组调用即实现拖拽排序。
   */
  moveQuickRequest: (
    teamKey: string,
    projectKey: string,
    id: number,
    groupId: number | null,
    beforeId: number | null,
  ) => invoke<void>("move_quick_request", { teamKey, projectKey, id, groupId, beforeId }),
  renameQuickRequest: (teamKey: string, projectKey: string, id: number, newName: string) =>
    invoke<string>("rename_quick_request", { teamKey, projectKey, id, newName }),
  deleteQuickRequest: (teamKey: string, projectKey: string, id: number) =>
    invoke<void>("delete_quick_request", { teamKey, projectKey, id }),
  /** 发送快捷请求：host 由地址栏的完整地址提供，不继承环境变量 */
  sendQuickRequest: async (
    teamKey: string,
    projectKey: string,
    id: number,
    iface: InterfaceFile,
  ): Promise<SendOutcome> => {
    try {
      const res = await invoke<SendResponse>("send_quick_request", { teamKey, projectKey, id, iface });
      return { ok: true, res };
    } catch (e) {
      return { ok: false, err: e as SendErrorInfo };
    }
  },

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
    ifaceKey?: string,
    ifaceName?: string,
    groupPath?: string[],
  ): Promise<SendOutcome> => {
    try {
      const res = await invoke<SendResponse>("send_request", {
        teamKey,
        projectKey,
        envId,
        iface,
        ifaceKey: ifaceKey ?? null,
        ifaceName: ifaceName ?? null,
        groupPath: groupPath ?? [],
      });
      return { ok: true, res };
    } catch (e) {
      return { ok: false, err: e as SendErrorInfo };
    }
  },

  listRequestHistory: () => invoke<HistorySummary[]>("list_request_history"),
  getRequestHistory: (id: number) => invoke<HistoryRecord>("get_request_history", { id }),
  deleteRequestHistory: (id: number) => invoke<void>("delete_request_history", { id }),
  clearRequestHistory: () => invoke<void>("clear_request_history"),
  /** 按历史快照重发请求（可传入编辑后的接口定义；记为新历史），返回新记录 */
  resendHistory: (id: number, iface?: InterfaceFile) =>
    invoke<HistoryRecord>("resend_history", { id, iface: iface ?? null }),

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

/** 标签页配置数量（标题徽标用，与 InterfaceEditor 的 Tab 一一对应） */
export type ConfigTab = "params" | "headers" | "body" | "auth" | "vars" | "assert" | "desc" | "resp";

/**
 * 统计各区块「已配置」的数量：
 * - params / headers / vars：非空 key 的行数
 * - body：json / raw / file 视为整体 1 个（无内容为 0）；urlencoded / form-data 按字段行数；none 为 0
 * - auth：非 none 视为 1
 * - assert：断言条数；desc：说明非空视为 1
 */
export function configCounts(doc: InterfaceFile): Record<ConfigTab, number> {
  const params = doc.query.filter((p) => p.key.trim()).length;
  const headers = doc.headers.filter((p) => p.key.trim()).length;
  let body = 0;
  switch (doc.body.mode) {
    case "json":
      body = isJsonBodyEmpty(doc.body.json) && !doc.body.content.trim() ? 0 : 1;
      break;
    case "raw":
      body = doc.body.content.trim() ? 1 : 0;
      break;
    case "urlencoded":
    case "form-data":
      body = doc.body.form.filter((p) => p.key.trim()).length;
      break;
    case "file":
      body = doc.body.filePath?.trim() ? 1 : 0;
      break;
    default:
      body = 0;
  }
  const auth = doc.auth.kind !== "none" ? 1 : 0;
  const vars = doc.variables.filter((v) => v.key.trim()).length;
  const assert = doc.assertions.length;
  const desc = doc.description.trim() ? 1 : 0;
  const resp = doc.responses.filter((r) => r.statusCode.trim()).length;
  return { params, headers, body, auth, vars, assert, desc, resp };
}