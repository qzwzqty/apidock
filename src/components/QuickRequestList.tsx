import { useEffect, useRef, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Copy,
  Folder,
  FolderPlus,
  MoreVertical,
  MoveRight,
  Pencil,
  Trash2,
  Zap,
} from "lucide-react";
import type { QuickGroupSummary, QuickRequestSummary, QuickTree } from "@/lib/api";
import { cn } from "@/lib/utils";
import { DropLine, MenuItem } from "@/components/InterfaceTree";

/** 分组内的直接内容（供递归渲染） */
interface Level {
  groups: QuickGroupSummary[];
  requests: QuickRequestSummary[];
}

/** 正在拖拽的对象：快捷请求或分组（分组可排序、可嵌套） */
type DragPayload =
  | { kind: "request"; item: QuickRequestSummary }
  | { kind: "group"; group: QuickGroupSummary };

/** 拖拽落点：请求行上/下半、分组行（前/后同层插入、拖入组内）、请求行上的分组落点、根层末尾 */
type DropTarget =
  | { kind: "request"; groupId: number | null; id: number; after: boolean }
  | { kind: "into-group"; id: number }
  /** 分组同层插入（前/后） */
  | { kind: "group-pos"; id: number; parentId: number | null; after: boolean }
  /** 请求行上的分组落点：插到该层分组层的末尾 */
  | { kind: "level"; parentId: number | null }
  | { kind: "root" };

/** 拖拽上下文 */
interface QuickDnd {
  /** 是否支持拖拽（外部未提供落子回调时关闭） */
  enabled: boolean;
  /** 是否正在拖拽（未拖拽时不响应 dragover/drop） */
  active: boolean;
  /** 正在拖拽的对象类型（分组行只在拖拽分组时提供同层插入区） */
  payloadKind: "request" | "group" | null;
  /** 正在拖拽的行 id（用于淡化源行） */
  draggingId: string | null;
  /** 某层第一个快捷请求的 id（分组拖到请求行上时，插入线画在这一行） */
  firstRequestId: (parentId: number | null) => number | null;
  /** 取当前拖拽对象（事件里读取，不参与渲染） */
  getPayload: () => DragPayload | null;
  target: DropTarget | null;
  setTarget: (t: DropTarget | null) => void;
  begin: (payload: DragPayload, id: string) => void;
  end: () => void;
  drop: (t: DropTarget) => void;
}

/** 落点是否等价（dragover 高频触发，等值时返回原对象以避开重渲染） */
function sameTarget(a: DropTarget | null, b: DropTarget | null): boolean {
  if (a === b) return true;
  if (!a || !b || a.kind !== b.kind) return false;
  if (a.kind === "root") return true;
  if (a.kind === "into-group" && b.kind === "into-group") return a.id === b.id;
  if (a.kind === "level" && b.kind === "level") return a.parentId === b.parentId;
  if (a.kind === "group-pos" && b.kind === "group-pos") {
    return a.id === b.id && a.parentId === b.parentId && a.after === b.after;
  }
  if (a.kind === "request" && b.kind === "request") {
    return a.groupId === b.groupId && a.id === b.id && a.after === b.after;
  }
  return false;
}

/** 行操作（逐层透传给分组节点与请求行） */
interface NodeActions {
  onOpen: (id: number) => void;
  onCreateRequest: (groupId: number | null) => void;
  onCreateGroup: (parentId: number | null, name: string) => void;
  onRenameRequest: (item: QuickRequestSummary) => void;
  onCopyRequest: (item: QuickRequestSummary) => void;
  onMoveRequest: (item: QuickRequestSummary) => void;
  onDeleteRequest: (item: QuickRequestSummary) => void;
  onRenameGroup: (group: QuickGroupSummary) => void;
  onDeleteGroup: (group: QuickGroupSummary) => void;
  /** 分组 id -> 父分组 id（用于环检测） */
  parentOf: Map<number, number | null>;
  dnd: QuickDnd;
}

/** 按 parentId 把平铺数据归档到各层 */
function buildLevels(tree: QuickTree): Map<number | null, Level> {
  const levels = new Map<number | null, Level>();
  const at = (key: number | null) => {
    let level = levels.get(key);
    if (!level) {
      level = { groups: [], requests: [] };
      levels.set(key, level);
    }
    return level;
  };
  for (const g of tree.groups) at(g.parentId).groups.push(g);
  for (const r of tree.requests) at(r.groupId).requests.push(r);
  return levels;
}

/**
 * 接口树下方的「快捷请求」区块：与环境无关的临时接口（URL 为完整地址）。
 * 支持分组（可嵌套、可重名、无唯一键）；请求行的操作与接口行一致（编辑/复制/移动/删除，无导出）。
 */
export function QuickRequestList({
  tree,
  activeId,
  onOpen,
  onCreateRequest,
  onCreateGroup,
  onRenameRequest,
  onCopyRequest,
  onMoveRequest,
  onDeleteRequest,
  onRenameGroup,
  onDeleteGroup,
  onDropRequest,
  onDropGroup,
}: {
  tree: QuickTree;
  /** 当前激活的标签页 id（快捷请求为 `quick:<id>`），用于高亮 */
  activeId: string | null;
  onOpen: (id: number) => void;
  onCreateRequest: (groupId: number | null) => void;
  onCreateGroup: (parentId: number | null, name: string) => void;
  onRenameRequest: (item: QuickRequestSummary) => void;
  onCopyRequest: (item: QuickRequestSummary) => void;
  onMoveRequest: (item: QuickRequestSummary) => void;
  onDeleteRequest: (item: QuickRequestSummary) => void;
  onRenameGroup: (group: QuickGroupSummary) => void;
  onDeleteGroup: (group: QuickGroupSummary) => void;
  /** 拖拽落下：把请求移到 `targetGroupId` 并插到 `beforeId` 之前（null = 末尾） */
  onDropRequest?: (item: QuickRequestSummary, targetGroupId: number | null, beforeId: number | null) => void;
  /** 拖拽落下：把分组移到 `parentId` 下并插到 `beforeId` 之前（null = 同层末尾） */
  onDropGroup?: (group: QuickGroupSummary, parentId: number | null, beforeId: number | null) => void;
}) {
  const [expanded, setExpanded] = useState(true);
  const [menuOpen, setMenuOpen] = useState(false);
  const headerRef = useRef<HTMLDivElement>(null);
  const dragItem = useRef<DragPayload | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const [target, setTarget] = useState<DropTarget | null>(null);
  const levels = buildLevels(tree);
  const canDrag = !!onDropRequest || !!onDropGroup;

  /** 分组 id -> 父分组 id（用于环检测） */
  const parentOf = new Map<number, number | null>(tree.groups.map((g) => [g.id, g.parentId]));
  /** 目标层是否是正在拖拽分组的自己或其后代 */
  const blockedParent = (parentId: number | null, groupId: number) => {
    let cur = parentId;
    while (cur !== null) {
      if (cur === groupId) return true;
      cur = parentOf.get(cur) ?? null;
    }
    return false;
  };

  /** 请求落点 → 后端的「目标分组 + 插到谁之前」 */
  const resolveRequest = (t: DropTarget): { groupId: number | null; beforeId: number | null } | null => {
    if (t.kind === "root") return { groupId: null, beforeId: null };
    if (t.kind === "into-group") return { groupId: t.id, beforeId: null }; // 落分组上 = 放进该组末尾
    if (t.kind !== "request") return null;
    const list = levels.get(t.groupId)?.requests ?? [];
    const idx = list.findIndex((r) => r.id === t.id);
    if (idx < 0) return null;
    if (!t.after) return { groupId: t.groupId, beforeId: t.id };
    const next = list[idx + 1];
    return { groupId: t.groupId, beforeId: next ? next.id : null };
  };

  /** 分组落点 → 后端的「目标父分组 + 插到哪个同层分组之前」 */
  const resolveGroup = (t: DropTarget): { parentId: number | null; beforeId: number | null } | null => {
    if (t.kind === "root") return { parentId: null, beforeId: null };
    // 分组恒定排在请求之前：落在请求行上 = 排到该层分组层末尾
    if (t.kind === "level") return { parentId: t.parentId, beforeId: null };
    if (t.kind === "into-group") return { parentId: t.id, beforeId: null };
    if (t.kind !== "group-pos") return null;
    if (!t.after) return { parentId: t.parentId, beforeId: t.id };
    const list = levels.get(t.parentId)?.groups ?? [];
    const idx = list.findIndex((g) => g.id === t.id);
    const next = idx >= 0 ? list[idx + 1] : undefined;
    return { parentId: t.parentId, beforeId: next ? next.id : null };
  };

  const drop = (t: DropTarget) => {
    const payload = dragItem.current;
    dragItem.current = null;
    setDragging(null);
    setTarget(null);
    if (!payload) return;
    if (payload.kind === "request") {
      if (!onDropRequest) return;
      const to = resolveRequest(t);
      if (!to) return;
      // 落在自己身上：位置不变，不必打扰后端
      if (payload.item.groupId === to.groupId && payload.item.id === to.beforeId) return;
      onDropRequest(payload.item, to.groupId, to.beforeId);
      return;
    }
    if (!onDropGroup) return;
    const to = resolveGroup(t);
    if (!to || blockedParent(to.parentId, payload.group.id)) return; // 不能拖进自己的子树
    if (payload.group.parentId === to.parentId && payload.group.id === to.beforeId) return;
    onDropGroup(payload.group, to.parentId, to.beforeId);
  };

  const dnd: QuickDnd = {
    enabled: canDrag,
    active: canDrag && dragging !== null,
    payloadKind: dragItem.current?.kind ?? null,
    draggingId: dragging,
    firstRequestId: (parentId) => levels.get(parentId)?.requests[0]?.id ?? null,
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

  const actions: NodeActions = {
    onOpen,
    onCreateRequest,
    onCreateGroup,
    onRenameRequest,
    onCopyRequest,
    onMoveRequest,
    onDeleteRequest,
    onRenameGroup,
    onDeleteGroup,
    parentOf,
    dnd,
  };

  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (headerRef.current && !headerRef.current.contains(e.target as Node)) setMenuOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [menuOpen]);

  return (
    <div className="mt-2 border-t border-border pt-1">
      <div
        ref={headerRef}
        className="relative flex cursor-pointer items-center gap-1 rounded-md py-1 pl-2 pr-1 text-sm text-muted-foreground hover:bg-muted hover:text-foreground"
        onClick={() => setExpanded((v) => !v)}
        title="快捷请求：不继承环境变量，地址栏填写完整地址（分组与接口分组相互独立）"
      >
        <span className="w-3 shrink-0">
          {expanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
        </span>
        <Zap className="h-3.5 w-3.5 shrink-0 text-sky-400" />
        <span className="min-w-0 truncate">快捷请求</span>
        {tree.requests.length > 0 && (
          <span className="shrink-0 rounded bg-border/60 px-1 text-[10px]">{tree.requests.length}</span>
        )}
        <button
          className={cn("ml-auto shrink-0 rounded p-0.5 hover:bg-border cursor-pointer", menuOpen && "bg-border")}
          title="更多操作（新建快捷请求 / 新建分组）"
          onClick={(e) => {
            e.stopPropagation();
            setMenuOpen((v) => !v);
          }}
        >
          <MoreVertical className="h-3 w-3" />
        </button>
        {menuOpen && (
          <div className="absolute right-1 top-7 z-50 w-44 overflow-hidden rounded-md border border-border bg-muted shadow-xl">
            <MenuItem
              icon={<Zap className="h-3.5 w-3.5" />}
              label="新建快捷请求"
              onClick={() => {
                setMenuOpen(false);
                setExpanded(true);
                onCreateRequest(null);
              }}
            />
            <MenuItem
              icon={<FolderPlus className="h-3.5 w-3.5" />}
              label="新建分组"
              onClick={() => {
                setMenuOpen(false);
                setExpanded(true);
                onCreateGroup(null, "");
              }}
            />
          </div>
        )}
      </div>

      {expanded && (
        <div className="ml-2 border-l border-border pl-1">
          {tree.requests.length === 0 && tree.groups.length === 0 && (
            <p className="px-2 py-2 text-xs text-muted-foreground/80">
              暂无快捷请求：点标题右侧 ⋮ 新建快捷请求或分组
            </p>
          )}
          <Level
            groups={levels.get(null)?.groups ?? []}
            requests={levels.get(null)?.requests ?? []}
            levels={levels}
            depth={0}
            activeId={activeId}
            actions={actions}
          />
          {dnd.active && (
            <div
              className={cn(
                "mt-1 rounded-md border border-dashed px-2 py-1.5 text-[11px] transition-colors",
                target?.kind === "root"
                  ? "border-accent bg-accent/10 text-accent"
                  : "border-border text-muted-foreground",
              )}
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
              拖到这里 → 移到未分组末尾
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function Level({
  groups,
  requests,
  levels,
  depth,
  activeId,
  actions,
}: {
  groups: QuickGroupSummary[];
  requests: QuickRequestSummary[];
  levels: Map<number | null, Level>;
  depth: number;
  activeId: string | null;
  actions: NodeActions;
}) {
  return (
    <>
      {groups.map((group) => (
        <GroupNode
          key={`g-${group.id}`}
          group={group}
          levels={levels}
          depth={depth}
          activeId={activeId}
          actions={actions}
        />
      ))}
      {requests.map((item) => (
        <QuickRequestRow
          key={`q-${item.id}`}
          item={item}
          depth={depth}
          active={activeId === `quick:${item.id}`}
          actions={actions}
        />
      ))}
    </>
  );
}

/** 分组行：默认折叠；菜单提供新建快捷请求 / 新建子分组 / 重命名 / 删除 */
function GroupNode({
  group,
  levels,
  depth,
  activeId,
  actions,
}: {
  group: QuickGroupSummary;
  levels: Map<number | null, Level>;
  depth: number;
  activeId: string | null;
  actions: NodeActions;
}) {
  const [open, setOpen] = useState(false);
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

  const child = levels.get(group.id);
  const dnd = actions.dnd;
  const hovering = dnd.target?.kind === "into-group" && dnd.target.id === group.id;
  const marker =
    dnd.target?.kind === "group-pos" && dnd.target.id === group.id
      ? dnd.target.after
        ? "after"
        : "before"
      : null;
  const draggingId = `group:${group.id}`;
  /** 本分组是否是非法落点（正在拖拽自己或自己的后代） */
  const blocked = () => {
    const p = dnd.getPayload();
    if (p?.kind !== "group") return false;
    let cur: number | null = group.id;
    while (cur !== null) {
      if (cur === p.group.id) return true;
      cur = actions.parentOf.get(cur) ?? null;
    }
    return false;
  };
  /** 上/下 30% = 同层前/后插入，中间 = 拖入组内（仅拖拽分组时有前/后区） */
  const zone = (e: React.DragEvent): "before" | "after" | "into" => {
    const rect = rowRef.current?.getBoundingClientRect();
    if (!rect) return "into";
    const frac = (e.clientY - rect.top) / rect.height;
    return frac < 0.3 ? "before" : frac > 0.7 ? "after" : "into";
  };
  const run = (fn: () => void) => {
    setMenuOpen(false);
    fn();
  };

  return (
    <div>
      <div
        ref={rowRef}
        draggable={dnd.enabled}
        className={cn(
          "group relative flex cursor-pointer items-center gap-1 rounded-md py-1 pr-1 text-sm text-muted-foreground hover:bg-muted hover:text-foreground",
          hovering && "bg-accent/10 text-foreground ring-1 ring-accent",
          dnd.enabled && "cursor-grab",
          dnd.draggingId === draggingId && "opacity-40",
        )}
        style={{ paddingLeft: depth * 14 + 2 }}
        onClick={() => setOpen((v) => !v)}
        title={dnd.enabled ? `${group.name}（可拖拽调整位置，或把快捷请求拖入该分组）` : group.name}
        onDragStart={(e) => {
          e.stopPropagation();
          e.dataTransfer.effectAllowed = "move";
          e.dataTransfer.setData("text/plain", String(group.id));
          dnd.begin({ kind: "group", group }, draggingId);
        }}
        onDragEnd={() => dnd.end()}
        onDragOver={(e) => {
          if (!dnd.active || blocked()) return;
          e.preventDefault();
          e.stopPropagation();
          e.dataTransfer.dropEffect = "move";
          if (dnd.payloadKind === "group") {
            const z = zone(e);
            if (z === "into") {
              setOpen(true);
              dnd.setTarget({ kind: "into-group", id: group.id });
            } else {
              dnd.setTarget({ kind: "group-pos", id: group.id, parentId: group.parentId, after: z === "after" });
            }
          } else {
            // 拖入该分组：展开以便看到落点
            setOpen(true);
            dnd.setTarget({ kind: "into-group", id: group.id });
          }
        }}
        onDragLeave={(e) => {
          if (rowRef.current?.contains(e.relatedTarget as Node)) return;
          const t = dnd.target;
          if (t && (t.kind === "into-group" || t.kind === "group-pos") && t.id === group.id) {
            dnd.setTarget(null);
          }
        }}
        onDrop={(e) => {
          e.preventDefault();
          e.stopPropagation();
          if (dnd.payloadKind === "group") {
            const z = zone(e);
            dnd.drop(
              z === "into"
                ? { kind: "into-group", id: group.id }
                : { kind: "group-pos", id: group.id, parentId: group.parentId, after: z === "after" },
            );
          } else {
            dnd.drop({ kind: "into-group", id: group.id });
          }
        }}
      >
        {marker && <DropLine pos={marker} />}
        <span className="w-3 shrink-0">
          {open ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
        </span>
        <Folder className="h-3.5 w-3.5 shrink-0 text-yellow-500/80" />
        <span className="min-w-0 truncate">{group.name}</span>
        <span className="ml-auto flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100">
          <button
            className={cn("rounded p-0.5 hover:bg-border cursor-pointer", menuOpen && "opacity-100")}
            title="更多操作（新建 / 重命名 / 删除）"
            onClick={(e) => {
              e.stopPropagation();
              setMenuOpen((v) => !v);
            }}
          >
            <MoreVertical className="h-3 w-3" />
          </button>
        </span>
        {menuOpen && (
          <div className="absolute right-1 top-7 z-50 w-44 overflow-hidden rounded-md border border-border bg-muted shadow-xl">
            <MenuItem
              icon={<Zap className="h-3.5 w-3.5" />}
              label="新建快捷请求"
              onClick={() => run(() => actions.onCreateRequest(group.id))}
            />
            <MenuItem
              icon={<FolderPlus className="h-3.5 w-3.5" />}
              label="新建子分组"
              onClick={() => run(() => actions.onCreateGroup(group.id, ""))}
            />
            <MenuItem
              icon={<Pencil className="h-3.5 w-3.5" />}
              label="重命名"
              onClick={() => run(() => actions.onRenameGroup(group))}
            />
            <div className="my-1 h-px bg-border" />
            <MenuItem
              danger
              icon={<Trash2 className="h-3.5 w-3.5" />}
              label="删除"
              onClick={() => run(() => actions.onDeleteGroup(group))}
            />
          </div>
        )}
      </div>
      {open && (
        <div className="ml-2 border-l border-border">
          <Level
            groups={child?.groups ?? []}
            requests={child?.requests ?? []}
            levels={levels}
            depth={depth + 1}
            activeId={activeId}
            actions={actions}
          />
        </div>
      )}
    </div>
  );
}

/** 请求行：操作与接口行一致（编辑 / 复制 / 移动 / 删除，无导出） */
function QuickRequestRow({
  item,
  depth,
  active,
  actions,
}: {
  item: QuickRequestSummary;
  depth: number;
  active: boolean;
  actions: NodeActions;
}) {
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

  const run = (fn: () => void) => {
    setMenuOpen(false);
    fn();
  };

  const dnd = actions.dnd;
  const rowId = `request:${item.id}`;
  const marker =
    dnd.target?.kind === "request" && dnd.target.id === item.id
      ? dnd.target.after
        ? "after"
        : "before"
      : dnd.target?.kind === "level" &&
          dnd.target.parentId === item.groupId &&
          dnd.firstRequestId(item.groupId) === item.id
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
    if (p?.kind !== "group") return true;
    let cur: number | null = item.groupId;
    while (cur !== null) {
      if (cur === p.group.id) return false;
      cur = actions.parentOf.get(cur) ?? null;
    }
    return true;
  };

  return (
    <div
      ref={rowRef}
      draggable={dnd.enabled}
      className={cn(
        "group relative flex items-center gap-1 rounded-md py-1 pr-1 text-sm cursor-pointer",
        active
          ? "bg-accent text-accent-foreground"
          : "text-muted-foreground hover:bg-muted hover:text-foreground",
        dnd.enabled && "cursor-grab",
        dnd.draggingId === rowId && "opacity-40",
      )}
      style={{ paddingLeft: depth * 14 + 7 }}
      onClick={() => actions.onOpen(item.id)}
      title={item.url || "（未设置地址）"}
      onDragStart={(e) => {
        e.stopPropagation();
        e.dataTransfer.effectAllowed = "move";
        e.dataTransfer.setData("text/plain", String(item.id));
        dnd.begin({ kind: "request", item }, rowId);
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
            ? { kind: "level", parentId: item.groupId }
            : { kind: "request", groupId: item.groupId, id: item.id, after: dropAfter(e) },
        );
      }}
      onDragLeave={(e) => {
        if (rowRef.current?.contains(e.relatedTarget as Node)) return;
        if (dnd.target?.kind === "request" && dnd.target.id === item.id) dnd.setTarget(null);
        if (dnd.target?.kind === "level" && dnd.target.parentId === item.groupId) dnd.setTarget(null);
      }}
      onDrop={(e) => {
        e.preventDefault();
        e.stopPropagation();
        dnd.drop(
          dnd.payloadKind === "group"
            ? { kind: "level", parentId: item.groupId }
            : { kind: "request", groupId: item.groupId, id: item.id, after: dropAfter(e) },
        );
      }}
    >
      {marker && <DropLine pos={marker} />}
      <span
        className={cn(
          "w-10 shrink-0 truncate text-[10px]",
          item.method.startsWith("G") ? "text-green-500" : "text-orange-400",
        )}
      >
        {item.method}
      </span>
      <span className="min-w-0 truncate">{item.name}</span>
      <span className="ml-auto flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100">
        <button
          className={cn("rounded p-0.5 hover:bg-border cursor-pointer", menuOpen && "opacity-100")}
          title="更多操作（编辑 / 复制 / 移动 / 删除）"
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
          <MenuItem
            icon={<Pencil className="h-3.5 w-3.5" />}
            label="编辑"
            onClick={() => run(() => actions.onRenameRequest(item))}
          />
          <MenuItem
            icon={<Copy className="h-3.5 w-3.5" />}
            label="复制"
            onClick={() => run(() => actions.onCopyRequest(item))}
          />
          <MenuItem
            icon={<MoveRight className="h-3.5 w-3.5" />}
            label="移动"
            onClick={() => run(() => actions.onMoveRequest(item))}
          />
          <div className="my-1 h-px bg-border" />
          <MenuItem
            danger
            icon={<Trash2 className="h-3.5 w-3.5" />}
            label="删除"
            onClick={() => run(() => actions.onDeleteRequest(item))}
          />
        </div>
      )}
    </div>
  );
}
