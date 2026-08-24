import { useMemo, useState } from "react";
import { marked } from "marked";
import { Send, Loader2, SlidersHorizontal } from "lucide-react";
import type { ApiParam, Assertion, HistoryRecord, InterfaceFile, KeyValue, SendOutcome } from "@/lib/api";
import { newBodyField } from "@/lib/api";
import { METHODS } from "@/lib/methods";
import { normalizeUrlForSend } from "@/lib/url";
import { ResponseView, type RequestSnapshot } from "@/components/ResponseView";
import { Button } from "@/components/ui/button";
import {
  ParamList,
  BodyEditor,
  AuthEditor,
  KvList,
  AssertionEditor,
  SendOptionsDialog,
  UrlInput,
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

  const update = (patch: Partial<InterfaceFile>) => setDoc((d) => ({ ...d, ...patch }));
  const updateList = (field: "query" | "headers", list: ApiParam[]) =>
    update({ [field]: list } as Partial<InterfaceFile>);
  const updateKv = (field: "variables", list: KeyValue[]) =>
    update({ [field]: list } as Partial<InterfaceFile>);
  const updateAssertions = (list: Assertion[]) => update({ assertions: list });

  /** 发送：URL 统一规范化为 {{host}}/路径（host 为记录时环境快照）；
   *  JSON 模式按用户输入文本发送（清空结构树回落 content），与调试界面一致 */
  const handleSend = () => {
    const sendDoc: InterfaceFile = {
      ...doc,
      url: normalizeUrlForSend(doc.url, record.env.host),
    };
    if (doc.body.mode === "json" && debugJson != null) {
      onResend({
        ...sendDoc,
        body: { ...doc.body, json: { root: { ...newBodyField(""), type: "" } }, content: debugJson },
      });
    } else {
      onResend(sendDoc);
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* 请求行：方法与地址可编辑（host 固定为记录时环境快照，不可改） */}
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
        <UrlInput
          url={doc.url}
          host={record.env.host}
          onChange={(url) => update({ url })}
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
          disabled={resending || !record.env.host.trim()}
          className="bg-green-600 hover:bg-green-500"
          title={record.env.host.trim() ? "按当前内容重新发送（会记为新的一条历史）" : "该历史记录的环境未配置 host，无法重新发送"}
        >
          <Send className="h-4 w-4" /> {resending ? "发送中…" : "再次发送"}
        </Button>
      </div>

      {/* 元信息：第一行接口（可省略不换行），第二行项目 / 环境 / 时间 */}
      <div className="shrink-0 border-y border-border px-3 py-1.5 text-xs">
        <div className="flex min-w-0 items-baseline gap-2">
          <span className="shrink-0 text-muted-foreground">接口：</span>
          <span className="min-w-0 flex-1 truncate text-foreground" title={record.ifaceName || undefined}>
            {record.ifaceName || "（未知）"}
          </span>
          {record.ifaceKey && record.ifaceKey !== record.ifaceName && (
            <span className="shrink-0 font-mono text-[11px] text-muted-foreground/70">
              （{record.ifaceKey}）
            </span>
          )}
        </div>
        <div className="mt-1 flex min-w-0 flex-wrap items-baseline gap-x-4 gap-y-0.5 text-[11px] text-muted-foreground">
          <span>
            项目：<span className="text-foreground">{record.projectName || record.projectKey}</span>
          </span>
          <span>
            环境：<span className="text-foreground">{record.envName || record.envId}</span>
            {record.env.host && <span className="font-mono text-muted-foreground/70">（{record.env.host}）</span>}
          </span>
          <span>
            时间：
            {record.createdAtMs
              ? new Date(record.createdAtMs).toLocaleString("zh-CN", { hour12: false })
              : ""}
          </span>
        </div>
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
            showAutoGenerate={false}
          />
        )}
        {tab === "auth" && <AuthEditor auth={doc.auth} onChange={(auth) => update({ auth })} />}
        {tab === "vars" && <KvList rows={doc.variables} onChange={(list) => updateKv("variables", list)} />}
        {tab === "assert" && <AssertionEditor rows={doc.assertions} onChange={updateAssertions} />}
        {tab === "desc" && (
          <div className="max-w-3xl">
            <div className="mb-1.5">
              <label className="text-xs text-muted-foreground">接口说明（只读预览）</label>
            </div>
            {doc.description.trim() ? (
              <div
                className="markdown-body prose prose-sm max-w-none rounded-md border border-border bg-muted p-3 text-sm text-foreground"
                dangerouslySetInnerHTML={{ __html: marked.parse(doc.description) as string }}
              />
            ) : (
              <p className="text-xs text-muted-foreground">暂无说明。</p>
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

  // 实际请求快照：发送时的最终形态（接口定义已解析实际值 + 全局参数注入）
  const requestSnapshot: RequestSnapshot = useMemo(() => {
    const doc = record.doc;
    const query: { key: string; value: string }[] = [];
    for (const kv of record.globalParams.query) {
      if (kv.enabled && kv.key.trim()) query.push({ key: kv.key, value: kv.value });
    }
    for (const p of doc.query) {
      if (p.enabled && p.key.trim()) query.push({ key: p.key, value: p.example });
    }
    const headers: { key: string; value: string }[] = [];
    for (const kv of record.globalParams.headers) {
      if (kv.enabled && kv.key.trim()) headers.push({ key: kv.key, value: kv.value });
    }
    for (const c of record.globalParams.cookies) {
      if (c.enabled && c.key.trim()) headers.push({ key: "Cookie", value: `${c.key}=${c.value}` });
    }
    for (const p of doc.headers) {
      if (p.enabled && p.key.trim()) headers.push({ key: p.key, value: p.example });
    }
    return { method: doc.method, url: record.url, query, headers };
  }, [record]);

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
        <ResponseView
          outcome={outcome}
          onClear={() => {}}
          showClose={false}
          request={requestSnapshot}
        />
      </div>
    </div>
  );
}