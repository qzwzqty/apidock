import { Loader2, Check, X as XIcon, ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import type { RunReport } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Dialog } from "@/components/ui/dialog";

export function RunReportDialog({
  open,
  running,
  report,
  onClose,
}: {
  open: boolean;
  running: boolean;
  report: RunReport | null;
  onClose: () => void;
}) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});

  return (
    <Dialog open={open} onClose={onClose} title="分组/项目一键运行报告" className="w-[720px]">
      <div className="max-h-[65vh] overflow-y-auto">
        {running && (
          <p className="flex items-center gap-2 py-4 text-sm text-accent">
            <Loader2 className="h-4 w-4 animate-spin" /> 正在运行…
          </p>
        )}
        {!running && report && (
          <>
            <div className="mb-3 flex items-center gap-3 rounded-md border border-border bg-muted p-3 text-sm">
              <span>共 {report.total}</span>
              <span className="text-green-500">通过 {report.passed}</span>
              <span className={report.failed ? "text-red-400" : ""}>失败 {report.failed}</span>
            </div>
            <div className="space-y-1">
              {report.items.map((item) => {
                const id = item.groupPath.join("/") + "/" + item.key;
                const exp = expanded[id];
                return (
                  <div
                    key={id}
                    className="rounded-md border border-border"
                  >
                    <button
                      className="flex w-full cursor-pointer select-none items-center gap-2 px-3 py-2 text-left text-sm hover:bg-muted"
                      onClick={() => setExpanded((e) => ({ ...e, [id]: !e[id] }))}
                    >
                      {exp ? <ChevronDown className="h-4 w-4 shrink-0 text-muted-foreground" /> : <ChevronRight className="h-4 w-4 shrink-0 text-muted-foreground" />}
                      {item.ok ? <Check className="h-4 w-4 shrink-0 text-green-500" /> : <XIcon className="h-4 w-4 shrink-0 text-red-400" />}
                      <span className={cn("w-12 shrink-0", item.method.startsWith("G") ? "text-green-600" : "text-orange-400")}>
                        {item.method}
                      </span>
                      <span className="truncate text-foreground">{item.name}</span>
                      {item.status != null && (
                        <span className={cn("ml-auto text-xs", item.status >= 400 ? "text-red-400" : "text-muted-foreground")}>
                          {item.status} {item.timeMs}ms
                        </span>
                      )}
                    </button>
                    {exp && (
                      <div className="space-y-1 border-t border-border px-3 py-2 text-xs">
                        <p className="break-all text-muted-foreground">{item.method} {item.url}</p>
                        {item.error && <p className="text-red-400">错误：{item.error}</p>}
                        {item.assertionResults.length > 0 && (
                          <div className="space-y-0.5">
                            {item.assertionResults.map((r, i) => (
                              <p key={i} className={r.passed ? "text-green-500" : "text-red-400"}>
                                {r.passed ? "✓ " : "✗ "}{r.message}
                              </p>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </>
        )}
      </div>
    </Dialog>
  );
}