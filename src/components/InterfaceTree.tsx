import { useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronDown, ChevronRight, FolderPlus, FilePlus2, Pencil, Trash2, Folder, MoveRight, MoreVertical, Download, Copy } from "lucide-react";
import type { TreeNode } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogFooter } from "@/components/ui/dialog";

export interface IfaceRef {
  groupPath: string[];
  key: string;
}

/** 正在拖拽的对象：接口或分组（分组可排序、可嵌套） */
type DragPayload =
  | { kind: "iface"; ref: IfaceRef }
  | { kind: "group"; path: string[] };

/** 拖拽落点：接口行上/下半、分组行（前/后同层插入、拖入组内）、某层的分组层末尾、根目录末尾 */
type DropTarget =
  | { kind: "iface"; path: string[]; key: string; after: boolean }
  /** 拖入分组（放到该分组末尾） */
  | { kind: "into-group"; path: string[] }
  /** 分组同层插入（前/后），path 为目标分组的完整路径 */
  | { kind: "group-pos"; path: string[]; after: boolean }
  /** 接口行上的分组落点：插到该层分组层的末尾 */
  | { kind: "level"; path: string[]; key: string }
  | { kind: "root" };

/** 拖拽期间下发给各行的上下文（避免逐层透传多个回调） */
interface DndCtx {
  /** 是否支持拖拽（外部未提供落子回调时关闭） */
  enabled: boolean;
  /** 是否正在拖拽（未拖拽时不响应 dragover/drop） */
  active: boolean;
  /** 正在拖拽的对象类型（分组行只在拖拽分组时提供同层插入区） */
  payloadKind: "iface" | "group" | null;
  /** 正在拖拽的行 id（用于淡化源行） */
  draggingId: string | null;
  /** 某层第一个接口的键（分组拖到接口行上时，插入线画在这一行） */
  firstIfaceKey: (path: string[]) => string | null;
  /** 取当前拖拽对象（事件里读取，不参与渲染） */
  getPayload: () => DragPayload | null;
  target: DropTarget | null;
  setTarget: (t: DropTarget | null) => void;
  begin: (payload: DragPayload, id: string) => void;
  end: () => void;
  drop: (t: DropTarget) => void;
}

const pathKey = (p: string[]) => p.join("/");

/** `path` 是否就是 `ancestor` 或在其子树内（用于阻断拖进自己） */
function isUnder(path: string[], ancestor: string[]): boolean {
  return path.length >= ancestor.length && pathKey(path.slice(0, ancestor.length)) === pathKey(ancestor);
}

/** 落点是否等价（dragover 高频触发，等值时返回原对象以避开重渲染） */
function sameTarget(a: DropTarget | null, b: DropTarget | null): boolean {
  if (a === b) return true;
  if (!a || !b || a.kind !== b.kind) return false;
  if (a.kind === "root") return true;
  if (a.kind === "iface" && b.kind === "iface") {
    return pathKey(a.path) === pathKey(b.path) && a.key === b.key && a.after === b.after;
  }
  if (a.kind === "into-group" && b.kind === "into-group") return pathKey(a.path) === pathKey(b.path);
  if (a.kind === "group-pos" && b.kind === "group-pos") {
    return pathKey(a.path) === pathKey(b.path) && a.after === b.after;
  }
  if (a.kind === "level" && b.kind === "level") {
    return pathKey(a.path) === pathKey(b.path) && a.key === b.key;
  }
  return false;
}

/** 插入位置指示线：左侧小圆点 + 一条细线 */
export function DropLine({ pos }: { pos: "before" | "after" }) {
  return (
    <span
      className={cn(
        "pointer-events-none absolute left-0 right-0 z-20 flex items-center",
        pos === "before" ? "-top-[3px]" : "-bottom-[3px]",
      )}
    >
      <span className="h-2 w-2 shrink-0 rounded-full border-2 border-sky-400 bg-background" />
      <span className="h-0.5 flex-1 rounded-full bg-sky-400" />
    </span>
  );
}

/** 取某分组路径下的直接子节点（path 为空 = 根层） */
function levelAt(tree: TreeNode[], path: string[]): TreeNode[] {
  if (path.length === 0) return tree;
  for (const n of tree) {
    if (n.type === "group" && n.key === path[0]) return levelAt(n.children, path.slice(1));
  }
  return [];
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
  onMoveIface,
  onMoveGroup,
  onExportIface,
  onCopyIface,
  onDropIface,
  onDropGroup,
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
  onMoveIface?: (ref: IfaceRef) => void;
  onMoveGroup?: (ref: IfaceRef) => void;
  onExportIface?: (ref: IfaceRef) => void;
  onCopyIface?: (ref: IfaceRef) => void;
  /** 拖拽落下：把接口移到 `targetGroupPath` 并插到 `beforeKey` 之前（null = 末尾） */
  onDropIface?: (ref: IfaceRef, targetGroupPath: string[], beforeKey: string | null) => void;
  /** 拖拽落下：把分组移到 `targetParentPath` 并插到 `beforeKey` 之前（null = 同层末尾） */
  onDropGroup?: (groupPath: string[], targetParentPath: string[], beforeKey: string | null) => void;
}) {
  const dragItem = useRef<DragPayload | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const [target, setTarget] = useState<DropTarget | null>(null);
  const canDrag = !!onDropIface || !!onDropGroup;

  /** 接口落点 → 后端的「目标分组 + 插到谁之前」 */
  const resolveIface = (t: DropTarget): { groupPath: string[]; beforeKey: string | null } | null => {
    if (t.kind === "root") return { groupPath: [], beforeKey: null };
    if (t.kind === "into-group") return { groupPath: t.path, beforeKey: null }; // 落分组上 = 放进该组末尾
    if (t.kind !== "iface") return null;
    const level = levelAt(tree, t.path);
    const idx = level.findIndex((n) => n.type === "interface" && n.key === t.key);
    if (idx < 0) return null;
    if (!t.after) return { groupPath: t.path, beforeKey: t.key };
    // 落在下半部 = 插到该行之后（即其后第一个接口之前；没有则末尾）
    const next = level.slice(idx + 1).find((n) => n.type === "interface");
    return { groupPath: t.path, beforeKey: next && next.type === "interface" ? next.key : null };
  };

  /** 分组落点 → 后端的「目标父分组 + 插到哪个同层分组之前」 */
  const resolveGroup = (t: DropTarget): { parentPath: string[]; beforeKey: string | null } | null => {
    if (t.kind === "root") return { parentPath: [], beforeKey: null };
    // 分组恒定排在接口之前：落在接口行上 = 排到该层分组层的末尾
    if (t.kind === "level" || t.kind === "into-group") {
      return { parentPath: t.path, beforeKey: null };
    }
    if (t.kind !== "group-pos" || t.path.length === 0) return null;
    const parentPath = t.path.slice(0, -1);
    const key = t.path[t.path.length - 1];
    if (!t.after) return { parentPath, beforeKey: key };
    const level = levelAt(tree, parentPath);
    const idx = level.findIndex((n) => n.type === "group" && n.key === key);
    if (idx < 0) return { parentPath, beforeKey: null };
    const next = level.slice(idx + 1).find((n) => n.type === "group");
    return { parentPath, beforeKey: next && next.type === "group" ? next.key : null };
  };

  const drop = (t: DropTarget) => {
    const payload = dragItem.current;
    dragItem.current = null;
    setDragging(null);
    setTarget(null);
    if (!payload) return;
    if (payload.kind === "iface") {
      if (!onDropIface) return;
      const to = resolveIface(t);
      if (!to) return;
      // 落在自己身上：位置不变，不必打扰后端
      if (pathKey(payload.ref.groupPath) === pathKey(to.groupPath) && payload.ref.key === to.beforeKey) return;
      onDropIface(payload.ref, to.groupPath, to.beforeKey);
      return;
    }
    if (!onDropGroup) return;
    const to = resolveGroup(t);
    if (!to || isUnder(to.parentPath, payload.path)) return; // 不能拖进自己的子树
    const parent = payload.path.slice(0, -1);
    if (pathKey(parent) === pathKey(to.parentPath) && payload.path[payload.path.length - 1] === to.beforeKey) return;
    onDropGroup(payload.path, to.parentPath, to.beforeKey);
  };

  const dnd: DndCtx = {
    enabled: canDrag,
    active: canDrag && dragging !== null,
    payloadKind: dragItem.current?.kind ?? null,
    draggingId: dragging,
    firstIfaceKey: (path) => {
      const node = levelAt(tree, path).find((n) => n.type === "interface");
      return node && node.type === "interface" ? node.key : null;
    },
    getPayload: () => dragItem.current,
    target,
    setTarget: (t) => setTarget((prev) => (sameTarget(prev, t) ? prev : t)),
    begin: (payload, id) => {
      dragItem.current = payload;
      setDragging(id);
    },
    end: () => {
      dragItem.current = null;
      setDragging(null);
      setTarget(null);
    },
    drop,
  };

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
          dnd={dnd}
          onOpenIface={onOpenIface}
          onCreateGroup={onCreateGroup}
          onRenameGroup={onRenameGroup}
          onDeleteGroup={onDeleteGroup}
          onCreateIface={onCreateIface}
          onRenameIface={onRenameIface}
          onDeleteIface={onDeleteIface}
          onMoveIface={onMoveIface}
          onMoveGroup={onMoveGroup}
          onExportIface={onExportIface}
          onCopyIface={onCopyIface}
        />
      ))}
      {dnd.active && (
        <div
          className={`mt-1 rounded-md border border-dashed px-2 py-1.5 text-[11px] transition-colors ${
            target?.kind === "root"
              ? "border-accent bg-accent/10 text-accent"
              : "border-border text-muted-foreground"
          }`}
          onDragOver={(e) => {
            e.preventDefault();
            e.stopPropagation();
            e.dataTransfer.dropEffect = "move";
            dnd.setTarget({ kind: "root" });
          }}
          onDragLeave={(e) => {
            if ((e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) return;
            dnd.setTarget(null);
          }}
          onDrop={(e) => {
            e.preventDefault();
            e.stopPropagation();
            dnd.drop({ kind: "root" });
          }}
        >
          拖到这里 → 移到根目录末尾
        </div>
      )}
    </div>
  );
}

function Node({
  node,
  path,
  depth,
  activeId,
  dnd,
  onOpenIface,
  onCreateGroup,
  onRenameGroup,
  onDeleteGroup,
  onCreateIface,
  onRenameIface,
  onDeleteIface,
  onMoveIface,
  onMoveGroup,
  onExportIface,
  onCopyIface,
}: {
  node: TreeNode;
  path: string[];
  depth: number;
  activeId: string | null;
  dnd: DndCtx;
  onOpenIface: (ref: IfaceRef) => void;
  onCreateGroup: (parentPath: string[]) => void;
  onRenameGroup: (ref: IfaceRef) => void;
  onDeleteGroup: (ref: IfaceRef) => void;
  onCreateIface: (groupPath: string[]) => void;
  onRenameIface: (ref: IfaceRef) => void;
  onDeleteIface: (ref: IfaceRef) => void;
  onMoveIface?: (ref: IfaceRef) => void;
  onMoveGroup?: (ref: IfaceRef) => void;
  onExportIface?: (ref: IfaceRef) => void;
  onCopyIface?: (ref: IfaceRef) => void;
}) {
  // 接口树默认不展开：根分组/子分组都收起，由用户按需展开
  const [expanded, setExpanded] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const rowRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (rowRef.current && !rowRef.current.contains(e.target as Node)) setMenuOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [menuOpen]);

  if (node.type === "interface") {
    const id = [...path, node.key].join("/");
    const ref = { groupPath: path, key: node.key };
    const marker =
      dnd.target?.kind === "iface" && dnd.target.path.join("/") === path.join("/") && dnd.target.key === node.key
        ? dnd.target.after
          ? "after"
          : "before"
        : dnd.target?.kind === "level" &&
            dnd.target.path.join("/") === path.join("/") &&
            dnd.firstIfaceKey(path) === node.key
          ? "before"
          : null;
    /** 落在上半部 = 插到本行之前，下半部 = 之后 */
    const dropAfter = (e: React.DragEvent) => {
      const rect = rowRef.current?.getBoundingClientRect();
      return rect ? e.clientY > rect.top + rect.height / 2 : false;
    };
    /** 拖拽分组时本行所在层是否是合法落点（不能拖进自己子树） */
    const groupTargetOk = () => {
      const p = dnd.getPayload();
      return !p || p.kind !== "group" || !isUnder(path, p.path);
    };
    return (
      <div
        ref={rowRef}
        draggable={dnd.enabled}
        className={cn(
          "group relative flex items-center gap-1 rounded-md py-1 pl-[7px] pr-1 text-sm cursor-pointer",
          activeId === id
            ? "bg-accent text-accent-foreground"
            : "text-muted-foreground hover:bg-muted hover:text-foreground",
          dnd.enabled && "cursor-grab",
          dnd.draggingId === id && "opacity-40",
        )}
        style={{ paddingLeft: depth * 14 + 7 }}
        onClick={() => onOpenIface(ref)}
        onDragStart={(e) => {
          e.stopPropagation();
          e.dataTransfer.effectAllowed = "move";
          e.dataTransfer.setData("text/plain", node.key);
          dnd.begin({ kind: "iface", ref }, id);
        }}
        onDragEnd={() => dnd.end()}
        onDragOver={(e) => {
          if (!dnd.active || !groupTargetOk()) return;
          e.preventDefault();
          e.stopPropagation();
          e.dataTransfer.dropEffect = "move";
          // 拖拽分组时只能落到本行所在层的分组层末尾
          dnd.setTarget(
            dnd.payloadKind === "group"
              ? { kind: "level", path, key: node.key }
              : { kind: "iface", path, key: node.key, after: dropAfter(e) },
          );
        }}
        onDragLeave={(e) => {
          if (rowRef.current?.contains(e.relatedTarget as Node)) return;
          if (
            dnd.target?.kind === "iface" && dnd.target.key === node.key && dnd.target.path.join("/") === path.join("/")
          ) {
            dnd.setTarget(null);
          }
          if (dnd.target?.kind === "level" && dnd.target.key === node.key && dnd.target.path.join("/") === path.join("/")) {
            dnd.setTarget(null);
          }
        }}
        onDrop={(e) => {
          e.preventDefault();
          e.stopPropagation();
          dnd.drop(
            dnd.payloadKind === "group"
              ? { kind: "level", path, key: node.key }
              : { kind: "iface", path, key: node.key, after: dropAfter(e) },
          );
        }}
      >
        {marker && <DropLine pos={marker} />}
        <span className={cn("w-10 shrink-0 truncate text-[10px]", node.method.startsWith("G") ? "text-green-500" : "text-orange-400")}>
          {node.method}
        </span>
        <span className="min-w-0 truncate">{node.name}</span>
        <span className="ml-auto flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100">
          <button
            className={cn("rounded p-0.5 hover:bg-border cursor-pointer", menuOpen && "opacity-100")}
            title="更多操作（编辑 / 移动 / 删除 / 导出）"
            onClick={(e) => {
              e.stopPropagation();
              setMenuOpen((v) => !v);
            }}
          >
            <MoreVertical className="h-3 w-3" />
          </button>
        </span>
        {menuOpen && (
          <div className="absolute right-1 top-7 z-50 w-40 overflow-hidden rounded-md border border-border bg-muted shadow-xl">
            <MenuItem icon={<Pencil className="h-3.5 w-3.5" />} label="编辑" onClick={() => { setMenuOpen(false); onRenameIface(ref); }} />
            {onCopyIface && (
              <MenuItem icon={<Copy className="h-3.5 w-3.5" />} label="复制" onClick={() => { setMenuOpen(false); onCopyIface(ref); }} />
            )}
            {onMoveIface && (
              <MenuItem icon={<MoveRight className="h-3.5 w-3.5" />} label="移动" onClick={() => { setMenuOpen(false); onMoveIface(ref); }} />
            )}
            {onExportIface && (
              <MenuItem icon={<Download className="h-3.5 w-3.5" />} label="导出（OpenAPI）" onClick={() => { setMenuOpen(false); onExportIface(ref); }} />
            )}
            <div className="my-1 h-px bg-border" />
            <MenuItem danger icon={<Trash2 className="h-3.5 w-3.5" />} label="删除" onClick={() => { setMenuOpen(false); onDeleteIface(ref); }} />
          </div>
        )}
      </div>
    );
  }

  const groupPath = [...path, node.key];
  const groupId = `group:${pathKey(groupPath)}`;
  const hovering =
    dnd.target?.kind === "into-group" && dnd.target.path.join("/") === groupPath.join("/");
  const groupMarker =
    dnd.target?.kind === "group-pos" && dnd.target.path.join("/") === groupPath.join("/")
      ? dnd.target.after
        ? "after"
        : "before"
      : null;
  /** 本分组是否是非法落点（正在拖拽自己或自己的后代） */
  const blocked = () => {
    const p = dnd.getPayload();
    return p?.kind === "group" && isUnder(groupPath, p.path); // 含自己
  };
  /** 上/下 30% = 同层前/后插入，中间 = 拖入组内（仅拖拽分组时有前/后区） */
  const groupZone = (e: React.DragEvent): "before" | "after" | "into" => {
    const rect = rowRef.current?.getBoundingClientRect();
    if (!rect) return "into";
    const frac = (e.clientY - rect.top) / rect.height;
    return frac < 0.3 ? "before" : frac > 0.7 ? "after" : "into";
  };
  return (
    <div>
      <div
        ref={rowRef}
        draggable={dnd.enabled}
        className={cn(
          "group relative flex items-center gap-1 rounded-md py-1 pr-1 text-sm cursor-pointer text-muted-foreground hover:bg-muted hover:text-foreground",
          hovering && "ring-1 ring-accent bg-accent/10 text-foreground",
          dnd.enabled && "cursor-grab",
          dnd.draggingId === groupId && "opacity-40",
        )}
        style={{ paddingLeft: depth * 14 + 2 }}
        onClick={() => setExpanded((e) => !e)}
        title={`${node.key}${dnd.enabled ? "（可拖拽调整分组位置，或把接口拖入该分组）" : ""}`}
        onDragStart={(e) => {
          e.stopPropagation();
          e.dataTransfer.effectAllowed = "move";
          e.dataTransfer.setData("text/plain", node.key);
          dnd.begin({ kind: "group", path: groupPath }, groupId);
        }}
        onDragEnd={() => dnd.end()}
        onDragOver={(e) => {
          if (!dnd.active || blocked()) return;
          e.preventDefault();
          e.stopPropagation();
          e.dataTransfer.dropEffect = "move";
          if (dnd.payloadKind === "group") {
            const zone = groupZone(e);
            if (zone === "into") {
              setExpanded(true);
              dnd.setTarget({ kind: "into-group", path: groupPath });
            } else {
              dnd.setTarget({ kind: "group-pos", path: groupPath, after: zone === "after" });
            }
          } else {
            // 拖入该分组：展开以便看到落点
            setExpanded(true);
            dnd.setTarget({ kind: "into-group", path: groupPath });
          }
        }}
        onDragLeave={(e) => {
          if (rowRef.current?.contains(e.relatedTarget as Node)) return;
          const t = dnd.target;
          if (!t) return;
          if (
            (t.kind === "into-group" || t.kind === "group-pos") &&
            t.path.join("/") === groupPath.join("/")
          ) {
            dnd.setTarget(null);
          }
        }}
        onDrop={(e) => {
          e.preventDefault();
          e.stopPropagation();
          if (dnd.payloadKind === "group") {
            const zone = groupZone(e);
            dnd.drop(
              zone === "into"
                ? { kind: "into-group", path: groupPath }
                : { kind: "group-pos", path: groupPath, after: zone === "after" },
            );
          } else {
            dnd.drop({ kind: "into-group", path: groupPath });
          }
        }}
      >
        {groupMarker && <DropLine pos={groupMarker} />}
        <span className="w-3 shrink-0">
          {expanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
        </span>
        <Folder className="h-3.5 w-3.5 shrink-0 text-yellow-500/80" />
        <span className="min-w-0 truncate">{node.name}</span>
        <span className="ml-auto flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100">
          <button
            className={cn("rounded p-0.5 hover:bg-border cursor-pointer", menuOpen && "opacity-100")}
            title="更多操作（新建 / 运行 / 编辑 / 移动 / 删除）"
            onClick={(e) => {
              e.stopPropagation();
              setMenuOpen((v) => !v);
            }}
          >
            <MoreVertical className="h-3 w-3" />
          </button>
        </span>
        {menuOpen && (
          <div className="absolute right-1 top-7 z-50 w-40 overflow-hidden rounded-md border border-border bg-muted shadow-xl">
            <MenuItem icon={<FilePlus2 className="h-3.5 w-3.5" />} label="新建接口" onClick={() => { setMenuOpen(false); onCreateIface([...path, node.key]); }} />
            <MenuItem icon={<FolderPlus className="h-3.5 w-3.5" />} label="新建分组" onClick={() => { setMenuOpen(false); onCreateGroup([...path, node.key]); }} />
            <MenuItem icon={<Pencil className="h-3.5 w-3.5" />} label="编辑" onClick={() => { setMenuOpen(false); onRenameGroup({ groupPath: [...path, node.key], key: node.key }); }} />
            {onMoveGroup && (
              <MenuItem icon={<MoveRight className="h-3.5 w-3.5" />} label="移动" onClick={() => { setMenuOpen(false); onMoveGroup({ groupPath: [...path, node.key], key: node.key }); }} />
            )}
            <div className="my-1 h-px bg-border" />
            <MenuItem danger icon={<Trash2 className="h-3.5 w-3.5" />} label="删除" onClick={() => { setMenuOpen(false); onDeleteGroup({ groupPath: [...path, node.key], key: node.key }); }} />
          </div>
        )}
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
              dnd={dnd}
              onOpenIface={onOpenIface}
              onCreateGroup={onCreateGroup}
              onRenameGroup={onRenameGroup}
              onDeleteGroup={onDeleteGroup}
              onCreateIface={onCreateIface}
              onRenameIface={onRenameIface}
              onDeleteIface={onDeleteIface}
              onMoveIface={onMoveIface}
              onMoveGroup={onMoveGroup}
              onExportIface={onExportIface}
              onCopyIface={onCopyIface}
            />
          ))}
        </div>
      )}
    </div>
  );
}

/** 下拉菜单项（接口树 / 快捷请求共用） */
export function MenuItem({
  icon,
  label,
  danger,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  danger?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={`flex w-full cursor-pointer select-none items-center gap-2 px-3 py-1.5 text-sm transition-colors hover:bg-border ${
        danger ? "text-red-400" : "text-foreground"
      }`}
      // 菜单渲染在可点击行内部，必须阻断冒泡：否则选菜单项会连带触发行本身的展开/折叠
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
    >
      {icon} {label}
    </button>
  );
}

export function PromptDialog({
  open,
  title,
  nameLabel,
  description,
  mode = "create",
  extraName,
  renameHint,
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
  /** 重命名时的提示文案（默认针对目录名/文件名的限制说明） */
  renameHint?: string;
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
          <p className="text-[11px] text-muted-foreground">
            {renameHint ?? '目录名将直接使用此名称，禁止包含 \\ / : * ? " < > | 等特殊字符。'}
          </p>
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