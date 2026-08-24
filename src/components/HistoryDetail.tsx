import { useState } from "react";
import { marked } from "marked";
import { Send, Loader2, SlidersHorizontal, Eye, PencilLine } from "lucide-react";
import type { ApiParam, Assertion, HistoryRecord, InterfaceFile, KeyValue, SendOutcome } from "@/lib/api";
import { newBodyField } from "@/lib/api";
import { METHODS } from "@/lib/methods";
import { ResponseView } from "@/components/ResponseView";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  ParamList,
  BodyEditor,
  AuthEditor,
  KvList,
  AssertionEditor,
  SendOptionsDialog,
  type Tab,
} from "@/components/InterfaceEditor";

/** 可编辑的请求面板：与接口管理-调试界面一致（方法/URL/参数/请求头/Body/鉴权/变量/断言/说明），
 *  编辑结果仅用于「再次发送」，不落库。切换历史记录时整体重置（外层按 record.id 传 key）。 */
function EditableRequest({
  record,
  resending,
  onResend,
}: {
  record: HistoryRecord;
  resending: boolean;
  onResend: (doc: InterfaceFile) => void;
}) {
  const [doc, setDoc] = useState<InterfaceFile>({ ...record.doc });
  const [tab, setTab] = useState<Tab>("params");
  const [debugJson, setDebugJson] = useState<string | null>(null);
  const [showOpts, setShowOpts] = useState(false);
  const [preview, setPreview] = useState(false);

  const update = (patch: Partial<InterfaceFile>) => setDoc((d) => ({ ...d, ...patch }));
  const updateList = (field: "query" | "headers", list: ApiParam[]) =>
    update({ [field]: list } as Partial<InterfaceFile>);
  const updateKv = (field: "variables", list: KeyValue[]) =>
    update({ [field]: list } as Partial<InterfaceFile>);
  const updateAssertions = (list: Assertion[]) => update({ assertions: list });

  /** 发送：JSON 模式按用户输入文本发送（清空结构树回落 content），与调试界面一致 */
  const handleSend = () => {
    if (doc.body.mode === "json" && debugJson != null) {
      onResend({
        ...doc,
        body: { ...doc.body, json: { root: { ...newBodyField(""), type: "" } }, content: debugJson },
      });
    } else {
      onResend(doc);
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* 请求行：方法与地址可编辑 */}
      <div className="flex shrink-0 items-center gap-2 px-3 py-2">
        <select
          className="h-9 min-w-20 cursor-pointer rounded-md border border-border bg-muted px-2 text-sm font-semibold text-accent outline-none focus:border-ring"
          value={doc.method}
          onChange={(e) => update({ method: e.target.value })}
        >
          {METHODS.map((m) => (
            <option key={m} value={m}>{m}</option>
          ))}
        </select>
        <Input
          className="h-9 flex-1"
          placeholder="请求地址，如 {{host}}/api/login"
          value={doc.url}
          onChange={(e) => update({ url: e.target.value })}
        />
        <Button
          variant="ghost"
          title="发送选项（超时/重定向/TLS/CA）"
          onClick={() => setShowOpts(true)}
        >
          <SlidersHorizontal className="h-4 w-4" />
        </Button>
        <Button
          onClick={handleSend}
          disabled={resending}
          className="bg-green-600 hover:bg-green-500"
          title="按当前内容重新发送（会记为新的一条历史）"
        >
          <Send className="h-4 w-4" /> {resending ? "发送中…" : "再次发送"}
        </Button>
      </div>

      {/* 元信息行：接口/项目/环境/时间 */}
      <div className="flex h-7 shrink-0 items-center gap-4 border-y border-border px-3 text-[11px] text-muted-foreground">
        <span>
          接口：<span className="text-foreground">{record.ifaceName || "（未知）"}</span>
          {record.ifaceKey && <span className="text-muted-foreground/70">（{record.ifaceKey}）</span>}
        </span>
        <span>
          项目：<span className="text-foreground">{record.projectName || record.projectKey}</span>
        </span>
        <span>
          环境：<span className="text-foreground">{record.envName || record.envId}</span>
          {record.env.host && <span className="font-mono text-muted-foreground/70">（{record.env.host}）</span>}
        </span>
        <span className="ml-auto shrink-0">
          时间：{record.createdAtMs ? new Date(record.createdAtMs).toLocaleString("zh-CN", { hour12: false }) : ""}
        </span>
      </div>

      {/* 区块标签（与调试界面一致） */}
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

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {tab === "params" && (
          <ParamList
            rows={doc.query}
            onChange={(list) => updateList("query", list)}
            placeholderK="参数名"
            placeholderV="示例值"
            showEnabled
          />
        )}
        {tab === "headers" && (
          <ParamList rows={doc.headers} onChange={(list) => updateList("headers", list)} showEnabled />
        )}
        {tab === "body" && (
          <BodyEditor
            key={doc.id}
            body={doc.body}
            onChange={(body) => update({ body })}
            debugMode
            debugJson={debugJson}
            onDebugJsonChange={setDebugJson}
          />
        )}
        {tab === "auth" && <AuthEditor auth={doc.auth} onChange={(auth) => update({ auth })} />}
        {tab === "vars" && <KvList rows={doc.variables} onChange={(list) => updateKv("variables", list)} />}
        {tab === "assert" && <AssertionEditor rows={doc.assertions} onChange={updateAssertions} />}
        {tab === "desc" && (
          <div>
            <div className="mb-1 flex items-center justify-between">
              <label className="text-xs text-muted-foreground">接口说明（支持 Markdown）</label>
              <Button size="sm" variant="outline" onClick={() => setPreview(!preview)}>
                {preview ? <PencilLine className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
                {preview ? "编辑" : "预览"}
              </Button>
            </div>
            {preview ? (
              <div
                className="markdown-body prose prose-sm max-w-none rounded-md border border-border bg-muted p-3 text-sm text-foreground"
                dangerouslySetInnerHTML={{ __html: marked.parse(doc.description || "*暂无说明*") as string }}
              />
            ) : (
              <textarea
                className="h-40 w-full resize-y rounded-md border border-border bg-muted p-2.5 text-sm text-foreground placeholder:text-muted-foreground outline-none focus:border-ring"
                value={doc.description}
                onChange={(e) => update({ description: e.target.value })}
                placeholder="# 接口说明"
              />
            )}
          </div>
        )}
      </div>

      <SendOptionsDialog
        iface={doc}
        open={showOpts}
        onClose={() => setShowOpts(false)}
        onApply={(patch) => {
          update(patch);
          setShowOpts(false);
        }}
      />
    </div>
  );
}

export function HistoryDetail({
  record,
  resending,
  onResend,
}: {
  record: HistoryRecord;
  resending: boolean;
  onResend: (doc: InterfaceFile) => void;
}) {
  // 历史记录要么成功（response）要么失败（error）
  const outcome: SendOutcome =
    record.response != null
      ? { ok: true, res: record.response }
      : {
          ok: false,
          err: record.error ?? { kind: "http", message: "无响应数据" },
        };

  return (
    <div className="flex h-full min-w-0">
      {/* 左：可编辑请求（与调试界面一致） */}
      <div className="flex w-3/5 min-w-0 flex-col border-r border-border">
        <EditableRequest
          key={record.id}
          record={record}
          resending={resending}
          onResend={onResend}
        />
      </div>

      {/* 右：响应 */}
      <div className="relative min-w-0 flex-1">
        {resending && (
          <div className="absolute inset-0 z-10 flex items-center justify-center gap-2 bg-background/70 text-sm text-accent">
            <Loader2 className="h-4 w-4 animate-spin" /> 发送中…
          </div>
        )}
        <ResponseView outcome={outcome} onClear={() => {}} />
      </div>
    </div>
  );
}