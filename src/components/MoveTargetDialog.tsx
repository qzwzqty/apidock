import { useState } from "react";
import { Folder, Home } from "lucide-react";
import type { TreeNode } from "@/lib/api";
import { Dialog, DialogFooter } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

function collectGroups(nodes: TreeNode[], base: string[], out: { path: string[]; label: string }[]): { path: string[]; label: string }[] {
  for (const n of nodes) {
    if (n.type === "group") {
      const path = [...base, n.key];
      out.push({ path, label: path.map((p) => p).join(" / ") });
      collectGroups(n.children, path, out);
    }
  }
  return out;
}

export function MoveTargetDialog({
  open,
  title,
  tree,
  excludePath,
  onClose,
  onConfirm,
}: {
  open: boolean;
  title: string;
  tree: TreeNode[];
  excludePath?: string[] | null;
  onClose: () => void;
  onConfirm: (target: string[]) => void;
}) {
  const [target, setTarget] = useState<string[] | null>(null);
  const groups = collectGroups(tree, [], []);
  const filtered = excludePath
    ? groups.filter((g) => {
        // 排除自身及子孙
        if (g.path.length < excludePath.length) return true;
        return !(g.path.slice(0, excludePath.length).join("/") === excludePath.join("/"));
      })
    : groups;

  return (
    <Dialog open={open} onClose={onClose} title={title} className="w-[460px]">
      <div className="space-y-2">
        <button
          className={`flex w-full cursor-pointer select-none items-center gap-2 rounded-md border border-border px-3 py-2 text-sm transition-colors ${
            target === null ? "border-accent bg-muted" : "hover:bg-muted"
          }`}
          onClick={() => setTarget(null)}
        >
          <Home className="h-4 w-4 text-muted-foreground" />
          <span>项目根目录</span>
        </button>
        {filtered.map((g) => (
          <button
            key={g.path.join("/")}
            className={`flex w-full cursor-pointer select-none items-center gap-2 rounded-md border border-border px-3 py-2 text-sm transition-colors ${
              target && target.join("/") === g.path.join("/") ? "border-accent bg-muted" : "hover:bg-muted"
            }`}
            onClick={() => setTarget(g.path)}
          >
            <Folder className="h-4 w-4 shrink-0 text-yellow-500/80" />
            <span className="truncate">{g.label}</span>
          </button>
        ))}
      </div>
      <DialogFooter>
        <Button variant="outline" onClick={onClose}>取消</Button>
        <Button disabled={target === undefined} onClick={() => target && onConfirm(target)}>
          移动
        </Button>
      </DialogFooter>
    </Dialog>
  );
}