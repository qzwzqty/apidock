import { useState } from "react";
import { Search, Plus, Folder, Trash2, Users, Pencil } from "lucide-react";
import type { TeamInfo, ProjectInfo } from "@/lib/api";
import { useWorkspace } from "@/lib/workspace";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogFooter } from "@/components/ui/dialog";

export function MainWindow() {
  const teams = useWorkspace((s) => s.teams);
  const selectedTeamKey = useWorkspace((s) => s.selectedTeamKey);
  const projects = useWorkspace((s) => s.projects);
  const selectTeam = useWorkspace((s) => s.selectTeam);
  const deleteTeam = useWorkspace((s) => s.deleteTeam);
  const renameTeam = useWorkspace((s) => s.renameTeam);
  const createTeam = useWorkspace((s) => s.createTeam);
  const createProject = useWorkspace((s) => s.createProject);
  const deleteProject = useWorkspace((s) => s.deleteProject);
  const renameProject = useWorkspace((s) => s.renameProject);
  const openProject = useWorkspace((s) => s.openProject);

  const [teamQ, setTeamQ] = useState("");
  const [projQ, setProjQ] = useState("");
  const [showTeamDlg, setShowTeamDlg] = useState(false);
  const [showProjDlg, setShowProjDlg] = useState(false);
  const [renameTeamTarget, setRenameTeamTarget] = useState<TeamInfo | null>(null);
  const [renameProjTarget, setRenameProjTarget] = useState<ProjectInfo | null>(null);

  const filteredTeams = teams.filter((t) => t.name.includes(teamQ.trim()));
  const selectedTeam = teams.find((t) => t.key === selectedTeamKey) ?? null;
  const filteredProjects = projects.filter((p) => p.name.includes(projQ.trim()));

  return (
    <div className="flex h-full">
      {/* 左侧：团队列表 */}
      <aside className="flex w-60 shrink-0 flex-col border-r border-border bg-muted">
        <div className="flex items-center justify-between px-3 py-2">
          <span className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
            <Users className="h-4 w-4" /> 团队
          </span>
          <Button variant="ghost" size="icon" onClick={() => setShowTeamDlg(true)} title="新建团队">
            <Plus className="h-4 w-4" />
          </Button>
        </div>
        <div className="px-2 pb-2">
          <div className="relative">
            <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              className="pl-7"
              placeholder="搜索团队"
              value={teamQ}
              onChange={(e) => setTeamQ(e.target.value)}
            />
          </div>
        </div>
        <div className="flex-1 overflow-y-auto px-2 pb-2">
          {filteredTeams.map((team) => (
            <TeamRow
              key={team.key}
              team={team}
              active={team.key === selectedTeamKey}
              onClick={() => selectTeam(team.key)}
              onRename={() => setRenameTeamTarget(team)}
              onDelete={() => {
                if (confirm(`删除团队「${team.name}」及其下所有项目？此操作不可恢复。`)) {
                  void deleteTeam(team.key);
                }
              }}
            />
          ))}
          {filteredTeams.length === 0 && (
            <p className="px-2 py-3 text-xs text-muted-foreground">
              {teams.length === 0 ? "还没有团队，点击右上角 + 新建" : "没有匹配的团队"}
            </p>
          )}
        </div>
      </aside>

      {/* 右侧：项目列表 */}
      <main className="flex min-w-0 flex-1 flex-col">
        {selectedTeam ? (
          <>
            <div className="flex items-center justify-between border-b border-border px-4 py-2.5">
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <span className="truncate text-base font-semibold text-foreground">
                    {selectedTeam.name}
                  </span>
                  <span className="rounded-full bg-accent/15 px-2 py-0.5 text-xs text-accent">
                    {projects.length} 个项目
                  </span>
                </div>
              </div>
              <Button onClick={() => setShowProjDlg(true)}>
                <Plus className="h-4 w-4" /> 新建项目
              </Button>
            </div>

            <div className="px-4 py-3">
              <div className="relative w-72">
                <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
                <Input
                  className="pl-7"
                  placeholder="搜索项目"
                  value={projQ}
                  onChange={(e) => setProjQ(e.target.value)}
                />
              </div>
            </div>

            <div className="grid flex-1 auto-rows-max grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3 overflow-y-auto px-4 pb-4">
              {filteredProjects.map((proj) => (
                <div
                  key={proj.key}
                  className="group relative flex cursor-pointer flex-col gap-1 rounded-lg border border-border bg-muted p-3.5 transition-colors hover:border-accent/60"
                  onClick={() => openProject(selectedTeam.key, proj.key)}
                >
                  <div className="flex items-center gap-2">
                    <Folder className="h-4 w-4 shrink-0 text-accent" />
                    <span className="truncate text-sm font-medium text-foreground">
                      {proj.name}
                    </span>
                  </div>
                  <span className="absolute right-2 top-2 flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
                    <button
                      className="rounded-sm p-1 text-muted-foreground hover:bg-border hover:text-foreground cursor-pointer"
                      onClick={(e) => {
                        e.stopPropagation();
                        setRenameProjTarget(proj);
                      }}
                      title="重命名项目"
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </button>
                    <button
                      className="rounded-sm p-1 text-muted-foreground hover:bg-border hover:text-red-400 cursor-pointer"
                      onClick={(e) => {
                        e.stopPropagation();
                        if (confirm(`删除项目「${proj.name}」及其所有接口？此操作不可恢复。`)) {
                          void deleteProject(proj.key);
                        }
                      }}
                      title="删除项目"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </span>
                </div>
              ))}
              {filteredProjects.length === 0 && (
                <p className="col-span-full py-6 text-center text-xs text-muted-foreground">
                  {projects.length === 0 ? "该团队还没有项目，点击右上角新建" : "没有匹配的项目"}
                </p>
              )}
            </div>
          </>
        ) : (
          <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
            点击左侧团队名称查看项目（或新建一个团队）
          </div>
        )}
      </main>

      <CreateDialog
        open={showTeamDlg}
        title="新建团队"
        nameLabel="团队名"
        onClose={() => setShowTeamDlg(false)}
        onSubmit={async (name, description) => {
          await createTeam(name, description);
          setShowTeamDlg(false);
        }}
      />
      <CreateDialog
        open={showProjDlg}
        title={`在「${selectedTeam?.name ?? ""}」新建项目`}
        nameLabel="项目名"
        onClose={() => setShowProjDlg(false)}
        onSubmit={async (name, description) => {
          await createProject(name, description);
          setShowProjDlg(false);
        }}
      />
      {renameTeamTarget && (
        <CreateDialog
          open
          title="重命名团队"
          nameLabel="团队名（同时作为唯一键）"
          mode="rename"
          extraName={renameTeamTarget.name}
          confirmText="重命名"
          onClose={() => setRenameTeamTarget(null)}
          onSubmit={async (name) => {
            await renameTeam(renameTeamTarget.key, name);
            setRenameTeamTarget(null);
          }}
        />
      )}
      {renameProjTarget && (
        <CreateDialog
          open
          title="重命名项目"
          nameLabel="项目名（同时作为唯一键）"
          mode="rename"
          extraName={renameProjTarget.name}
          confirmText="重命名"
          onClose={() => setRenameProjTarget(null)}
          onSubmit={async (name) => {
            const team = selectedTeamKey;
            if (team) await renameProject(team, renameProjTarget.key, name);
            setRenameProjTarget(null);
          }}
        />
      )}
    </div>
  );
}

function TeamRow({
  team,
  active,
  onClick,
  onRename,
  onDelete,
}: {
  team: TeamInfo;
  active: boolean;
  onClick: () => void;
  onRename: () => void;
  onDelete: () => void;
}) {
  return (
    <div
      className={cn(
        "group mb-0.5 flex items-center justify-between rounded-md px-2 py-1.5 text-sm cursor-pointer transition-colors",
        active
          ? "bg-accent text-accent-foreground"
          : "text-muted-foreground hover:bg-muted hover:text-foreground",
      )}
      onClick={onClick}
    >
      <span className="truncate">{team.name}</span>
      <span className="flex shrink-0 items-center">
        <button
          className="rounded-sm p-0.5 opacity-0 transition-opacity hover:bg-border group-hover:opacity-100 cursor-pointer"
          onClick={(e) => {
            e.stopPropagation();
            onRename();
          }}
          title="重命名团队"
        >
          <Pencil className="h-3 w-3" />
        </button>
        <button
          className="rounded-sm p-0.5 opacity-0 transition-opacity hover:bg-border group-hover:opacity-100 cursor-pointer"
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
          title="删除团队"
        >
          <Trash2 className="h-3 w-3" />
        </button>
      </span>
    </div>
  );
}

function CreateDialog({
  open,
  title,
  nameLabel,
  mode = "create",
  extraName,
  confirmText,
  onClose,
  onSubmit,
}: {
  open: boolean;
  title: string;
  nameLabel: string;
  /** create：显示「名称 + 描述（可选）」，onSubmit(name, description) */
  /** rename：显示「名称」直接作为目录名，onSubmit(newName, "") */
  mode?: "create" | "rename";
  extraName?: string;
  confirmText?: string;
  onClose: () => void;
  onSubmit: (a: string, b: string) => Promise<void>;
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
            placeholder="显示名称（可为中文）"
          />
        </div>
        {mode === "create" ? (
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">描述（可选）</label>
            <textarea
              className="h-20 w-full resize-y rounded-md border border-border bg-muted p-2 text-sm text-foreground placeholder:text-muted-foreground outline-none focus:border-ring"
              value={desc}
              onChange={(e) => setDesc(e.target.value)}
              placeholder="一句说明这个团队 / 项目的用途"
            />
          </div>
        ) : (
          <p className="text-[11px] text-muted-foreground">名称将同时作为唯一键，禁止包含 \ / : * ? " &lt; &gt; | 等特殊字符。</p>
        )}
        {err && <p className="text-xs text-red-400">{err}</p>}
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            取消
          </Button>
          <Button onClick={submit} disabled={busy}>
            {confirmText ?? "创建"}
          </Button>
        </DialogFooter>
      </div>
    </Dialog>
  );
}