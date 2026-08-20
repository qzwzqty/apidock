import { useState } from "react";
import { Copy, Check, X } from "lucide-react";
import type { SendOutcome } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

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
}: {
  outcome: SendOutcome;
  onClear: () => void;
}) {
  const [pretty, setPretty] = useState(true);
  const [copied, setCopied] = useState(false);

  if (!outcome.ok) {
    return (
      <div className="flex h-full flex-col">
        <Header onClear={onClear} title="发送失败" />
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
  let body = res.body;
  let okJson = false;
  if (pretty) {
    try {
      body = JSON.stringify(JSON.parse(res.body), null, 2);
      okJson = true;
    } catch {
      body = res.body;
    }
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

function Header({
  title,
  extra,
  onClear,
}: {
  title: React.ReactNode;
  extra?: React.ReactNode;
  onClear: () => void;
}) {
  return (
    <div className="flex h-11 shrink-0 items-center gap-3 border-b border-border px-4">
      {title}
      {extra && <span className="ml-auto">{extra}</span>}
      <button
        className="ml-auto cursor-pointer rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
        onClick={onClear}
        title="关闭响应面板"
      >
        <X className="h-4 w-4" />
      </button>
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