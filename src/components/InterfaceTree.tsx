import { useState } from "react";
import { ChevronDown, ChevronRight, FolderPlus, FilePlus2, Pencil, Trash2, Folder, Play, MoveRight } from "lucide-react";
import type { TreeNode } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogFooter } from "@/components/ui/dialog";

export interface IfaceRef {
  groupPath: string[];
  key: string;
}

export function InterfaceTree({
  tree,
  activeId,
  onOpenIface,
  onCreateGroup,
  onRenameGroup,
  onDeleteGroup,
  onCreateIface,
  onRenameIface,
  onDeleteIface,
  onRunGroup,
  onMoveIface,
  onMoveGroup,
}: {
  tree: TreeNode[];
  activeId: string | null;
  onOpenIface: (ref: IfaceRef) => void;
  onCreateGroup: (parentPath: string[]) => void;
  onRenameGroup: (ref: IfaceRef) => void;
  onDeleteGroup: (ref: IfaceRef) => void;
  onCreateIface: (groupPath: string[]) => void;
  onRenameIface: (ref: IfaceRef) => void;
  onDeleteIface: (ref: IfaceRef) => void;
  onRunGroup?: (groupPath: string[]) => void;
  onMoveIface?: (ref: IfaceRef) => void;
  onMoveGroup?: (ref: IfaceRef) => void;
}) {
  return (
    <div className="select-none">
      {tree.length === 0 && (
        <p className="px-2 py-3 text-xs text-muted-foreground">
          还没有接口，点击上方「更多」菜单新建分组或接口
        </p>
      )}
      {tree.map((node) => (
        <Node
          key={node.type === "group" ? node.key : node.key}
          node={node}
          path={[]}
          depth={0}
          activeId={activeId}
          onOpenIface={onOpenIface}
          onCreateGroup={onCreateGroup}
          onRenameGroup={onRenameGroup}
          onDeleteGroup={onDeleteGroup}
          onCreateIface={onCreateIface}
          onRenameIface={onRenameIface}
          onDeleteIface={onDeleteIface}
          onRunGroup={onRunGroup}
          onMoveIface={onMoveIface}
          onMoveGroup={onMoveGroup}
        />
      ))}
    </div>
  );
}

function Node({
  node,
  path,
  depth,
  activeId,
  onOpenIface,
  onCreateGroup,
  onRenameGroup,
  onDeleteGroup,
  onCreateIface,
  onRenameIface,
  onDeleteIface,
  onRunGroup,
  onMoveIface,
  onMoveGroup,
}: {
  node: TreeNode;
  path: string[];
  depth: number;
  activeId: string | null;
  onOpenIface: (ref: IfaceRef) => void;
  onCreateGroup: (parentPath: string[]) => void;
  onRenameGroup: (ref: IfaceRef) => void;
  onDeleteGroup: (ref: IfaceRef) => void;
  onCreateIface: (groupPath: string[]) => void;
  onRenameIface: (ref: IfaceRef) => void;
  onDeleteIface: (ref: IfaceRef) => void;
  onRunGroup?: (groupPath: string[]) => void;
  onMoveIface?: (ref: IfaceRef) => void;
  onMoveGroup?: (ref: IfaceRef) => void;
}) {
  const [expanded, setExpanded] = useState(true);

  if (node.type === "interface") {
    const id = [...path, node.key].join("/");
    return (
      <div
        className={cn(
          "group flex items-center gap-1 rounded-md py-1 pl-[7px] pr-1 text-sm cursor-pointer",
          activeId === id
            ? "bg-accent text-accent-foreground"
            : "text-muted-foreground hover:bg-muted hover:text-foreground",
        )}
        style={{ paddingLeft: depth * 14 + 7 }}
        onClick={() => onOpenIface({ groupPath: path, key: node.key })}
      >
        <span className={cn("w-10 shrink-0 truncate text-[10px]", node.method.startsWith("G") ? "text-green-500" : "text-orange-400")}>
          {node.method}
        </span>
        <span className="min-w-0 truncate">{node.name}</span>
        <span className="ml-auto flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100">
          <button
            className="rounded p-0.5 hover:bg-border cursor-pointer"
            title="重命名接口"
            onClick={(e) => {
              e.stopPropagation();
              onRenameIface({ groupPath: path, key: node.key });
            }}
          >
            <Pencil className="h-3 w-3" />
          </button>
          {onMoveIface && (
            <button
              className="rounded p-0.5 hover:bg-border cursor-pointer"
              title="移动到其它分组"
              onClick={(e) => {
                e.stopPropagation();
                onMoveIface({ groupPath: path, key: node.key });
              }}
            >
              <MoveRight className="h-3 w-3" />
            </button>
          )}
          <button
            className="rounded p-0.5 hover:bg-border cursor-pointer"
            title="删除接口"
            onClick={(e) => {
              e.stopPropagation();
              onDeleteIface({ groupPath: path, key: node.key });
            }}
          >
            <Trash2 className="h-3 w-3" />
          </button>
        </span>
      </div>
    );
  }

  return (
    <div>
      <div
        className="group flex items-center gap-1 rounded-md py-1 pr-1 text-sm cursor-pointer text-muted-foreground hover:bg-muted hover:text-foreground"
        style={{ paddingLeft: depth * 14 + 2 }}
        onClick={() => setExpanded((e) => !e)}
        title={node.key}
      >
        <span className="w-3 shrink-0">
          {expanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
        </span>
        <Folder className="h-3.5 w-3.5 shrink-0 text-yellow-500/80" />
        <span className="min-w-0 truncate">{node.name}</span>
        <span className="ml-auto flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100">
          {onRunGroup && (
            <button
              className="rounded p-0.5 hover:bg-border cursor-pointer"
              title="运行本分组全部接口"
              onClick={(e) => {
                e.stopPropagation();
                onRunGroup([...path, node.key]);
              }}
            >
              <Play className="h-3 w-3" />
            </button>
          )}
          <button
            className="rounded p-0.5 hover:bg-border cursor-pointer"
            title="新建分组"
            onClick={(e) => {
              e.stopPropagation();
              onCreateGroup([...path, node.key]);
            }}
          >
            <FolderPlus className="h-3 w-3" />
          </button>
          <button
            className="rounded p-0.5 hover:bg-border cursor-pointer"
            title="新建接口"
            onClick={(e) => {
              e.stopPropagation();
              onCreateIface([...path, node.key]);
            }}
          >
            <FilePlus2 className="h-3 w-3" />
          </button>
          <button
            className="rounded p-0.5 hover:bg-border cursor-pointer"
            title="重命名分组"
            onClick={(e) => {
              e.stopPropagation();
              onRenameGroup({ groupPath: [...path, node.key], key: node.key });
            }}
          >
            <Pencil className="h-3 w-3" />
          </button>
          {onMoveGroup && (
            <button
              className="rounded p-0.5 hover:bg-border cursor-pointer"
              title="移动到其它分组"
              onClick={(e) => {
                e.stopPropagation();
                onMoveGroup({ groupPath: [...path, node.key], key: node.key });
              }}
            >
              <MoveRight className="h-3 w-3" />
            </button>
          )}
          <button
            className="rounded p-0.5 hover:bg-border hover:text-red-400 cursor-pointer"
            title="删除分组"
            onClick={(e) => {
              e.stopPropagation();
              onDeleteGroup({ groupPath: [...path, node.key], key: node.key });
            }}
          >
            <Trash2 className="h-3 w-3" />
          </button>
        </span>
      </div>
      {expanded && (
        <div className="ml-2 border-l border-border">
          {node.children.map((child) => (
            <Node
              key={child.type === "group" ? child.key : child.key}
              node={child}
              path={[...path, node.key]}
              depth={depth + 1}
              activeId={activeId}
              onOpenIface={onOpenIface}
              onCreateGroup={onCreateGroup}
              onRenameGroup={onRenameGroup}
              onDeleteGroup={onDeleteGroup}
              onCreateIface={onCreateIface}
              onRenameIface={onRenameIface}
              onDeleteIface={onDeleteIface}
              onRunGroup={onRunGroup}
              onMoveIface={onMoveIface}
              onMoveGroup={onMoveGroup}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function PromptDialog({
  open,
  title,
  nameLabel,
  description,
  mode = "create",
  extraName,
  onClose,
  onSubmit,
  confirmText,
}: {
  open: boolean;
  title: string;
  nameLabel: string;
  description?: boolean;
  mode?: "create" | "rename";
  extraName?: string;
  onClose: () => void;
  onSubmit: (a: string, b: string) => Promise<void>;
  confirmText?: string;
}) {
  const [name, setName] = useState(extraName ?? "");
  const [desc, setDesc] = useState("");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (!name.trim()) {
      setErr("请填写名称");
      return;
    }
    setBusy(true);
    try {
      await onSubmit(name.trim(), mode === "rename" ? "" : desc.trim());
      onClose();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onClose={onClose} title={title}>
      <div className="space-y-3">
        <div>
          <label className="mb-1 block text-xs text-muted-foreground">{nameLabel}</label>
          <Input
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </div>
        {mode === "create" ? (
          description && (
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">描述（可选）</label>
              <textarea
                className="h-16 w-full resize-y rounded-md border border-border bg-muted p-2 text-sm text-foreground placeholder:text-muted-foreground outline-none focus:border-ring"
                value={desc}
                onChange={(e) => setDesc(e.target.value)}
              />
            </div>
          )
        ) : (
          <p className="text-[11px] text-muted-foreground">目录名将直接使用此名称，禁止包含 \ / : * ? " &lt; &gt; | 等特殊字符。</p>
        )}
        {err && <p className="text-xs text-red-400">{err}</p>}
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={busy}>取消</Button>
          <Button onClick={submit} disabled={busy}>{confirmText ?? "确定"}</Button>
        </DialogFooter>
      </div>
    </Dialog>
  );
}