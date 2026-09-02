import { useEffect, useMemo, useState } from "react";
import { History, Search, Trash2, Loader2 } from "lucide-react";
import type { HistorySummary } from "@/lib/api";
import { useHistory } from "@/lib/history";
import { methodColor, statusClass } from "@/lib/methods";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { HistoryDetail } from "@/components/HistoryDetail";

/** 本地日期键 YYYY-MM-DD */
function dayKey(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** 分组标题：今天 / 昨天 / M月d日（跨年含年份） */
function dayLabel(key: string): string {
  const now = Date.now();
  if (key === dayKey(now)) return "今天";
  if (key === dayKey(now - 86_400_000)) return "昨天";
  const [y, m, d] = key.split("-").map(Number);
  const cur = new Date();
  return cur.getFullYear() === y ? `${m}月${d}日` : `${y}年${m}月${d}日`;
}

function timeStr(ms: number): string {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}:${String(d.getSeconds()).padStart(2, "0")}`;
}

export function RequestHistoryPage() {
  const { entries, loaded, loading, selectedId, select, remove, clear, load } = useHistory();
  const [keyword, setKeyword] = useState("");

  useEffect(() => {
    void load();
  }, [load]);

  const filtered = useMemo(() => {
    const q = keyword.trim().toLowerCase();
    if (!q) return entries;
    return entries.filter(
      (e) =>
        e.url.toLowerCase().includes(q) ||
        e.ifaceName.toLowerCase().includes(q) ||
        e.method.toLowerCase().includes(q) ||
        String(e.status ?? "").includes(q),
    );
  }, [entries, keyword]);

  /** 按日期分组（保持倒序） */
  const groups = useMemo(() => {
    const map = new Map<string, HistorySummary[]>();
    for (const e of filtered) {
      const k = dayKey(e.createdAtMs);
      const list = map.get(k) ?? [];
      list.push(e);
      map.set(k, list);
    }
    return [...map.entries()];
  }, [filtered]);

  return (
    <div className="flex h-full">
      {/* 左侧：按日期分组的请求历史列表 */}
      <aside className="flex w-72 shrink-0 flex-col border-r border-border bg-muted">
        <div className="flex items-center justify-between px-3 py-2">
          <span className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
            <History className="h-4 w-4" /> 请求历史
            {loaded && <span className="text-xs font-normal text-muted-foreground">（{entries.length}）</span>}
          </span>
          <button
            className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-red-400 cursor-pointer"
            title="清空全部历史"
            onClick={() => {
              if (confirm("清空全部请求历史？此操作不可恢复。")) void clear();
            }}
          >
            <Trash2 className="h-4 w-4" />
          </button>
        </div>
        <div className="px-2 pb-2">
          <div className="relative">
            <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              className="pl-7"
              placeholder="搜索方法 / 地址 / 接口名"
              value={keyword}
              onChange={(e) => setKeyword(e.target.value)}
            />
          </div>
        </div>
        <div className="flex-1 overflow-y-auto px-2 pb-2">
          {loading && !loaded ? (
            <p className="flex items-center gap-1.5 px-2 py-3 text-xs text-muted-foreground">
              <Loader2 className="h-3.5 w-3.5 animate-spin" /> 加载中…
            </p>
          ) : groups.length === 0 ? (
            <p className="px-2 py-3 text-xs text-muted-foreground">
              {keyword ? "没有匹配的历史记录" : "暂无请求历史，发送请求后会自动记录"}
            </p>
          ) : (
            groups.map(([key, list]) => (
              <div key={key} className="mb-2">
                <div className="sticky top-0 z-10 mb-0.5 flex items-center gap-2 bg-muted px-2 py-1 text-[11px] font-medium text-muted-foreground">
                  <span>{dayLabel(key)}</span>
                  <span className="rounded bg-border/60 px-1 text-[10px]">{list.length}</span>
                  <span className="hidden flex-1 text-right text-[10px] text-muted-foreground/60">{key}</span>
                </div>
                {list.map((e) => (
                  <HistoryRow
                    key={e.id}
                    entry={e}
                    active={e.id === selectedId}
                    onClick={() => void select(e.id)}
                    onDelete={() => void remove(e.id)}
                  />
                ))}
              </div>
            ))
          )}
        </div>
      </aside>

      {/* 右侧：详情 + 再次发送 */}
      <main className="flex min-w-0 flex-1 flex-col">
        <HistoryDetailShell />
      </main>
    </div>
  );
}

function HistoryRow({
  entry: e,
  active,
  onClick,
  onDelete,
}: {
  entry: HistorySummary;
  active: boolean;
  onClick: () => void;
  onDelete: () => void;
}) {
  return (
    <div
      className={cn(
        "group relative flex cursor-pointer select-none flex-col gap-1 rounded-md border-l-2 px-2 py-1.5 transition-colors",
        active ? "border-accent bg-accent/10" : "border-transparent hover:bg-muted",
      )}
      onClick={onClick}
    >
      <div className="flex min-w-0 items-center gap-1.5 pr-5">
        <span
          className={cn(
            "flex h-5 w-12 shrink-0 items-center justify-center rounded text-[10px] font-bold text-white",
            methodColor(e.method),
          )}
        >
          {e.method}
        </span>
        <span
          className="truncate font-mono text-xs text-foreground"
          title={e.url}
        >
          {e.url || "（无地址）"}
        </span>
      </div>
      <div className="flex min-w-0 items-center gap-1.5 pl-[52px] text-[11px] text-muted-foreground">
        {e.ok && e.status != null ? (
          <span className={cn("font-medium", statusClass(e.status))}>{e.status}</span>
        ) : (
          <span className="text-red-400">失败</span>
        )}
        <span>{e.timeMs}ms</span>
        <span className="truncate">{e.ifaceName || e.projectName}</span>
        <span className="ml-auto shrink-0">{timeStr(e.createdAtMs)}</span>
      </div>
      <button
        className="absolute right-1 top-1.5 rounded-sm p-0.5 text-muted-foreground opacity-0 transition-opacity hover:bg-border hover:text-red-400 group-hover:opacity-100 cursor-pointer"
        title="删除该条历史"
        onClick={(ev) => {
          ev.stopPropagation();
          onDelete();
        }}
      >
        <Trash2 className="h-3 w-3" />
      </button>
    </div>
  );
}

/** 详情区域：经由 store 取当前选中记录 */
function HistoryDetailShell() {
  const { detail, detailLoading, resending, resend } = useHistory();
  if (detailLoading && !detail) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        <Loader2 className="mr-2 h-4 w-4 animate-spin" /> 加载中…
      </div>
    );
  }
  if (!detail) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 text-sm text-muted-foreground">
        <History className="h-8 w-8 opacity-40" />
        从左侧选择一条历史记录查看详情
        <span className="text-xs">或在接口调试页发送请求，历史会自动记录在这里</span>
      </div>
    );
  }
  return (
    <HistoryDetail
      record={detail}
      resending={resending}
      onResend={(doc) => void resend(detail.id, doc)}
    />
  );
}