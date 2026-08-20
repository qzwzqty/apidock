import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { FolderPlus, FilePlus2, X, Settings2, ChevronDown, FileText, Loader2 } from "lucide-react";
import { InterfaceTree, PromptDialog, type IfaceRef } from "@/components/InterfaceTree";
import { InterfaceEditor } from "@/components/InterfaceEditor";
import { ResponseView } from "@/components/ResponseView";
import { EnvManager } from "@/components/EnvManager";
import { ProjectSettingsDialog } from "@/components/ProjectSettingsDialog";
import { api, type EnvironmentSummary, type SendOutcome } from "@/lib/api";
import { useProject } from "@/lib/project";
import { cn } from "@/lib/utils";

type DlgState =
  | { kind: "createGroup"; parentPath: string[] }
  | { kind: "createIface"; groupPath: string[] }
  | { kind: "renameGroup"; ref: IfaceRef }
  | null;

export function ProjectPage({ teamKey, projectKey }: { teamKey: string; projectKey: string }) {
  const tabId = `project:${teamKey}:${projectKey}`;
  const proj = useProject((s) => s.states[tabId]);
  const {
    loadTree, openInterface, closeInterface, setActive, createGroup, renameGroup, deleteGroup,
    createInterface, deleteInterface, saveDoc,
  } = useProject.getState();

  const [dlg, setDlg] = useState<DlgState>(null);
  const [envs, setEnvs] = useState<EnvironmentSummary[]>([]);
  const [activeEnv, setActiveEnv] = useState<string>("env-prod");
  const [showEnvMgr, setShowEnvMgr] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [sendState, setSendState] = useState<
    { kind: "idle" } | { kind: "loading" } | { kind: "done"; outcome: SendOutcome }
  >({ kind: "idle" });

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

  // 外部变更（git pull / 外部编辑器 / 其它实例）自动刷新
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let promise: Promise<() => void> | undefined;
    void (promise = listen<string>("fs://changed", () => {
      if (disposed) return;
      void useProject.getState().loadTree(tabId, teamKey, projectKey);
      void loadEnvs();
    }));
    return () => {
      disposed = true;
      if (unlisten) unlisten();
      if (promise) void Promise.resolve(promise).then((f) => f());
    };
  }, [tabId, teamKey, projectKey]);

  const activeTabObj = proj?.openTabs.find((t) => t.id === proj.activeTab);
  const activeDoc = activeTabObj ? proj.docs[activeTabObj.id] : undefined;
  const showResponse = sendState.kind === "done";
  const activeEnvName = envs.find((e) => e.id === activeEnv)?.name ?? activeEnv;

  const handleSend = async (doc: Parameters<typeof api.sendRequest>[3]) => {
    setSendState({ kind: "loading" });
    const outcome = await api.sendRequest(teamKey, projectKey, activeEnv, doc);
    setSendState({ kind: "done", outcome });
  };

  const switchEnv = async (id: string) => {
    await api.setActiveEnvironment(teamKey, projectKey, id);
    setActiveEnv(id);
  };

  return (
    <div className="flex h-full">
      {/* 左侧：接口树 */}
      <aside className="flex w-64 shrink-0 flex-col border-r border-border bg-muted">
        <div className="flex items-center justify-between px-3 py-2">
          <span className="text-sm font-semibold text-foreground">接口</span>
          <div className="flex items-center gap-1">
            <button
              className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground cursor-pointer"
              title="在根级新建分组"
              onClick={() => setDlg({ kind: "createGroup", parentPath: [] })}
            >
              <FolderPlus className="h-4 w-4" />
            </button>
            <button
              className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground cursor-pointer"
              title="在根级新建接口"
              onClick={() => setDlg({ kind: "createIface", groupPath: [] })}
            >
              <FilePlus2 className="h-4 w-4" />
            </button>
          </div>
        </div>
        <div className="flex-1 overflow-y-auto px-2 pb-2">
          {!proj?.loaded ? (
            <p className="px-2 py-3 text-xs text-muted-foreground">加载中…</p>
          ) : (
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
              onDeleteIface={(ref) => {
                if (confirm(`删除接口「${ref.key}」？此操作不可恢复。`)) {
                  void deleteInterface(tabId, teamKey, projectKey, ref.groupPath, ref.key);
                }
              }}
            />
          )}
        </div>
      </aside>

      {/* 右侧：接口定义 / 调试 */}
      <main className="flex min-w-0 flex-1 flex-col">
        {/* 项目上下文行：项目定位 + 环境选择 + 项目设置 */}
        <div className="flex h-9 shrink-0 items-center border-b border-border text-sm">
          <span className="px-4 text-muted-foreground">{teamKey} / {projectKey}</span>
          <button
            className="ml-auto flex h-full cursor-pointer items-center gap-2 px-3 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            onClick={() => setShowEnvMgr(true)}
            title="环境管理"
          >
            <select
              className="cursor-pointer bg-transparent text-xs text-muted-foreground outline-none"
              value={activeEnv}
              onClick={(e) => e.stopPropagation()}
              onChange={(e) => void switchEnv(e.target.value)}
            >
              {envs.map((e) => (
                <option key={e.id} value={e.id}>{e.name}</option>
              ))}
            </select>
            <ChevronDown className="h-3.5 w-3.5" />
          </button>
          <button
            className="flex h-full cursor-pointer items-center px-3 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            onClick={() => setShowSettings(true)}
            title="项目设置（全局变量/全局参数）"
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
                "group relative flex max-w-[200px] cursor-pointer items-center gap-1 border-r border-border px-3",
                proj.activeTab === tab.id ? "bg-muted text-foreground" : "text-muted-foreground hover:bg-muted",
              )}
              onClick={() => setActive(tabId, tab.id)}
            >
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
            <span className="self-center px-3 text-xs text-muted-foreground">左侧点击接口打开</span>
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
              <InterfaceEditor
                doc={activeDoc}
                onSave={(doc) => saveDoc(tabId, teamKey, projectKey, activeTabObj.groupPath, activeTabObj.key, doc)}
                onSend={(doc) => void handleSend(doc)}
              />
            ) : (
              <div className="flex h-full flex-col items-center justify-center gap-2 text-sm text-muted-foreground">
                <FileText className="h-8 w-8 opacity-40" />
                从左侧接口树选择接口进行定义
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
          keyLabel="英文键"
          onClose={() => setDlg(null)}
          onSubmit={(key, name) => createGroup(tabId, teamKey, projectKey, dlg.parentPath, key, name)}
        />
      )}
      {dlg?.kind === "createIface" && (
        <PromptDialog
          open
          title="新建接口"
          nameLabel="接口名"
          keyLabel="英文键"
          onClose={() => setDlg(null)}
          onSubmit={(key, name) => createInterface(tabId, teamKey, projectKey, dlg.groupPath, key, name)}
        />
      )}
      {dlg?.kind === "renameGroup" && (
        <PromptDialog
          open
          title="重命名分组"
          nameLabel="显示名"
          keyLabel="英文键"
          extraName={dlg.ref.key}
          extraKey={dlg.ref.key}
          onClose={() => setDlg(null)}
          onSubmit={(key, name) => renameGroup(tabId, teamKey, projectKey, dlg.ref.groupPath, key, name)}
          confirmText="重命名"
        />
      )}

      <EnvManager
        teamKey={teamKey}
        projectKey={projectKey}
        activeId={activeEnv}
        open={showEnvMgr}
        onClose={() => setShowEnvMgr(false)}
        onChanged={(id) => {
          setActiveEnv(id);
          void loadEnvs();
        }}
      />
      <ProjectSettingsDialog
        teamKey={teamKey}
        projectKey={projectKey}
        open={showSettings}
        onClose={() => setShowSettings(false)}
      />
    </div>
  );
}