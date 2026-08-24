import { useState, type ReactNode } from "react";
import { marked } from "marked";
import { Save, Send, Check, SlidersHorizontal, RotateCcw, Plus, Trash2, Eye, PencilLine, ChevronRight, ChevronDown, Wand2 } from "lucide-react";
import type { ApiParam, Assertion, BodyField, InterfaceFile, JsonBody, KeyValue } from "@/lib/api";
import { isJsonBodyEmpty, newApiParam, newBodyField, jsonBodyToValue } from "@/lib/api";
import { METHODS, methodColor } from "@/lib/methods";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogFooter } from "@/components/ui/dialog";

/** 视图模式：文档（只读）/ 编辑（可改）/ 调试（发请求） */
export type EditorMode = "doc" | "edit" | "debug";

/** 参数类型（Params / Headers / 表单字段） */
const PARAM_TYPES = ["string", "integer", "number", "boolean", "object", "array", "file"];
/** JSON 字段类型 */
const FIELD_TYPES = ["string", "integer", "number", "boolean", "object", "array", "null"];

/** Body 模式（Apifox 风格分段选择） */
const BODY_MODES: [string, string][] = [
  ["none", "none"],
  ["form-data", "form-data"],
  ["urlencoded", "x-www-form-urlencoded"],
  ["json", "JSON"],
  ["raw", "文本"],
  ["file", "文件"],
];

type Tab = "params" | "headers" | "body" | "auth" | "vars" | "assert" | "desc";

/** 调试模式 JSON 初始文本：由文档结构树生成示例（树为空时回落旧 content） */
function initialDebugJson(body: InterfaceFile["body"]): string {
  if (body.mode === "json" && !isJsonBodyEmpty(body.json)) {
    return JSON.stringify(jsonBodyToValue(body.json), null, 2);
  }
  return body.content.trim() || "{}";
}

/** JSON 文本合法性（{{变量}} 占位符视为字符串，不视为语法错误） */
function isDebuggableJson(text: string): boolean {
  if (!text.trim()) return true;
  try {
    JSON.parse(text.replace(/\{\{[\s\S]*?\}\}/g, "\"{{var}}\""));
    return true;
  } catch {
    return false;
  }
}

export function InterfaceEditor({
  doc,
  onSave,
  onSend,
  onModeChange,
}: {
  doc: InterfaceFile;
  onSave: (doc: InterfaceFile) => Promise<void>;
  onSend: (doc: InterfaceFile) => void;
  onModeChange?: (mode: EditorMode) => void;
}) {
  const [base, setBase] = useState<InterfaceFile>({ ...doc });
  const [mode, setMode] = useState<EditorMode>("doc");
  const [tab, setTab] = useState<Tab>("params");
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [showOpts, setShowOpts] = useState(false);
  const [preview, setPreview] = useState(false);
  /** 调试模式下 Body-JSON 的临时文本（null=未编辑，切换接口时重置） */
  const [debugJson, setDebugJson] = useState<string | null>(null);

  const switchMode = (m: EditorMode) => {
    setMode(m);
    onModeChange?.(m);
  };

  // 外部 doc 变化（切换接口/外部刷新）时同步
  const [lastDocId, setLastDocId] = useState(doc.id);
  if (doc.id !== lastDocId) {
    setLastDocId(doc.id);
    setBase({ ...doc });
    setDirty(false);
    setSaved(false);
    setDebugJson(null);
  }

  const update = (patch: Partial<InterfaceFile>) => {
    setBase((d) => ({ ...d, ...patch }));
    setDirty(true);
    setSaved(false);
  };
  const updateList = (field: "query" | "headers", list: ApiParam[]) =>
    update({ [field]: list } as Partial<InterfaceFile>);
  const updateKv = (field: "variables", list: KeyValue[]) =>
    update({ [field]: list } as Partial<InterfaceFile>);
  const updateAssertions = (list: Assertion[]) => update({ assertions: list });

  const save = async () => {
    setSaving(true);
    try {
      await onSave(base);
      setDirty(false);
      setSaved(true);
    } finally {
      setSaving(false);
    }
  };

  /** 发送：调试模式下 JSON 请求体按用户输入的原始文本发送（清空结构树让后端回落 content） */
  const handleSend = () => {
    if (mode === "debug" && base.body.mode === "json" && debugJson != null) {
      onSend({
        ...base,
        body: {
          ...base.body,
          json: { root: { ...newBodyField(""), type: "" } },
          content: debugJson,
        },
      });
    } else {
      onSend(base);
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* 模式标签：文档 / 编辑 / 调试（Apifox 风格） */}
      <div className="flex h-9 shrink-0 items-center border-b border-border px-2 text-sm">
        {(
          [
            ["doc", "文档"],
            ["edit", "编辑"],
            ["debug", "调试"],
          ] as [EditorMode, string][]
        ).map(([m, label]) => (
          <button
            key={m}
            className={`h-full cursor-pointer px-3 transition-colors ${
              mode === m
                ? "border-b-2 border-accent text-accent"
                : "border-b-2 border-transparent text-muted-foreground hover:text-foreground"
            }`}
            onClick={() => switchMode(m)}
          >
            {label}
          </button>
        ))}
        {saved && !dirty && mode !== "doc" && (
          <span className="ml-auto flex items-center gap-1 pr-2 text-xs text-green-500">
            <Check className="h-3 w-3" /> 已保存
          </span>
        )}
      </div>

      {/* 请求行 */}
      {mode === "doc" ? (
        <div className="flex items-center gap-2 px-4 py-3">
          <span
            className={`flex h-6 w-16 shrink-0 items-center justify-center rounded-md text-xs font-bold text-white ${methodColor(base.method)}`}
          >
            {base.method}
          </span>
          <span className="min-w-0 flex-1 truncate font-mono text-sm text-foreground">
            {base.url || "（未设置请求地址）"}
          </span>
          <Button variant="outline" onClick={() => switchMode("edit")}>
            <PencilLine className="h-4 w-4" /> 编辑
          </Button>
          <Button onClick={() => switchMode("debug")}>调试</Button>
        </div>
      ) : (
        <div className="flex items-center gap-2 px-4 py-3">
          <select
            className="h-9 min-w-20 cursor-pointer rounded-md border border-border bg-muted px-2 text-sm font-semibold text-accent outline-none focus:border-ring"
            value={base.method}
            onChange={(e) => update({ method: e.target.value })}
          >
            {METHODS.map((m) => (
              <option key={m} value={m}>{m}</option>
            ))}
          </select>
          <Input
            className="h-9 flex-1"
            placeholder="请求地址，如 {{host}}/api/login"
            value={base.url}
            onChange={(e) => update({ url: e.target.value })}
          />
          {mode === "edit" && (
            <Button onClick={save} disabled={saving || (!dirty && !saved)} variant="outline">
              <Save className="h-4 w-4" /> 保存
            </Button>
          )}
          <Button variant="ghost" title="发送选项（超时/重定向/TLS/CA）"
            onClick={() => setShowOpts(true)}>
            <SlidersHorizontal className="h-4 w-4" />
          </Button>
          {mode !== "debug" && (
            <Button variant="outline" title="切换到调试标签页发送请求" onClick={() => switchMode("debug")}>
              调试
            </Button>
          )}
          {mode === "debug" && (
            <Button onClick={handleSend} className="bg-green-600 hover:bg-green-500">
              <Send className="h-4 w-4" /> 发送
            </Button>
          )}
        </div>
      )}

      <SendOptionsDialog
        iface={base}
        open={showOpts}
        onClose={() => setShowOpts(false)}
        onApply={(patch) => {
          update(patch);
          setShowOpts(false);
        }}
      />

      {/* 区块标签（编辑/调试模式） */}
      <div className="flex h-9 shrink-0 items-center border-b border-border px-2 text-sm">
        {(
          [
            ["params", "参数"],
            ["headers", "请求头"],
            ["body", "Body"],
            ["auth", "鉴权"],
            ["vars", "变量"],
            ["assert", "断言"],
            ["desc", "说明"],
          ] as [Tab, string][]
        ).map(([k, label]) => (
          <button
            key={k}
            className={`h-full cursor-pointer border-r border-border px-3 transition-colors ${
              tab === k ? "text-accent" : "text-muted-foreground hover:text-foreground"
            }`}
            onClick={() => setTab(k)}
          >
            {label}
          </button>
        ))}
      </div>

      {mode === "doc" ? (
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          <DocView doc={base} />
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {tab === "params" && (
          <ParamList
            rows={base.query}
            onChange={(list) => updateList("query", list)}
            placeholderK="参数名"
            placeholderV="示例值"
            showEnabled={mode === "debug"}
          />
        )}
        {tab === "headers" && (
          <ParamList rows={base.headers} onChange={(list) => updateList("headers", list)} showEnabled={mode === "debug"} />
        )}
        {tab === "body" && (
          <BodyEditor
            key={base.id}
            body={base.body}
            onChange={(body) => update({ body })}
            debugMode={mode === "debug"}
            debugJson={debugJson}
            onDebugJsonChange={setDebugJson}
          />
        )}
        {tab === "auth" && <AuthEditor auth={base.auth} onChange={(auth) => update({ auth })} />}
        {tab === "vars" && (
          <KvList rows={base.variables} onChange={(list) => updateKv("variables", list)} />
        )}
        {tab === "assert" && <AssertionEditor rows={base.assertions} onChange={updateAssertions} />}
        {tab === "desc" && (
          <div>
            <div className="mb-1 flex items-center justify-between">
              <label className="text-xs text-muted-foreground">接口说明（支持 Markdown）</label>
              <div className="flex gap-1">
                <Button size="sm" variant="outline" onClick={() => setPreview(!preview)}>
                  {preview ? <PencilLine className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
                  {preview ? "编辑" : "预览"}
                </Button>
              </div>
            </div>
            {preview ? (
              <div
                className="markdown-body prose prose-sm max-w-none rounded-md border border-border bg-muted p-3 text-sm text-foreground"
                dangerouslySetInnerHTML={{ __html: marked.parse(base.description || "*暂无说明*") as string }}
              />
            ) : (
              <textarea
                className="h-40 w-full resize-y rounded-md border border-border bg-muted p-2.5 text-sm text-foreground placeholder:text-muted-foreground outline-none focus:border-ring"
                value={base.description}
                onChange={(e) => update({ description: e.target.value })}
                placeholder="# 接口说明"
              />
            )}
          </div>
        )}
        </div>
      )}
    </div>
  );
}

function KvList({
  rows,
  onChange,
  placeholderK = "Key",
  placeholderV = "Value",
}: {
  rows: KeyValue[];
  onChange: (rows: KeyValue[]) => void;
  placeholderK?: string;
  placeholderV?: string;
}) {
  const setRow = (i: number, patch: Partial<KeyValue>) =>
    onChange(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  return (
    <div className="space-y-1">
      {rows.map((row, i) => (
        <div key={i} className="flex items-center gap-1.5">
          <input
            type="checkbox"
            className="h-3.5 w-3.5 cursor-pointer accent-(--ring)"
            checked={row.enabled}
            onChange={(e) => setRow(i, { enabled: e.target.checked })}
          />
          <Input
            className="h-7 flex-1"
            placeholder={placeholderK}
            value={row.key}
            onChange={(e) => setRow(i, { key: e.target.value })}
          />
          <Input
            className="h-7 flex-1"
            placeholder={placeholderV}
            value={row.value}
            onChange={(e) => setRow(i, { value: e.target.value })}
          />
          <Button
            size="icon"
            variant="ghost"
            title="删除"
            onClick={() => onChange(rows.filter((_, j) => j !== i))}
          >
            ×
          </Button>
        </div>
      ))}
      <Button size="sm" variant="ghost" onClick={() => onChange([...rows, { key: "", value: "", enabled: true }])}>
        + 添加
      </Button>
    </div>
  );
}

/** 文档化参数表格：参数名 | 类型 | 必填 | 示例值 | 说明（Apifox 风格）；调试模式隐藏必填列、显示"参与发送"勾选 */
function ParamList({
  rows,
  onChange,
  placeholderK = "参数名",
  placeholderV = "示例值",
  showEnabled = false,
}: {
  rows: ApiParam[];
  onChange: (rows: ApiParam[]) => void;
  placeholderK?: string;
  placeholderV?: string;
  /** 调试模式：显示"是否参与发送"勾选（启停权归调试模式，不持久化进文档），并隐藏必填列 */
  showEnabled?: boolean;
}) {
  const setRow = (i: number, patch: Partial<ApiParam>) =>
    onChange(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  return (
    <div className="space-y-1">
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
        {showEnabled && <span className="w-5 shrink-0" title="是否参与发送" />}
        <span className="w-44">参数名</span>
        <span className="w-24">类型</span>
        {!showEnabled && <span className="w-14 shrink-0">必填</span>}
        <span className="flex-1">{placeholderV}</span>
        <span className="flex-1">说明</span>
        <span className="w-8 shrink-0" />
      </div>
      {rows.length === 0 && (
        <p className="py-1 text-xs text-muted-foreground">
          暂无参数。定义接口的请求参数（名称、类型、是否必填、示例值与说明），保存后即成为接口文档。
        </p>
      )}
      {rows.map((row, i) => (
        <div key={i} className="flex items-center gap-1.5">
          {showEnabled && (
            <span className="flex w-5 shrink-0 items-center justify-center">
              <input
                type="checkbox"
                className="h-3.5 w-3.5 cursor-pointer accent-(--ring)"
                title="是否参与发送"
                checked={row.enabled}
                onChange={(e) => setRow(i, { enabled: e.target.checked })}
              />
            </span>
          )}
          <Input
            className="h-7 w-44"
            placeholder={placeholderK}
            value={row.key}
            onChange={(e) => setRow(i, { key: e.target.value })}
          />
          <select
            className="h-7 w-24 rounded-md border border-border bg-muted px-1.5 text-xs outline-none focus:border-ring cursor-pointer"
            value={row.type || "string"}
            onChange={(e) => setRow(i, { type: e.target.value })}
          >
            {PARAM_TYPES.map((t) => (
              <option key={t} value={t}>{t}</option>
            ))}
          </select>
          {!showEnabled && (
            <label className="flex w-14 shrink-0 items-center gap-1 text-xs text-muted-foreground" title="是否必填">
              <input
                type="checkbox"
                className="h-3.5 w-3.5 cursor-pointer accent-(--ring)"
                checked={row.required}
                onChange={(e) => setRow(i, { required: e.target.checked })}
              />
            </label>
          )}
          <Input
            className="h-7 flex-1"
            placeholder={placeholderV}
            value={row.example}
            onChange={(e) => setRow(i, { example: e.target.value })}
          />
          <Input
            className="h-7 flex-1"
            placeholder="说明"
            value={row.description}
            onChange={(e) => setRow(i, { description: e.target.value })}
          />
          <Button
            size="icon"
            variant="ghost"
            title="删除"
            onClick={() => onChange(rows.filter((_, j) => j !== i))}
          >
            ×
          </Button>
        </div>
      ))}
      <Button size="sm" variant="ghost" onClick={() => onChange([...rows, newApiParam()])}>
        <Plus className="h-3.5 w-3.5" /> 添加参数
      </Button>
    </div>
  );
}

function BodyEditor({
  body,
  onChange,
  debugMode = false,
  debugJson = null,
  onDebugJsonChange = () => {},
}: {
  body: InterfaceFile["body"];
  onChange: (body: InterfaceFile["body"]) => void;
  /** 调试模式：JSON 用富文本框（临时文本，直接发送），表单显示参与发送勾选 */
  debugMode?: boolean;
  /** 调试模式下用户手写的 JSON 文本（null = 未编辑，展示文档生成的初始值） */
  debugJson?: string | null;
  onDebugJsonChange?: (text: string) => void;
}) {
  const [preview, setPreview] = useState(false);
  /** 调试 JSON 文本：未编辑时由文档结构树（或旧 content）生成 */
  const debugText = debugJson ?? (body.mode === "json" ? initialDebugJson(body) : "");
  const debugJsonValid = isDebuggableJson(debugText);
  const autoGenerate = () => onDebugJsonChange(initialDebugJson(body));
  return (
    <div className="space-y-3">
      {/* Body 格式（分段按钮，Apifox 风格） */}
      <div className="flex flex-wrap items-center gap-1">
        {BODY_MODES.map(([mode, label]) => (
          <button
            key={mode}
            className={`h-7 rounded-md border px-2.5 text-xs transition-colors cursor-pointer ${
              body.mode === mode
                ? "border-ring bg-accent/10 text-accent"
                : "border-border bg-muted text-muted-foreground hover:text-foreground"
            }`}
            onClick={() => onChange({ ...body, mode })}
          >
            {label}
          </button>
        ))}
        {body.mode === "json" && (
          <>
            <span className="ml-2 text-xs text-muted-foreground">application/json</span>
            {!debugMode && (
              <Button
                size="sm"
                variant="ghost"
                className="ml-auto"
                onClick={() => setPreview(!preview)}
              >
                {preview ? <PencilLine className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
                {preview ? "编辑" : "预览 JSON"}
              </Button>
            )}
          </>
        )}
      </div>

      {body.mode === "json" && debugMode && (
        <div>
          <div className="mb-1 flex items-center justify-between">
            <span className="text-xs text-muted-foreground">
              请求体 JSON（直接发送该文本，支持 {"{{变量}}"}；修改仅在调试中生效，不影响文档）
            </span>
            <Button size="sm" variant="outline" title="根据文档中定义的参数结构生成 JSON" onClick={autoGenerate}>
              <Wand2 className="h-3.5 w-3.5" /> 自动生成
            </Button>
          </div>
          <textarea
            className="h-72 w-full resize-y rounded-md border border-border bg-muted p-2.5 font-mono text-xs text-foreground outline-none focus:border-ring"
            value={debugText}
            onChange={(e) => onDebugJsonChange(e.target.value)}
            spellCheck={false}
            placeholder='输入 JSON，如 {"name": "{{userName}}"}'
          />
          {!debugJsonValid && (
            <p className="mt-1 text-xs text-amber-500">JSON 语法无效：发送时将按原文（不校验）发出。</p>
          )}
        </div>
      )}

      {body.mode === "json" && !debugMode && !preview && (
        <JsonBodyEditor json={body.json} onChange={(json) => onChange({ ...body, json })} />
      )}
      {body.mode === "json" && !debugMode && preview && (
        <pre className="max-h-96 overflow-auto rounded-md border border-border bg-muted p-3 font-mono text-xs text-foreground">
          {JSON.stringify(jsonBodyToValue(body.json), null, 2)}
        </pre>
      )}

      {body.mode === "raw" && (
        <div>
          <div className="mb-2 flex w-72 items-center gap-2">
            <label className="text-xs text-muted-foreground">Content-Type</label>
            <Input
              className="h-7"
              value={body.contentType}
              placeholder="text/plain"
              onChange={(e) => onChange({ ...body, contentType: e.target.value })}
            />
          </div>
          <textarea
            className="h-52 w-full resize-y rounded-md border border-border bg-muted p-2.5 font-mono text-xs text-foreground outline-none focus:border-ring"
            value={body.content}
            onChange={(e) => onChange({ ...body, content: e.target.value })}
            spellCheck={false}
            placeholder="原始文本内容"
          />
        </div>
      )}

      {(body.mode === "urlencoded" || body.mode === "form-data") && (
        <ParamList
          rows={body.form}
          onChange={(form) => onChange({ ...body, form })}
          placeholderK="字段名"
          placeholderV={body.mode === "form-data" ? "值 或 @文件路径" : "示例值"}
          showEnabled={debugMode}
        />
      )}

      {body.mode === "file" && (
        <div className="flex w-full max-w-96 items-center gap-2">
          <label className="shrink-0 text-xs text-muted-foreground">文件路径</label>
          <Input
            className="h-8 flex-1 font-mono text-xs"
            value={body.filePath ?? ""}
            placeholder="C:\path\to\file.bin"
            onChange={(e) => onChange({ ...body, filePath: e.target.value })}
          />
        </div>
      )}

      {body.mode === "none" && (
        <p className="text-xs text-muted-foreground">无请求体。</p>
      )}
    </div>
  );
}

/** JSON 请求体树形编辑器：根节点类型 + 嵌套字段（Apifox 风格） */
function JsonBodyEditor({
  json,
  onChange,
}: {
  json: JsonBody;
  onChange: (json: JsonBody) => void;
}) {
  return (
    <div className="space-y-1">
      <JsonFieldRow
        f={json.root}
        depth={0}
        isRoot
        onChange={(root) => onChange({ ...json, root })}
        onDelete={() => {}}
        onAddChild={
          json.root.type === "object"
            ? () => onChange({ ...json, root: { ...json.root, children: [...json.root.children, newBodyField()] } })
            : null
        }
      />
      <p className="pt-1 text-xs text-muted-foreground">
        根节点与字段同构：类型可选 string/integer/number/boolean/object/array/null。发送请求时按此结构生成 JSON 载荷，示例值作为实际值（支持 {"{{变量}}"}）。
      </p>
    </div>
  );
}

/** 树的一行：字段名 | 类型 | 必填 | 示例值 | 中文名 | 说明 | 操作 */
function JsonFieldRow({
  f,
  depth,
  isItems = false,
  isRoot = false,
  onChange,
  onDelete,
  onAddChild,
}: {
  f: BodyField;
  depth: number;
  isItems?: boolean;
  isRoot?: boolean;
  onChange: (f: BodyField) => void;
  onDelete: () => void;
  onAddChild: (() => void) | null;
}) {
  const [open, setOpen] = useState(true);
  const isContainer = f.type === "object" || f.type === "array";
  const fixedLabel = isRoot ? "根节点" : isItems ? "ITEMS" : "";
  const patch = (p: Partial<BodyField>) => onChange({ ...f, ...p });
  const setType = (t: string) => {
    const next: BodyField = { ...f, type: t };
    if (t === "object") {
      next.items = null;
    } else if (t === "array") {
      next.children = [];
      if (!next.items) next.items = newBodyField();
    } else {
      next.children = [];
      next.items = null;
    }
    onChange(next);
  };
  return (
    <>
      <div className="flex items-center gap-1.5" style={{ paddingLeft: depth * 24 }}>
        {isContainer ? (
          <button
            className="flex h-5 w-5 shrink-0 cursor-pointer items-center justify-center rounded text-muted-foreground hover:text-foreground"
            onClick={() => setOpen(!open)}
            title={open ? "折叠" : "展开"}
          >
            {open ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
          </button>
        ) : (
          <span className="w-5 shrink-0" />
        )}
        <Input
          className="h-7 w-44"
          placeholder={fixedLabel || "字段名"}
          value={fixedLabel || f.key}
          disabled={isRoot || isItems}
          onChange={(e) => patch({ key: e.target.value })}
        />
        <select
          className="h-7 w-24 rounded-md border border-border bg-muted px-1.5 text-xs outline-none focus:border-ring cursor-pointer"
          value={f.type}
          onChange={(e) => setType(e.target.value)}
        >
          {FIELD_TYPES.map((t) => (
            <option key={t} value={t}>{t}</option>
          ))}
        </select>
        <label className="flex w-6 shrink-0 items-center justify-center" title="是否必填">
          <input
            type="checkbox"
            className="h-3.5 w-3.5 cursor-pointer accent-(--ring)"
            checked={f.required}
            onChange={(e) => patch({ required: e.target.checked })}
          />
        </label>
        <Input
          className={`h-7 flex-1 ${isContainer ? "text-muted-foreground" : ""}`}
          placeholder="示例值"
          value={isContainer ? "" : f.example}
          disabled={isContainer}
          onChange={(e) => patch({ example: e.target.value })}
        />
        <Input
          className="h-7 w-32"
          placeholder="中文名"
          value={f.name}
          onChange={(e) => patch({ name: e.target.value })}
        />
        <Input
          className="h-7 flex-[1.6]"
          placeholder="说明"
          value={f.description}
          onChange={(e) => patch({ description: e.target.value })}
        />
        {onAddChild && f.type === "object" && (
          <Button size="icon" variant="ghost" title="添加子字段" onClick={onAddChild}>
            <Plus className="h-3.5 w-3.5" />
          </Button>
        )}
        {!isRoot && (
          <Button size="icon" variant="ghost" title="删除字段" onClick={onDelete}>
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        )}
      </div>
      {isContainer && open && f.type === "array" && f.items && (
        <JsonFieldRow
          f={f.items}
          depth={depth + 1}
          isItems
          onChange={(nf) => onChange({ ...f, items: nf })}
          onDelete={() => onChange({ ...f, items: null })}
          onAddChild={
            f.items?.type === "object"
              ? () => {
                  const items = f.items ?? newBodyField();
                  onChange({ ...f, items: { ...items, children: [...(items.children ?? []), newBodyField()] } });
                }
              : null
          }
        />
      )}
      {isContainer && open && f.type === "object" && (
        <div className="space-y-1">
          {f.children.map((c, i) => (
            <JsonFieldRow
              key={i}
              f={c}
              depth={depth + 1}
              onChange={(nc) => {
                const children = [...f.children];
                children[i] = nc;
                onChange({ ...f, children });
              }}
              onDelete={() => onChange({ ...f, children: f.children.filter((_, j) => j !== i) })}
              onAddChild={
                c.type === "object"
                  ? () => {
                      const children = [...f.children];
                      children[i] = { ...c, children: [...c.children, newBodyField()] };
                      onChange({ ...f, children });
                    }
                  : null
              }
            />
          ))}
        </div>
      )}
    </>
  );
}

function AuthEditor({
  auth,
  onChange,
}: {
  auth: InterfaceFile["auth"];
  onChange: (auth: InterfaceFile["auth"]) => void;
}) {
  return (
    <div className="max-w-xl space-y-3">
      <div>
        <label className="mb-1 block text-xs text-muted-foreground">鉴权类型</label>
        <select
          className="h-8 rounded-md border border-border bg-muted px-2 text-sm outline-none focus:border-ring cursor-pointer"
          value={auth.kind}
          onChange={(e) => onChange({ ...auth, kind: e.target.value })}
        >
          <option value="none">None（无鉴权）</option>
          <option value="bearer">Bearer Token</option>
          <option value="basic">Basic Auth</option>
          <option value="api-key">API Key</option>
        </select>
      </div>

      {auth.kind === "bearer" && (
        <div>
          <label className="mb-1 block text-xs text-muted-foreground">Token（支持 {"{{var}}"}）</label>
          <Input value={auth.token} onChange={(e) => onChange({ ...auth, token: e.target.value })} />
        </div>
      )}
      {auth.kind === "basic" && (
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">用户名</label>
            <Input value={auth.username} onChange={(e) => onChange({ ...auth, username: e.target.value })} />
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">密码</label>
            <Input type="password" value={auth.password} onChange={(e) => onChange({ ...auth, password: e.target.value })} />
          </div>
        </div>
      )}
      {auth.kind === "api-key" && (
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">Key 名称</label>
              <Input value={auth.apiKeyName} onChange={(e) => onChange({ ...auth, apiKeyName: e.target.value })} />
            </div>
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">Key 值</label>
              <Input value={auth.apiKeyValue} onChange={(e) => onChange({ ...auth, apiKeyValue: e.target.value })} />
            </div>
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">放置位置</label>
            <select
              className="h-8 rounded-md border border-border bg-muted px-2 text-sm outline-none focus:border-ring cursor-pointer"
              value={auth.apiKeyIn}
              onChange={(e) => onChange({ ...auth, apiKeyIn: e.target.value })}
            >
              <option value="header">请求头 (Header)</option>
              <option value="query">查询参数 (Query)</option>
            </select>
          </div>
        </div>
      )}
    </div>
  );
}
function SendOptionsDialog({
  iface,
  open,
  onClose,
  onApply,
}: {
  iface: InterfaceFile;
  open: boolean;
  onClose: () => void;
  onApply: (patch: Partial<InterfaceFile>) => void;
}) {
  const [t, setT] = useState(iface.timeoutMs == null ? "" : String(iface.timeoutMs / 1000));
  const [r, setR] = useState(iface.redirectLimit == null ? "" : String(iface.redirectLimit));
  const [tls, setTls] = useState(iface.tlsVerify == null ? "null" : String(iface.tlsVerify));
  const [ca, setCa] = useState(iface.caCertPath ?? "");

  const apply = () => {
    const timeoutMs = t.trim() === "" ? null : Math.max(1, Math.floor(Number(t) * 1000));
    const redirectLimit = r.trim() === "" ? null : Math.max(0, Math.floor(Number(r)));
    const tlsVerify = tls === "null" ? null : tls === "true";
    const caCertPath = ca.trim() === "" ? null : ca.trim();
    onApply({ timeoutMs, redirectLimit, tlsVerify, caCertPath });
  };

  return (
    <Dialog open={open} onClose={onClose} title="发送选项（保存后随接口生效）">
      <div className="space-y-3">
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">超时（秒，留空=默认 30s）</label>
            <Input value={t} type="number" min={1} onChange={(e) => setT(e.target.value)} />
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">最大重定向（0=不跟随，留空=默认 10）</label>
            <Input value={r} type="number" min={0} onChange={(e) => setR(e.target.value)} />
          </div>
        </div>
        <div>
          <label className="mb-1 block text-xs text-muted-foreground">TLS 证书校验</label>
          <select className="h-8 w-full rounded-md border border-border bg-muted px-2 text-sm outline-none cursor-pointer"
            value={tls} onChange={(e) => setTls(e.target.value)}>
            <option value="null">默认（校验）</option>
            <option value="true">强制校验</option>
            <option value="false">跳过校验（有风险，慎用）</option>
          </select>
        </div>
        <div>
          <label className="mb-1 block text-xs text-muted-foreground">自定义 CA 证书路径（PEM，留空=无）</label>
          <Input value={ca} onChange={(e) => setCa(e.target.value)} placeholder="C:\certs\ca.pem" />
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={() => { setT(""); setR(""); setTls("null"); setCa(""); }}>
            <RotateCcw className="h-3.5 w-3.5" /> 恢复默认
          </Button>
          <Button variant="outline" onClick={onClose}>取消</Button>
          <Button onClick={apply}>确定</Button>
        </DialogFooter>
      </div>
    </Dialog>
  );
}

const OPS = [
  "eq", "ne", "contains", "not-contains", "gt", "ge", "lt", "le", "regex",
];

function AssertionEditor({ rows, onChange }: { rows: Assertion[]; onChange: (rows: Assertion[]) => void }) {
  const set = (i: number, a: Assertion) => onChange(rows.map((r, j) => (j === i ? a : r)));
  return (
    <div className="space-y-2">
      {rows.map((a, i) => (
        <div key={i} className="flex flex-wrap items-center gap-2 rounded-md border border-border p-2">
          <select
            className="h-7 w-28 rounded border border-border bg-muted px-1.5 text-xs outline-none cursor-pointer"
            value={a.type}
            onChange={(e) => {
              const t = e.target.value;
              if (t === "statusCode") set(i, { type: "statusCode", op: "eq", expected: 200 });
              else if (t === "time") set(i, { type: "time", op: "lt", expectedMs: 1000 });
              else if (t === "header") set(i, { type: "header", key: "Content-Type", op: "contains", expected: "" });
              else set(i, { type: "jsonPath", path: "$.", op: "eq", expected: "" });
            }}
          >
            <option value="statusCode">状态码</option>
            <option value="header">响应头</option>
            <option value="time">耗时</option>
            <option value="jsonPath">JSONPath</option>
          </select>

          {a.type === "statusCode" && (
            <>
              <OpSel value={a.op} onChange={(op) => set(i, { ...a, op })} />
              <Input className="h-7 w-20" type="number" value={String(a.expected)}
                onChange={(e) => set(i, { ...a, expected: Number(e.target.value) })} />
            </>
          )}
          {a.type === "time" && (
            <>
              <OpSel value={a.op} onChange={(op) => set(i, { ...a, op })} />
              <Input className="h-7 w-20" type="number" value={String(a.expectedMs)}
                onChange={(e) => set(i, { ...a, expectedMs: Number(e.target.value) })} />
              <span className="text-xs text-muted-foreground">毫秒</span>
            </>
          )}
          {a.type === "header" && (
            <>
              <Input className="h-7 w-32" placeholder="头名" value={a.key}
                onChange={(e) => set(i, { ...a, key: e.target.value })} />
              <OpSel value={a.op} onChange={(op) => set(i, { ...a, op })} />
              <Input className="h-7 w-36" placeholder="期望值" value={a.expected}
                onChange={(e) => set(i, { ...a, expected: e.target.value })} />
            </>
          )}
          {a.type === "jsonPath" && (
            <>
              <Input className="h-7 w-44 font-mono" placeholder="$.data.items[0].id" value={a.path}
                onChange={(e) => set(i, { ...a, path: e.target.value })} />
              <OpSel value={a.op} onChange={(op) => set(i, { ...a, op })} />
              <Input className="h-7 w-36" placeholder="期望值" value={a.expected}
                onChange={(e) => set(i, { ...a, expected: e.target.value })} />
            </>
          )}

          <Button size="icon" variant="ghost" className="ml-auto" title="删除断言"
            onClick={() => onChange(rows.filter((_, j) => j !== i))}>
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      ))}
      <Button size="sm" variant="ghost" onClick={() => onChange([...rows, { type: "statusCode", op: "eq", expected: 200 }])}>
        <Plus className="h-3.5 w-3.5" /> 添加断言
      </Button>
      {rows.length === 0 && (
        <p className="text-xs text-muted-foreground">暂无断言。一键运行时会按断言语义标记通过/失败；没有断言时仅看请求是否成功。</p>
      )}
    </div>
  );
}

function OpSel({ value, onChange }: { value: string; onChange: (op: string) => void }) {
  return (
    <select className="h-7 w-28 rounded border border-border bg-muted px-1.5 text-xs outline-none cursor-pointer"
      value={value} onChange={(e) => onChange(e.target.value)}>
      {OPS.map((o) => <option key={o} value={o}>{o}</option>)}
    </select>
  );
}

/** 文档（只读）视图：Apifox 风格，仅展示接口文档内容 */
function DocView({ doc }: { doc: InterfaceFile }) {
  const hasJson = doc.body.mode === "json" && !isJsonBodyEmpty(doc.body.json);
  return (
    <div className="max-w-3xl space-y-6 pb-8">
      <DocSection title="接口说明">
        {doc.description.trim() ? (
          <div
            className="markdown-body prose prose-sm max-w-none text-sm text-foreground"
            dangerouslySetInnerHTML={{ __html: marked.parse(doc.description) as string }}
          />
        ) : (
          <p className="text-sm text-muted-foreground">暂无说明。</p>
        )}
      </DocSection>

      <DocSection title="查询参数" count={doc.query.length}>
        {doc.query.length > 0 ? (
          <DocTable rows={doc.query} mode="param" />
        ) : (
          <p className="text-sm text-muted-foreground">暂无查询参数。</p>
        )}
      </DocSection>

      <DocSection title="请求头" count={doc.headers.length}>
        {doc.headers.length > 0 ? (
          <DocTable rows={doc.headers} mode="header" />
        ) : (
          <p className="text-sm text-muted-foreground">暂无请求头。</p>
        )}
      </DocSection>

      <DocSection title="请求体">
        {doc.body.mode === "json" && (
          <pre className="overflow-auto rounded-md border border-border bg-muted p-3 font-mono text-xs text-foreground">
            {hasJson ? JSON.stringify(jsonBodyToValue(doc.body.json), null, 2) : "{}"}
          </pre>
        )}
        {doc.body.mode === "raw" && (
          <pre className="overflow-auto whitespace-pre-wrap rounded-md border border-border bg-muted p-3 font-mono text-xs text-foreground">
            {doc.body.content || "（无内容）"}
          </pre>
        )}
        {(doc.body.mode === "urlencoded" || doc.body.mode === "form-data") && (
          <DocTable rows={doc.body.form} mode="param" />
        )}
        {doc.body.mode === "file" && (
          <p className="font-mono text-sm text-foreground">{doc.body.filePath || "（未设置文件路径）"}</p>
        )}
        {doc.body.mode === "none" && <p className="text-sm text-muted-foreground">无请求体。</p>}
      </DocSection>

      <DocSection title="鉴权">
        <p className="text-sm text-foreground">{authSummary(doc.auth)}</p>
      </DocSection>

      {doc.assertions.length > 0 && (
        <DocSection title="断言" count={doc.assertions.length}>
          <ul className="list-disc space-y-0.5 pl-4 text-sm">
            {doc.assertions.map((a, i) => (
              <li key={i} className="text-foreground">{assertionText(a)}</li>
            ))}
          </ul>
        </DocSection>
      )}
    </div>
  );
}

function DocSection({ title, count, children }: { title: string; count?: number; children: ReactNode }) {
  return (
    <section>
      <h3 className="mb-2 flex items-center gap-2 text-sm font-semibold text-foreground">
        {title}
        {count != null && count > 0 && (
          <span className="rounded bg-muted px-1.5 py-0.5 text-xs font-normal text-muted-foreground">{count}</span>
        )}
      </h3>
      {children}
    </section>
  );
}

/** 只读参数表：param 模式含类型/必填列，header 模式仅名称/示例值/说明 */
function DocTable({ rows, mode }: { rows: ApiParam[]; mode: "param" | "header" }) {
  return (
    <table className="w-full border-collapse text-sm">
      <thead>
        <tr className="border-b border-border text-left text-xs text-muted-foreground">
          <th className="py-1.5 pr-3 font-normal">参数名</th>
          {mode === "param" && (
            <>
              <th className="w-24 py-1.5 pr-3 font-normal">类型</th>
              <th className="w-12 py-1.5 pr-3 font-normal">必填</th>
            </>
          )}
          <th className="py-1.5 pr-3 font-normal">示例值</th>
          <th className="py-1.5 font-normal">说明</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r, i) => (
          <tr key={i} className="border-b border-border/60">
            <td className="py-1.5 pr-3 font-mono text-foreground">{r.key || "—"}</td>
            {mode === "param" && (
              <>
                <td className="w-24 py-1.5 pr-3 text-muted-foreground">{r.type || "string"}</td>
                <td className="w-12 py-1.5 pr-3 text-red-500">{r.required ? "*" : ""}</td>
              </>
            )}
            <td className="py-1.5 pr-3 font-mono text-muted-foreground">{r.example || "—"}</td>
            <td className="py-1.5 text-muted-foreground">{r.description || "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function authSummary(a: InterfaceFile["auth"]): string {
  switch (a.kind) {
    case "bearer":
      return a.token ? `Bearer Token：${a.token}` : "Bearer Token（未设置 Token）";
    case "basic":
      return `Basic Auth：${a.username || "（未设置用户名）"} / ${a.password ? "••••••" : "（未设置密码）"}`;
    case "api-key":
      return `API Key：${a.apiKeyName || "（未设置）"} = ${a.apiKeyValue}（${a.apiKeyIn === "query" ? "查询参数" : "请求头"}）`;
    default:
      return "无鉴权";
  }
}

const OP_TEXT: Record<string, string> = {
  eq: "=",
  ne: "≠",
  contains: "包含",
  "not-contains": "不包含",
  gt: ">",
  ge: "≥",
  lt: "<",
  le: "≤",
  regex: "匹配",
};

function assertionText(a: Assertion): string {
  const op = OP_TEXT[a.op] ?? a.op;
  switch (a.type) {
    case "statusCode":
      return `状态码 ${op} ${a.expected}`;
    case "header":
      return `响应头「${a.key}」${op}「${a.expected}」`;
    case "time":
      return `耗时 ${op} ${a.expectedMs} 毫秒`;
    case "jsonPath":
      return `JSONPath「${a.path}」${op}「${a.expected}」`;
  }
}
