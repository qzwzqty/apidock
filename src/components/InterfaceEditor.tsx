import { useState } from "react";
import { Save, Send, Check } from "lucide-react";
import type { InterfaceFile, KeyValue } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

type Tab = "params" | "headers" | "body" | "auth" | "vars" | "desc";

const EMPTY_BODY = { mode: "none", content: "", contentType: "", form: [], filePath: null };
const EMPTY_AUTH = { kind: "none", token: "", username: "", password: "", apiKeyName: "", apiKeyIn: "header", apiKeyValue: "" };
// 向后兼容：旧文件缺 body/auth/variables
function normalize(iface: InterfaceFile): InterfaceFile {
  return {
    ...iface,
    headers: iface.headers ?? [],
    query: iface.query ?? [],
    variables: iface.variables ?? [],
    body: { ...EMPTY_BODY, ...(iface.body ?? {}) },
    auth: { ...EMPTY_AUTH, ...(iface.auth ?? {}) },
  };
}

export function InterfaceEditor({
  doc,
  onSave,
  onSend,
}: {
  doc: InterfaceFile;
  onSave: (doc: InterfaceFile) => Promise<void>;
  onSend: (doc: InterfaceFile) => void;
}) {
  const [base, setBase] = useState<InterfaceFile>(normalize(doc));
  const [tab, setTab] = useState<Tab>("params");
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  // 外部 doc 变化（切换接口/外部刷新）时同步
  const [lastDocId, setLastDocId] = useState(doc.id);
  if (doc.id !== lastDocId) {
    setLastDocId(doc.id);
    setBase(normalize(doc));
    setDirty(false);
    setSaved(false);
  }

  const update = (patch: Partial<InterfaceFile>) => {
    setBase((d) => ({ ...d, ...patch }));
    setDirty(true);
    setSaved(false);
  };
  const updateList = (field: "headers" | "query" | "variables", list: KeyValue[]) =>
    update({ [field]: list } as Partial<InterfaceFile>);

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

  return (
    <div className="flex h-full flex-col">
      {/* 请求行 */}
      <div className="flex items-center gap-2 px-4 py-3">
        <select
          className="h-9 min-w-20 rounded-md border border-border bg-muted px-2 text-sm font-semibold text-accent outline-none focus:border-ring cursor-pointer"
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
        <Button onClick={save} disabled={saving || (!dirty && !saved)} variant="outline">
          <Save className="h-4 w-4" /> 保存
        </Button>
        <Button onClick={() => onSend(base)} className="bg-green-600 hover:bg-green-500">
          <Send className="h-4 w-4" /> 发送
        </Button>
      </div>

      {/* 区块标签 */}
      <div className="flex h-9 shrink-0 items-center border-b border-border px-2 text-sm">
        {(
          [
            ["params", "参数"],
            ["headers", "请求头"],
            ["body", "Body"],
            ["auth", "鉴权"],
            ["vars", "变量"],
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
        {saved && !dirty && (
          <span className="ml-auto flex items-center gap-1 pr-2 text-xs text-green-500">
            <Check className="h-3 w-3" /> 已保存
          </span>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {tab === "params" && (
          <KvList
            rows={base.query}
            onChange={(list) => updateList("query", list)}
            placeholderK="param"
            placeholderV="value"
          />
        )}
        {tab === "headers" && (
          <KvList rows={base.headers} onChange={(list) => updateList("headers", list)} />
        )}
        {tab === "body" && <BodyEditor body={base.body} onChange={(body) => update({ body })} />}
        {tab === "auth" && <AuthEditor auth={base.auth} onChange={(auth) => update({ auth })} />}
        {tab === "vars" && (
          <KvList rows={base.variables} onChange={(list) => updateList("variables", list)} />
        )}
        {tab === "desc" && (
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">
              接口说明（Markdown 渲染在后续版本）
            </label>
            <textarea
              className="h-40 w-full resize-y rounded-md border border-border bg-muted p-2.5 text-sm text-foreground placeholder:text-muted-foreground outline-none focus:border-ring"
              value={base.description}
              onChange={(e) => update({ description: e.target.value })}
              placeholder="# 接口说明"

            />
          </div>
        )}
      </div>
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

function BodyEditor({
  body,
  onChange,
}: {
  body: InterfaceFile["body"];
  onChange: (body: InterfaceFile["body"]) => void;
}) {
  const setForm = (form: KeyValue[]) => onChange({ ...body, form });
  const isText = body.mode === "json" || body.mode === "raw";
  return (
    <div className="space-y-3">
      <div>
        <label className="mb-1 block text-xs text-muted-foreground">Body 类型</label>
        <select
          className="h-8 rounded-md border border-border bg-muted px-2 text-sm outline-none focus:border-ring cursor-pointer"
          value={body.mode}
          onChange={(e) => onChange({ ...body, mode: e.target.value })}
        >
          <option value="none">none</option>
          <option value="json">JSON</option>
          <option value="raw">Raw 文本</option>
          <option value="urlencoded">URL 编码表单</option>
          <option value="form-data">Form-data 多段表单</option>
          <option value="file">文件 (file)</option>
        </select>
      </div>

      {isText && (
        <div>
          {body.mode === "raw" && (
            <div className="mb-2 flex w-72 items-center gap-2">
              <label className="text-xs text-muted-foreground">Content-Type</label>
              <Input
                className="h-7"
                value={body.contentType}
                placeholder="text/plain"
                onChange={(e) => onChange({ ...body, contentType: e.target.value })}
              />
            </div>
          )}
          <textarea
            className="h-52 w-full resize-y rounded-md border border-border bg-muted p-2.5 font-mono text-xs text-foreground outline-none focus:border-ring"
            value={body.content}
            onChange={(e) => onChange({ ...body, content: e.target.value })}
            spellCheck={false}
            placeholder={body.mode === "json" ? '{"key": "{{var}}"}' : "原始文本"}
          />
        </div>
      )}

      {(body.mode === "urlencoded" || body.mode === "form-data") && (
        <KvList rows={body.form} onChange={setForm} placeholderK="field" placeholderV={body.mode === "form-data" ? "值 或 @文件路径" : "value"} />
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
    </div>
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