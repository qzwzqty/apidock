import { useState } from "react";
import { Folder, Home } from "lucide-react";
import type { QuickGroupSummary } from "@/lib/api";
import { Dialog, DialogFooter } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** 按 parentId 展开分组，带层级缩进（快捷请求分组按 id 定位，不用路径键） */
function flatten(
  groups: QuickGroupSummary[],
  parentId: number | null,
  depth: number,
): { id: number; label: string; depth: number }[] {
  const out: { id: number; label: string; depth: number }[] = [];
  for (const g of groups.filter((x) => x.parentId === parentId)) {
    out.push({ id: g.id, label: g.name, depth });
    out.push(...flatten(groups, g.id, depth + 1));
  }
  return out;
}

/**
 * 快捷请求的「移动到分组」选择器：列出未分组根层与全部分组（含嵌套层级）。
 * 分组名可重名，因此按 id 选择而非路径。
 */
export function QuickMoveDialog({
  open,
  title,
  groups,
  onClose,
  onConfirm,
}: {
  open: boolean;
  title: string;
  groups: QuickGroupSummary[];
  onClose: () => void;
  onConfirm: (groupId: number | null) => void;
}) {
  // undefined = 尚未选择（null 是合法目标：未分组根层）
  const [target, setTarget] = useState<number | null | undefined>(undefined);
  const rows = flatten(groups, null, 0);

  return (
    <Dialog open={open} onClose={onClose} title={title} className="w-[460px]">
      <div className="max-h-[320px] space-y-2 overflow-y-auto">
        <button
          className={cn(
            "flex w-full cursor-pointer select-none items-center gap-2 rounded-md border border-border px-3 py-2 text-sm transition-colors",
            target === null ? "border-accent bg-muted" : "hover:bg-muted",
          )}
          onClick={() => setTarget(null)}
        >
          <Home className="h-4 w-4 shrink-0 text-muted-foreground" />
          <span>未分组（快捷请求根层）</span>
        </button>
        {rows.map((g) => (
          <button
            key={g.id}
            className={cn(
              "flex w-full cursor-pointer select-none items-center gap-2 rounded-md border border-border px-3 py-2 text-sm transition-colors",
              target === g.id ? "border-accent bg-muted" : "hover:bg-muted",
            )}
            style={{ paddingLeft: g.depth * 16 + 12 }}
            onClick={() => setTarget(g.id)}
          >
            <Folder className="h-4 w-4 shrink-0 text-yellow-500/80" />
            <span className="truncate">{g.label}</span>
          </button>
        ))}
        {rows.length === 0 && (
          <p className="px-1 text-xs text-muted-foreground">还没有分组，可先在此页面新建分组。</p>
        )}
      </div>
      <DialogFooter>
        <Button variant="outline" onClick={onClose}>取消</Button>
        <Button disabled={target === undefined} onClick={() => onConfirm(target ?? null)}>
          移动
        </Button>
      </DialogFooter>
    </Dialog>
  );
}
