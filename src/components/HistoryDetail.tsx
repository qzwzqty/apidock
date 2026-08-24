import { Send, Loader2 } from "lucide-react";
import type { ApiParam, Auth, HistoryRecord, InterfaceFile, SendOutcome } from "@/lib/api";
import { isJsonBodyEmpty, jsonBodyToValue } from "@/lib/api";
import { methodColor } from "@/lib/methods";
import { ResponseView } from "@/components/ResponseView";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

function timeStr(ms: number): string {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}:${String(d.getSeconds()).padStart(2, "0")}`;
}

export function HistoryDetail({
  record,
  resending,
  onResend,
}: {
  record: HistoryRecord;
  resending: boolean;
  onResend: () => void;
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
      {/* 左：请求详情（只读） */}
      <div className="flex w-3/5 min-w-0 flex-col border-r border-border">
        {/* 请求行 */}
        <div className="flex h-11 shrink-0 items-center gap-2 border-b border-border px-3">
          <span
            className={cn(
              "flex h-6 w-16 shrink-0 items-center justify-center rounded-md text-xs font-bold text-white",
              methodColor(record.method),
            )}
          >
            {record.method}
          </span>
          <span className="min-w-0 flex-1 truncate font-mono text-sm text-foreground" title={record.url}>
            {record.url || "（无地址）"}
          </span>
          <span className="shrink-0 text-[11px] text-muted-foreground">
            {record.createdAtMs ? new Date(record.createdAtMs).toLocaleString("zh-CN", { hour12: false }) : ""}
          </span>
          <Button
            onClick={onResend}
            disabled={resending}
            className="bg-green-600 hover:bg-green-500"
            title="按本次快照重新发送（会记为新的一条历史）"
          >
            <Send className="h-4 w-4" /> {resending ? "发送中…" : "再次发送"}
          </Button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          <RequestReadonly record={record} time={timeStr(record.createdAtMs)} />
        </div>
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

/** 只读请求内容：元信息 + 参数 / 请求头 / Body / 鉴权 */
function RequestReadonly({ record, time }: { record: HistoryRecord; time: string }) {
  const doc = record.doc;
  return (
    <div className="max-w-3xl space-y-4">
      {/* 元信息 */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 rounded-md border border-border bg-muted px-3 py-2 text-xs text-muted-foreground">
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
        <span>时间：{time}</span>
        {record.ok ? (
          <span className="text-green-500">成功</span>
        ) : (
          <span className="text-red-400">失败</span>
        )}
      </div>

      <Section title="查询参数" count={doc.query.length}>
        {doc.query.length > 0 ? (
          <ParamTable rows={doc.query} showType />
        ) : (
          <p className="text-xs text-muted-foreground">无查询参数。</p>
        )}
      </Section>

      <Section title="请求头" count={doc.headers.length}>
        {doc.headers.length > 0 ? (
          <ParamTable rows={doc.headers} />
        ) : (
          <p className="text-xs text-muted-foreground">无请求头。</p>
        )}
      </Section>

      <Section title="请求体">
        <BodyText doc={doc} />
      </Section>

      <Section title="鉴权">
        <p className="text-xs text-foreground">{authText(doc.auth)}</p>
      </Section>
    </div>
  );
}

function Section({ title, count, children }: { title: string; count?: number; children: React.ReactNode }) {
  return (
    <section>
      <h3 className="mb-1.5 flex items-center gap-2 text-xs font-semibold text-foreground">
        {title}
        {count != null && count > 0 && (
          <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] font-normal text-muted-foreground">{count}</span>
        )}
      </h3>
      {children}
    </section>
  );
}

function ParamTable({ rows, showType = false }: { rows: ApiParam[]; showType?: boolean }) {
  return (
    <table className="w-full border-collapse text-xs">
      <thead>
        <tr className="border-b border-border text-left text-[11px] text-muted-foreground">
          <th className="py-1 pr-3 font-normal">参数名</th>
          {showType && <th className="w-20 py-1 pr-3 font-normal">类型</th>}
          <th className="w-12 py-1 pr-3 font-normal">必填</th>
          <th className="py-1 pr-3 font-normal">示例值</th>
          <th className="py-1 font-normal">说明</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r, i) => (
          <tr key={i} className={cn("border-b border-border/60", !r.enabled && "opacity-40")}>
            <td className="py-1 pr-3 font-mono text-foreground">{r.key || "—"}</td>
            {showType && <td className="w-20 py-1 pr-3 text-muted-foreground">{r.type || "string"}</td>}
            <td className="w-12 py-1 pr-3 text-red-500">{r.required ? "*" : ""}</td>
            <td className="py-1 pr-3 font-mono text-muted-foreground">{r.example || "—"}</td>
            <td className="py-1 text-muted-foreground">{r.description || "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function BodyText({ doc }: { doc: InterfaceFile }) {
  const body = doc.body;
  switch (body.mode) {
    case "json": {
      const useTree = !isJsonBodyEmpty(body.json);
      const text = useTree ? JSON.stringify(jsonBodyToValue(body.json), null, 2) : body.content;
      return (
        <pre className="overflow-auto rounded-md border border-border bg-muted p-2.5 font-mono text-xs text-foreground">
          {text || "{}"}
        </pre>
      );
    }
    case "raw":
      return (
        <div>
          <p className="mb-1 text-[11px] text-muted-foreground">Content-Type：{body.contentType || "text/plain"}</p>
          <pre className="overflow-auto whitespace-pre-wrap rounded-md border border-border bg-muted p-2.5 font-mono text-xs text-foreground">
            {body.content || "（空）"}
          </pre>
        </div>
      );
    case "urlencoded":
    case "form-data":
      return body.form.length > 0 ? (
        <ParamTable rows={body.form} />
      ) : (
        <p className="text-xs text-muted-foreground">无表单字段。</p>
      );
    case "file":
      return <p className="font-mono text-xs text-foreground">{body.filePath || "（未设置文件路径）"}</p>;
    default:
      return <p className="text-xs text-muted-foreground">无请求体。</p>;
  }
}

function authText(a: Auth): string {
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