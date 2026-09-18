import { useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { ChevronDown, ChevronRight, FolderPlus, FolderTree, FilePlus2, MoreVertical, Upload, Download, X, Settings2, FileText, Loader2, Zap } from "lucide-react";
import { InterfaceTree, PromptDialog, type IfaceRef } from "@/components/InterfaceTree";
import { QuickRequestList } from "@/components/QuickRequestList";
import { QuickMoveDialog } from "@/components/QuickMoveDialog";
import { InterfaceEditor, type EditorMode } from "@/components/InterfaceEditor";
import { ResponseView } from "@/components/ResponseView";
import { EnvironmentDialog } from "@/components/EnvironmentDialog";
import { ImportExportDialog } from "@/components/ImportExportDialog";
import { MoveTargetDialog } from "@/components/MoveTargetDialog";
import {
  api,
  type EnvironmentSummary,
  type InterfaceFile,
  type QuickGroupSummary,
  type QuickRequestSummary,
  type SendOutcome,
  type TreeNode,
} from "@/lib/api";
import { useProject } from "@/lib/project";
import { useWorkspace } from "@/lib/workspace";
import { cn } from "@/lib/utils";

type DlgState =
  | { kind: "createGroup"; parentPath: string[] }
  | { kind: "createIface"; groupPath: string[] }
  | { kind: "renameGroup"; ref: IfaceRef }
  | { kind: "renameIface"; ref: IfaceRef }
  | { kind: "moveIface"; ref: IfaceRef; exclude?: string[] | null }
  | { kind: "moveGroup"; ref: IfaceRef; exclude?: string[] | null }
  | { kind: "renameQuick"; item: QuickRequestSummary }
  | { kind: "createQuickGroup"; parentId: number | null }
  | { kind: "renameQuickGroup"; group: QuickGroupSummary }
  | { kind: "moveQuick"; item: QuickRequestSummary }
  | null;

/** 统计接口树内的接口数（用于「接口管理」标题徽标） */
function countInterfaces(nodes: TreeNode[]): number {
  let n = 0;
  for (const node of nodes) {
    if (node.type === "interface") n += 1;
    else n += countInterfaces(node.children);
  }
  return n;
}

export function ProjectPage({ teamKey, projectKey }: { teamKey: string; projectKey: string }) {
  const tabId = `project:${teamKey}:${projectKey}`;
  const proj = useProject((s) => s.states[tabId]);
  const {
    loadTree, openInterface, closeInterface, setActive, createGroup, renameGroup, deleteGroup,
    createInterface, renameInterface, moveInterface, deleteInterface, saveDoc, moveGroup,
    openQuickRequest, createQuickRequest, createQuickGroup, renameQuickGroup, deleteQuickGroup,
    moveQuickGroup, copyQuickRequest, moveQuickRequest, renameQuickRequest, deleteQuickRequest, saveQuickDoc,
  } = useProject.getState();

  const [dlg, setDlg] = useState<DlgState>(null);
  const [envs, setEnvs] = useState<EnvironmentSummary[]>([]);
  const [activeEnv, setActiveEnv] = useState<string>("env-prod");
  const [showEnvSettings, setShowEnvSettings] = useState(false);
  const [sendState, setSendState] = useState<
    { kind: "idle" } | { kind: "loading" } | { kind: "done"; outcome: SendOutcome }
  >({ kind: "idle" });
  const [showImportExport, setShowImportExport] = useState(false);
  const [importExportMode, setImportExportMode] = useState<"import" | "export">("import");
  const [editorMode, setEditorMode] = useState<EditorMode>("doc");
  const [menuOpen, setMenuOpen] = useState(false);
  /** 「接口管理」区块折叠（与「快捷请求」区块一致的标题 + 折叠交互） */
  const [treeExpanded, setTreeExpanded] = useState(true);
  const menuRef = useRef<HTMLDivElement>(null);

  const openImportExport = (mode: "import" | "export") => {
    setImportExportMode(mode);
    setShowImportExport(true);
    setMenuOpen(false);
  };

  useEffect(() => {
    if (!menuOpen) return;
    const onMouseDown = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) setMenuOpen(false);
    };
    document.addEventListener("mousedown", onMouseDown);
    return () => document.removeEventListener("mousedown", onMouseDown);
  }, [menuOpen]);

  const loadEnvs = async () => {
    const [list, settings] = await Promise.all([
      api.listEnvironments(teamKey, projectKey),
      api.getProjectSettings(teamKey, projectKey),
    ]);
    setEnvs(list);
    if (settings.activeEnvironmentId) setActiveEnv(settings.activeEnvironmentId);
  };

  useEffect(() => {
    void loadTree(tabId, teamKey, projectKey);
    void loadEnvs();
  }, [tabId, teamKey, projectKey]);

  const activeTabObj = proj?.openTabs.find((t) => t.id === proj.activeTab);
  const activeDoc = activeTabObj ? proj.docs[activeTabObj.id] : undefined;
  const quickActive = activeTabObj?.kind === "quick";
  const openInEdit = !!proj?.openInEditId && proj.openInEditId === activeTabObj?.id;
  // 快捷请求与请求历史一致：响应面板始终与请求并排展示（不受编辑器内部标签影响）
  const showResponse = sendState.kind === "done" && (quickActive || editorMode === "debug");
  const activeEnvName = envs.find((e) => e.id === activeEnv)?.name ?? activeEnv;
  const activeHost = envs.find((e) => e.id === activeEnv)?.host ?? "";

  // 新建接口的标签已进入编辑态，清除一次性标记
  useEffect(() => {
    if (!openInEdit) return;
    useProject.setState((s) => {
      const st = s.states[tabId];
      if (!st?.openInEditId) return {};
      return { states: { ...s.states, [tabId]: { ...st, openInEditId: null } } };
    });
  }, [openInEdit, tabId]);

  const handleSend = async (doc: Parameters<typeof api.sendRequest>[3]) => {
    setSendState({ kind: "loading" });
    // 附带当前接口 key/name，供后端写入请求历史
    const outcome = await api.sendRequest(
      teamKey,
      projectKey,
      activeEnv,
      doc,
      activeTabObj?.key,
      activeTabObj?.name,
      activeTabObj?.groupPath,
    );
    setSendState({ kind: "done", outcome });
  };

  /** 发送快捷请求：地址栏即完整地址（host 不继承环境），发送后把当前内容落库 */
  const handleQuickSend = async (id: number, doc: InterfaceFile) => {
    setSendState({ kind: "loading" });
    const outcome = await api.sendQuickRequest(teamKey, projectKey, id, doc);
    setSendState({ kind: "done", outcome });
    void saveQuickDoc(tabId, teamKey, projectKey, id, doc);
  };

  const switchEnv = async (id: string) => {
    await api.setActiveEnvironment(teamKey, projectKey, id);
    setActiveEnv(id);
  };

  /** 复制接口（同分组，名称后接 -copy） */
  const copyIface = async (ref: IfaceRef) => {
    try {
      await api.copyInterface(teamKey, projectKey, ref.groupPath, ref.key);
      await useProject.getState().loadTree(tabId, teamKey, projectKey);
    } catch (e) {
      alert(`复制失败：${e}`);
    }
  };

  /** 拖拽移动接口：可拖进其它分组，也可在同组内排序（落库为手动顺序） */
  const dropIface = async (ref: IfaceRef, targetGroupPath: string[], beforeKey: string | null) => {
    try {
      await moveInterface(tabId, teamKey, projectKey, ref.groupPath, ref.key, targetGroupPath, beforeKey);
    } catch (e) {
      alert(`移动失败：${e}`);
    }
  };

  /** 拖拽移动分组：可拖进其它分组（含嵌套与环检测），也可在同层排序 */
  const dropGroup = async (groupPath: string[], targetParentPath: string[], beforeKey: string | null) => {
    try {
      await moveGroup(tabId, teamKey, projectKey, groupPath, targetParentPath, beforeKey);
    } catch (e) {
      alert(`移动失败：${e}`);
    }
  };

  /** 拖拽移动快捷请求：可拖进其它分组，也可在同组内排序 */
  const dropQuick = async (item: QuickRequestSummary, targetGroupId: number | null, beforeId: number | null) => {
    try {
      await moveQuickRequest(tabId, teamKey, projectKey, item.id, targetGroupId, beforeId);
    } catch (e) {
      alert(`移动失败：${e}`);
    }
  };

  /** 拖拽移动快捷请求分组：同层排序或改父（不能拖进自己的子树） */
  const dropQuickGroup = async (group: QuickGroupSummary, parentId: number | null, beforeId: number | null) => {
    try {
      await moveQuickGroup(tabId, teamKey, projectKey, group.id, parentId, beforeId);
    } catch (e) {
      alert(`移动失败：${e}`);
    }
  };

  /** 导出单个接口为 OpenAPI 3.0 JSON */
  const exportIface = async (ref: IfaceRef) => {
    const path = await save({
      defaultPath: `${ref.key}.openapi.json`,
      filters: [{ name: "OpenAPI JSON", extensions: ["json"] }],
    });
    if (!path) return;
    try {
      await api.exportInterfaceOpenapiFile(path, teamKey, projectKey, ref.groupPath, ref.key, false);
    } catch (e) {
      alert(`导出失败：${e}`);
    }
  };

  return (
    <div className="flex h-full">
      {/* 左侧：接口树 */}
      <aside className="flex w-64 shrink-0 flex-col border-r border-border bg-muted">
        <div className="flex items-center justify-between py-2 pl-2 pr-2">
          <div
            className="flex min-w-0 flex-1 cursor-pointer items-center gap-1 rounded-md py-1 pl-1 text-sm text-muted-foreground hover:bg-muted hover:text-foreground"
            onClick={() => setTreeExpanded((v) => !v)}
            title="接口管理：展开 / 折叠接口树"
          >
            <span className="w-3 shrink-0">
              {treeExpanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
            </span>
            <FolderTree className="h-3.5 w-3.5 shrink-0 text-yellow-500/80" />
            <span className="min-w-0 truncate font-semibold text-foreground">接口管理</span>
            <span className="shrink-0 rounded bg-border/60 px-1 text-[10px]">
              {countInterfaces(proj?.tree ?? [])}
            </span>
          </div>
          <div className="relative" ref={menuRef}>
            <button
              className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground cursor-pointer"
              title="更多操作（新建 / 导入 / 导出）"
              onClick={(e) => {
                e.stopPropagation();
                setMenuOpen((v) => !v);
              }}
            >
              <MoreVertical className="h-4 w-4" />
            </button>
            {menuOpen && (
              <div className="absolute right-0 top-8 z-50 w-40 overflow-hidden rounded-md border border-border bg-muted shadow-xl">
                <button
                  className="flex w-full items-center gap-2 px-3 py-1.5 text-sm text-foreground transition-colors hover:bg-border cursor-pointer select-none"
                  onClick={() => {
                    setDlg({ kind: "createIface", groupPath: [] });
                    setMenuOpen(false);
                  }}
                >
                  <FilePlus2 className="h-4 w-4" /> 新建接口
                </button>
                <button
                  className="flex w-full items-center gap-2 px-3 py-1.5 text-sm text-foreground transition-colors hover:bg-border cursor-pointer select-none"
                  onClick={() => {
                    setDlg({ kind: "createGroup", parentPath: [] });
                    setMenuOpen(false);
                  }}
                >
                  <FolderPlus className="h-4 w-4" /> 新建目录
                </button>
                <div className="my-1 h-px bg-border" />
                <button
                  className="flex w-full items-center gap-2 px-3 py-1.5 text-sm text-foreground transition-colors hover:bg-border cursor-pointer select-none"
                  onClick={() => openImportExport("import")}
                >
                  <Upload className="h-4 w-4" /> 导入
                </button>
                <button
                  className="flex w-full items-center gap-2 px-3 py-1.5 text-sm text-foreground transition-colors hover:bg-border cursor-pointer select-none"
                  onClick={() => openImportExport("export")}
                >
                  <Download className="h-4 w-4" /> 导出
                </button>
              </div>
            )}
          </div>
        </div>
        <div className="flex-1 overflow-y-auto px-2 pb-2">
          {!proj?.loaded ? (
            <p className="px-2 py-3 text-xs text-muted-foreground">加载中…</p>
          ) : !treeExpanded ? null : (
            <InterfaceTree
              tree={proj.tree}
              activeId={proj.activeTab}
              onOpenIface={(ref) => void openInterface(tabId, teamKey, projectKey, ref.groupPath, ref.key)}
              onCreateGroup={(parentPath) => setDlg({ kind: "createGroup", parentPath })}
              onRenameGroup={(ref) => setDlg({ kind: "renameGroup", ref })}
              onDeleteGroup={(ref) => {
                if (confirm(`删除分组「${ref.groupPath[ref.groupPath.length - 1]}」及其下全部接口？此操作不可恢复。`)) {
                  void deleteGroup(tabId, teamKey, projectKey, ref.groupPath);
                }
              }}
              onCreateIface={(groupPath) => setDlg({ kind: "createIface", groupPath })}
              onRenameIface={(ref) => setDlg({ kind: "renameIface", ref })}
              onDeleteIface={(ref) => {
                if (confirm(`删除接口「${ref.key}」？此操作不可恢复。`)) {
                  void deleteInterface(tabId, teamKey, projectKey, ref.groupPath, ref.key);
                }
              }}
              onMoveIface={(ref) => setDlg({ kind: "moveIface", ref, exclude: ref.groupPath })}
              onDropIface={(ref, targetGroupPath, beforeKey) => void dropIface(ref, targetGroupPath, beforeKey)}
              onDropGroup={(groupPath, targetParentPath, beforeKey) =>
                void dropGroup(groupPath, targetParentPath, beforeKey)
              }
              onMoveGroup={(ref) => setDlg({ kind: "moveGroup", ref, exclude: ref.groupPath })}
              onExportIface={(ref) => void exportIface(ref)}
              onCopyIface={(ref) => void copyIface(ref)}
            />
          )}
          {/* 快捷请求：与环境无关的临时接口（地址栏填写完整地址），支持分组 */}
          <QuickRequestList
            tree={proj?.quick ?? { groups: [], requests: [] }}
            activeId={proj?.activeTab ?? null}
            onOpen={(id) => void openQuickRequest(tabId, teamKey, projectKey, id)}
            onCreateRequest={(groupId) => void createQuickRequest(tabId, teamKey, projectKey, groupId)}
            onCreateGroup={(parentId) => setDlg({ kind: "createQuickGroup", parentId })}
            onRenameRequest={(item) => setDlg({ kind: "renameQuick", item })}
            onCopyRequest={(item) => void copyQuickRequest(tabId, teamKey, projectKey, item.id)}
            onMoveRequest={(item) => setDlg({ kind: "moveQuick", item })}
            onDeleteRequest={(item) => {
              if (confirm(`删除快捷请求「${item.name}」？此操作不可恢复。`)) {
                void deleteQuickRequest(tabId, teamKey, projectKey, item.id);
              }
            }}
            onDropRequest={(item, targetGroupId, beforeId) => void dropQuick(item, targetGroupId, beforeId)}
            onDropGroup={(group, parentId, beforeId) => void dropQuickGroup(group, parentId, beforeId)}
            onRenameGroup={(group) => setDlg({ kind: "renameQuickGroup", group })}
            onDeleteGroup={(group) => {
              if (confirm(`删除分组「${group.name}」及其下全部子分组与快捷请求？此操作不可恢复。`)) {
                void deleteQuickGroup(tabId, teamKey, projectKey, group.id);
              }
            }}
          />
        </div>
      </aside>

      {/* 右侧：接口定义 / 调试 */}
      <main className="flex min-w-0 flex-1 flex-col">
        {/* 项目上下文行：项目定位 + 环境选择 + 环境设置 */}
        <div className="flex h-9 shrink-0 items-center border-b border-border text-sm select-none">
          <span className="px-4 text-muted-foreground">{teamKey} / {projectKey}</span>
          <div className="ml-auto flex h-full items-center pl-3">
            {quickActive ? (
              <span
                className="flex items-center gap-1 px-2 text-[11px] text-muted-foreground"
                title="快捷请求的 host 不继承环境变量，请在地址栏填写完整地址"
              >
                <Zap className="h-3 w-3 text-sky-400" /> 完整地址 · 不继承环境变量
              </span>
            ) : (
              <select
                className="h-6 cursor-pointer rounded-md border border-border bg-muted px-2 text-xs text-muted-foreground outline-none focus:border-ring"
                value={activeEnv}
                title="切换环境"
                onChange={(e) => void switchEnv(e.target.value)}
              >
                {envs.map((e) => (
                  <option key={e.id} value={e.id}>{e.name}</option>
                ))}
              </select>
            )}
          </div>
          <button
            className="flex h-full cursor-pointer items-center px-3 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            onClick={() => setShowEnvSettings(true)}
            title="环境设置（环境 / 全局变量 / 全局参数）"
          >
            <Settings2 className="h-3.5 w-3.5" />
          </button>
        </div>

        {/* 接口子标签栏 */}
        <div className="flex h-9 shrink-0 items-stretch border-b border-border">
          {proj?.openTabs.map((tab) => (
            <div
              key={tab.id}
              className={cn(
                "group relative flex max-w-[200px] cursor-pointer items-center gap-1 border-r border-border px-3 select-none",
                proj.activeTab === tab.id ? "bg-muted text-foreground" : "text-muted-foreground hover:bg-muted",
              )}
              onClick={() => setActive(tabId, tab.id)}
            >
              {tab.kind === "quick" && <Zap className="h-3 w-3 shrink-0 text-sky-400" />}
              <span className="truncate text-sm">{tab.name}</span>
              <button
                className="rounded-sm p-0.5 text-muted-foreground opacity-0 transition-opacity hover:bg-border group-hover:opacity-100 cursor-pointer"
                onClick={(e) => {
                  e.stopPropagation();
                  closeInterface(tabId, tab.id);
                }}
                title="关闭"
              >
                <X className="h-3 w-3" />
              </button>
            </div>
          ))}
          {!proj?.openTabs.length && (
            <span className="self-center px-3 text-xs text-muted-foreground">
              左侧点击接口打开，或在「快捷请求」中新建临时请求
            </span>
          )}
          {sendState.kind === "loading" && (
            <span className="ml-auto flex items-center gap-1.5 self-center pr-3 text-xs text-accent">
              <Loader2 className="h-3.5 w-3.5 animate-spin" /> 发送中…
            </span>
          )}
        </div>

        {/* 编辑区 + 响应面板（分割） */}
        <div className="flex min-h-0 flex-1">
          <div className={cn("min-w-0", showResponse ? "w-3/5 border-r border-border" : "w-full")}>
            {activeTabObj && activeDoc ? (
              activeTabObj.kind === "quick" ? (
                <InterfaceEditor
                  doc={activeDoc}
                  host=""
                  urlMode="absolute"
                  debugOnly
                  onSave={(doc) =>
                    saveQuickDoc(tabId, teamKey, projectKey, Number(activeTabObj.key), doc)
                  }
                  onSend={(doc) => void handleQuickSend(Number(activeTabObj.key), doc)}
                />
              ) : (
                <InterfaceEditor
                  doc={activeDoc}
                  host={activeHost}
                  defaultMode={openInEdit ? "edit" : undefined}
                  onSave={(doc) => saveDoc(tabId, teamKey, projectKey, activeTabObj.groupPath, activeTabObj.key, doc)}
                  onSend={(doc) => void handleSend(doc)}
                  onModeChange={setEditorMode}
                />
              )
            ) : (
              <div className="flex h-full flex-col items-center justify-center gap-2 text-sm text-muted-foreground">
                <FileText className="h-8 w-8 opacity-40" />
                从左侧接口树选择接口进行定义
                <span className="text-xs">或在下方「快捷请求」中新建临时请求（完整地址，不依赖环境）</span>
                <span className="text-xs">当前环境：{activeEnvName}（{activeEnv}）</span>
              </div>
            )}
          </div>
          {showResponse && (
            <div className="min-w-0 flex-1">
              <ResponseView
                outcome={sendState.outcome}
                onClear={() => setSendState({ kind: "idle" })}
              />
            </div>
          )}
        </div>
      </main>

      {dlg?.kind === "createGroup" && (
        <PromptDialog
          open
          title="新建分组"
          nameLabel="分组名"
          description
          onClose={() => setDlg(null)}
          onSubmit={(name, description) => createGroup(tabId, teamKey, projectKey, dlg.parentPath, name, description)}
        />
      )}
      {dlg?.kind === "createIface" && (
        <PromptDialog
          open
          title="新建接口"
          nameLabel="接口名（将作为文件名）"
          description
          onClose={() => setDlg(null)}
          onSubmit={(name, description) => createInterface(tabId, teamKey, projectKey, dlg.groupPath, name, description)}
        />
      )}
      {dlg?.kind === "renameIface" && (
        <PromptDialog
          open
          title="重命名接口"
          nameLabel="接口名（将作为文件名）"
          mode="rename"
          extraName={dlg.ref.key}
          onClose={() => setDlg(null)}
          onSubmit={(name) => renameInterface(tabId, teamKey, projectKey, dlg.ref.groupPath, dlg.ref.key, name)}
          confirmText="重命名"
        />
      )}
      {dlg?.kind === "renameGroup" && (
        <PromptDialog
          open
          title="重命名分组"
          nameLabel="分组名（将作为目录名）"
          mode="rename"
          extraName={dlg.ref.key}
          onClose={() => setDlg(null)}
          onSubmit={(name) => renameGroup(tabId, teamKey, projectKey, dlg.ref.groupPath, name)}
          confirmText="重命名"
        />
      )}
      {dlg?.kind === "moveIface" && (
        <MoveTargetDialog
          open
          title={`移动接口「${dlg.ref.key}」到：`}
          tree={proj?.tree ?? []}
          excludePath={null}
          onClose={() => setDlg(null)}
          onConfirm={async (target) => {
            // 与拖拽移动共用同一动作：已打开的标签页会跟着改路径
            await moveInterface(tabId, teamKey, projectKey, dlg.ref.groupPath, dlg.ref.key, target, null);
            setDlg(null);
          }}
        />
      )}
      {dlg?.kind === "renameQuick" && (
        <PromptDialog
          open
          title="重命名快捷请求"
          nameLabel="名称（仅作显示，可重名）"
          mode="rename"
          extraName={dlg.item.name}
          renameHint="快捷请求名称仅作显示，不参与唯一键，可与其他快捷请求或接口重名。"
          onClose={() => setDlg(null)}
          onSubmit={(name) => renameQuickRequest(tabId, teamKey, projectKey, dlg.item.id, name)}
          confirmText="重命名"
        />
      )}
      {dlg?.kind === "createQuickGroup" && (
        <PromptDialog
          open
          title={dlg.parentId === null ? "新建快捷请求分组" : "新建子分组"}
          nameLabel="分组名（仅作显示，可重名）"
          onClose={() => setDlg(null)}
          onSubmit={(name) => createQuickGroup(tabId, teamKey, projectKey, dlg.parentId, name)}
        />
      )}
      {dlg?.kind === "renameQuickGroup" && (
        <PromptDialog
          open
          title="重命名快捷请求分组"
          nameLabel="分组名（仅作显示，可重名）"
          mode="rename"
          extraName={dlg.group.name}
          renameHint="分组名仅作显示，不参与唯一键，可与其它分组重名。"
          onClose={() => setDlg(null)}
          onSubmit={(name) => renameQuickGroup(tabId, teamKey, projectKey, dlg.group.id, name)}
          confirmText="重命名"
        />
      )}
      {dlg?.kind === "moveQuick" && (
        <QuickMoveDialog
          open
          title={`移动快捷请求「${dlg.item.name}」到：`}
          groups={proj?.quick.groups ?? []}
          onClose={() => setDlg(null)}
          onConfirm={async (groupId) => {
            await moveQuickRequest(tabId, teamKey, projectKey, dlg.item.id, groupId);
            setDlg(null);
          }}
        />
      )}
      {dlg?.kind === "moveGroup" && (
        <MoveTargetDialog
          open
          title={`移动分组「${dlg.ref.key}」到：`}
          tree={proj?.tree ?? []}
          excludePath={dlg.ref.groupPath}
          onClose={() => setDlg(null)}
          onConfirm={async (target) => {
            // 与拖拽移动共用同一动作：整棵子树下已打开的标签页会跟着改路径
            await moveGroup(tabId, teamKey, projectKey, dlg.ref.groupPath, target, null);
            setDlg(null);
          }}
        />
      )}

      <EnvironmentDialog
        teamKey={teamKey}
        projectKey={projectKey}
        activeId={activeEnv}
        open={showEnvSettings}
        onClose={() => {
          setShowEnvSettings(false);
          // 关闭时重新拉取环境列表，保证保存的 host/变量立即反映到编辑器
          void loadEnvs();
        }}
        onChanged={(id) => {
          setActiveEnv(id);
          void loadEnvs();
        }}
      />
      <ImportExportDialog
        teamKey={teamKey}
        projectKey={projectKey}
        open={showImportExport}
        initialMode={importExportMode}
        onClose={() => setShowImportExport(false)}
        onImported={async () => {
          // 导入可能新增接口（当前项目）或新增项目（新建项目导入），两者都刷新；
          // 不在此关闭弹窗，便于用户查看导入报告（由弹窗内“关闭”按钮收起）
          await useWorkspace.getState().loadTeamsAndProjects();
          await useProject.getState().loadTree(tabId, teamKey, projectKey);
        }}
      />
    </div>
  );
}