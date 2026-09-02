import { useState } from "react";
import { Copy, Check, X } from "lucide-react";
import type { SendOutcome } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

/** 实际请求快照（历史记录展示用）：method/url/query/headers 均为发送时的最终形态 */
export interface RequestSnapshot {
  method: string;
  url: string;
  query?: { key: string; value: string }[];
  headers?: { key: string; value: string }[];
}

function statusClass(status: number): string {
  if (status >= 200 && status < 300) return "text-green-500";
  if (status >= 400) return "text-red-400";
  return "text-yellow-500";
}

function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

export function ResponseView({
  outcome,
  onClear,
  showClose = true,
  request,
}: {
  outcome: SendOutcome;
  onClear: () => void;
  /** 关闭按钮（历史记录等无关闭语义的场景隐藏） */
  showClose?: boolean;
  /** 实际请求信息（历史记录详情展示） */
  request?: RequestSnapshot | null;
}) {
  const [pretty, setPretty] = useState(true);
  const [copied, setCopied] = useState(false);

  if (!outcome.ok) {
    return (
      <div className="flex h-full flex-col">
        <Header onClear={onClear} showClose={showClose} title="发送失败" />
        <div className="flex-1 overflow-y-auto px-4 py-3">
          <div className="mb-2 flex items-center gap-2">
            <span className="rounded bg-red-500/20 px-2 py-0.5 text-xs text-red-400">
              {errKindText(outcome.err.kind)}
            </span>
            <span className="text-sm text-red-400">{outcome.err.message}</span>
          </div>
        </div>
      </div>
    );
  }

  const res = outcome.res;
  // 是否为 JSON 由响应内容决定，与「格式化」开关无关（否则取消勾选后开关会消失）
  let okJson = false;
  try {
    JSON.parse(res.body);
    okJson = true;
  } catch {
    // 非 JSON 响应体
  }
  let body = res.body;
  if (okJson && pretty) {
    body = JSON.stringify(JSON.parse(res.body), null, 2);
  }

  const copy = async () => {
    await navigator.clipboard.writeText(body);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className="flex h-full flex-col">
      <Header
        onClear={onClear}
        showClose={showClose}
        title={
          <span className={cn("text-lg font-semibold", statusClass(res.status))}>
            {res.status} {res.statusText}
          </span>
        }
        extra={
          <span className="flex items-center gap-3 text-xs text-muted-foreground">
            <span>{res.timeMs} ms</span>
            <span>{bytes(res.sizeBytes)}</span>
            {okJson && (
              <label className="flex cursor-pointer items-center gap-1">
                <input type="checkbox" checked={pretty} onChange={(e) => setPretty(e.target.checked)} />
                格式化
              </label>
            )}
            <Button size="sm" variant="ghost" onClick={copy} title="复制响应体">
              {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
            </Button>
          </span>
        }
      />
      <div className="flex-1 overflow-y-auto px-4 py-3">
        {request && <RequestBlock request={request} />}
        <p className="mb-2 break-all text-xs text-muted-foreground">{res.resolvedUrl}</p>
        <div className="mb-3">
          <h4 className="mb-1 text-xs font-medium text-muted-foreground">响应头（{res.headers.length}）</h4>
          <div className="space-y-0.5">
            {res.headers.map((h) => (
              <div key={h.key} className="flex gap-2 text-xs">
                <span className="shrink-0 text-accent">{h.key}:</span>
                <span className="break-all text-foreground">{h.value}</span>
              </div>
            ))}
          </div>
        </div>
        <h4 className="mb-1 text-xs font-medium text-muted-foreground">
          响应体
          {res.truncated && <span className="ml-2 text-red-400">（已截断，仅显示前 1MB）</span>}
        </h4>
        <pre className="whitespace-pre-wrap rounded-md border border-border bg-[#0f1115] p-3 font-mono text-xs leading-relaxed text-green-50">
          {body}
        </pre>
      </div>
    </div>
  );
}

/** 实际请求快照：请求行 + 查询参数 + 请求头（发送时的最终形态） */
function RequestBlock({ request }: { request: RequestSnapshot }) {
  return (
    <div className="mb-3 rounded-md border border-border bg-muted p-2.5">
      <h4 className="mb-1.5 text-xs font-medium text-muted-foreground">实际请求</h4>
      <div className="mb-1 font-mono text-xs text-foreground">
        <span className="mr-2 rounded bg-orange-500/90 px-1.5 py-0.5 text-[10px] font-bold text-white">
          {request.method}
        </span>
        <span className="break-all">{request.url}</span>
      </div>
      {request.query && request.query.length > 0 && (
        <div className="mb-1">
          <p className="text-[11px] text-muted-foreground">查询参数（{request.query.length}）</p>
          {request.query.map((q, i) => (
            <div key={i} className="flex gap-2 font-mono text-xs">
              <span className="shrink-0 text-accent">{q.key}:</span>
              <span className="break-all text-foreground">{q.value}</span>
            </div>
          ))}
        </div>
      )}
      {request.headers && request.headers.length > 0 && (
        <div>
          <p className="text-[11px] text-muted-foreground">请求头（{request.headers.length}）</p>
          {request.headers.map((h, i) => (
            <div key={i} className="flex gap-2 font-mono text-xs">
              <span className="shrink-0 text-accent">{h.key}:</span>
              <span className="break-all text-foreground">{h.value}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function Header({
  title,
  extra,
  onClear,
  showClose = true,
}: {
  title: React.ReactNode;
  extra?: React.ReactNode;
  onClear: () => void;
  showClose?: boolean;
}) {
  return (
    <div className="flex h-11 shrink-0 items-center gap-3 border-b border-border px-4">
      {title}
      {extra && <span className="ml-auto">{extra}</span>}
      {showClose && (
        <button
          className="ml-auto cursor-pointer rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          onClick={onClear}
          title="关闭响应面板"
        >
          <X className="h-4 w-4" />
        </button>
      )}
    </div>
  );
}

function errKindText(kind: string): string {
  switch (kind) {
    case "timeout": return "请求超时";
    case "connect": return "连接失败";
    case "redirect": return "重定向错误";
    case "unresolved": return "未解析变量";
    case "url": return "URL 错误";
    case "file": return "文件错误";
    default: return "HTTP 错误";
  }
}