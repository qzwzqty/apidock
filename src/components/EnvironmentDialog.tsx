import { useEffect, useState } from "react";
import { Plus, Save, Trash2, Variable, SlidersHorizontal } from "lucide-react";
import type { EnvironmentFile, EnvironmentSummary, GlobalParams, KeyValue, ProjectSettings } from "@/lib/api";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogFooter } from "@/components/ui/dialog";

/** 左侧选中项：全局变量 / 全局参数 / 某个环境 */
type Target =
  | { kind: "gv" }
  | { kind: "gp"; tab: keyof GlobalParams }
  | { kind: "env"; id: string };

const GP_TABS: { key: keyof GlobalParams; label: string }[] = [
  { key: "headers", label: "Header" },
  { key: "cookies", label: "Cookie" },
  { key: "query", label: "Query" },
];

export function EnvironmentDialog({
  teamKey,
  projectKey,
  activeId,
  open,
  onClose,
  onChanged,
}: {
  teamKey: string;
  projectKey: string;
  activeId: string;
  open: boolean;
  onClose: () => void;
  onChanged: (activeId: string) => void;
}) {
  const [summary, setSummary] = useState<EnvironmentSummary[]>([]);
  const [envs, setEnvs] = useState<Record<string, EnvironmentFile>>({});
  const [settings, setSettings] = useState<ProjectSettings | null>(null);
  const [target, setTarget] = useState<Target>({ kind: "env", id: "" });
  /** 已修改待保存的目标键：gv / gp / env:<id> */
  const [dirtyKeys, setDirtyKeys] = useState<Set<string>>(() => new Set());
  const [err, setErr] = useState("");
  const [saving, setSaving] = useState(false);

  const createDirty = (newKey: string) =>
    setDirtyKeys((s) => new Set([...s, newKey]));

  const reload = async () => {
    const [list, st] = await Promise.all([
      api.listEnvironments(teamKey, projectKey),
      api.getProjectSettings(teamKey, projectKey),
    ]);
    const map: Record<string, EnvironmentFile> = {};
    for (const s of list) {
      map[s.id] = await api.getEnvironment(teamKey, projectKey, s.id);
    }
    setSummary(list);
    setEnvs(map);
    setSettings(st);
    // 打开时选中当前激活环境；不存在则选第一个
    setTarget({ kind: "env", id: map[activeId] ? activeId : (list[0]?.id ?? "") });
    setDirtyKeys(new Set());
    setErr("");
  };

  useEffect(() => {
    if (open) void reload();
  }, [open]);

  /** 当前选中的目标键（保存用） */
  const dirtyKey = target.kind === "env" ? `env:${target.id}` : target.kind;
  const currentDirty = dirtyKeys.has(dirtyKey);

  const setEnv = (id: string, patch: Partial<EnvironmentFile>) => {
    setEnvs((m) => ({ ...m, [id]: { ...m[id], ...patch } }));
    createDirty(`env:${id}`);
  };

  const patchGlobalVars = (g: KeyValue[]) => {
    setSettings((s) => (s ? { ...s, globalVariables: g } : s));
    createDirty("gv");
  };

  const patchGlobalParams = (tab: keyof GlobalParams, list: KeyValue[]) => {
    setSettings((s) => (s ? { ...s, globalParams: { ...s.globalParams, [tab]: list } } : s));
    createDirty("gp");
  };

  /** 保存当前选中项（不再切换标签 => 保存按钮始终针对"当前选中的这个环境/全局项"），成功后清除该目标脏标记 */
  const saveCurrent = async (): Promise<boolean> => {
    if (!settings) return false;
    setSaving(true);
    setErr("");
    try {
      if (target.kind === "gv") {
        await api.saveProjectSettings(teamKey, projectKey, settings);
      } else if (target.kind === "gp") {
        await api.saveProjectSettings(teamKey, projectKey, settings);
      } else {
        const env = envs[target.id];
        if (env) await api.saveEnvironment(teamKey, projectKey, env);
      }
      setDirtyKeys((s) => {
        const n = new Set(s);
        n.delete(dirtyKey);
        return n;
      });
      return true;
    } catch (e) {
      setErr(String(e));
      return false;
    } finally {
      setSaving(false);
    }
  };

  const addEnv = () => {
    const now = Date.now();
    const id = `env-${now}`;
    const env: EnvironmentFile = { version: 1, id, file: id, name: "新环境", host: "", builtin: false, variables: [] };
    setEnvs((m) => ({ ...m, [id]: env }));
    setSummary((l) => [...l, { id, file: id, name: "新环境", host: "", builtin: false, active: false }]);
    setTarget({ kind: "env", id });
    createDirty(`env:${id}`);
  };

  const removeEnv = async (s: EnvironmentSummary) => {
    if (s.builtin || !window.confirm(`删除环境「${s.name}」？此操作不可恢复。`)) return;
    try {
      await api.deleteEnvironment(teamKey, projectKey, s.id);
      void reload();
    } catch (e) {
      setErr(String(e));
    }
  };

  const activate = async (id: string) => {
    await api.setActiveEnvironment(teamKey, projectKey, id);
    onChanged(id);
    void reload();
  };

  const selectedEnv = target.kind === "env" ? envs[target.id] : undefined;

  return (
    <Dialog open={open} onClose={onClose} title="环境设置" className="w-[860px]">
      <div className="flex h-[520px]">
        {/* 左侧：全局 + 环境列表 */}
        <div className="flex w-52 shrink-0 flex-col border-r border-border pr-2">
          <p className="mb-1 px-2 text-[11px] font-medium text-muted-foreground">全局</p>
          <SideItem
            icon={<Variable className="h-4 w-4" />}
            label="全局变量"
            active={target.kind === "gv"}
            onClick={() => setTarget({ kind: "gv" })}
            dirty={dirtyKeys.has("gv")}
          />
          <SideItem
            icon={<SlidersHorizontal className="h-4 w-4" />}
            label="全局参数"
            active={target.kind === "gp"}
            onClick={() =>
              setTarget({
                kind: "gp",
                tab: dirtyKeys.has("gp") ? (target.kind === "gp" ? target.tab : "headers") : "headers",
              })
            }
            dirty={dirtyKeys.has("gp")}
          />

          <div className="mt-3 flex items-center justify-between px-2">
            <p className="text-[11px] font-medium text-muted-foreground">环境</p>
            <span className="text-[10px] text-muted-foreground/60">共 {summary.length} 个</span>
          </div>
          <div className="mt-1 flex-1 space-y-0.5 overflow-y-auto">
            {summary.map((s) => (
              <div
                key={s.id}
                className={cn(
                  "group flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors",
                  target.kind === "env" && target.id === s.id
                    ? "bg-accent/15 text-accent"
                    : "text-foreground hover:bg-muted",
                )}
                onClick={() => setTarget({ kind: "env", id: s.id })}
                title={dirtyKeys.has(`env:${s.id}`) ? "有未保存的修改" : s.name}
              >
                <span
                  className={cn(
                    "h-2 w-2 shrink-0 rounded-full",
                    s.id === activeId ? "bg-green-500" : "bg-border",
                  )}
                  title={s.id === activeId ? "当前生效" : "未生效"}
                />
                <span className="min-w-0 flex-1 truncate">{s.name}</span>
                {dirtyKeys.has(`env:${s.id}`) && <span className="shrink-0 text-[10px] text-amber-400">●</span>}
                {!s.builtin && (
                  <button
                    className="shrink-0 rounded-sm p-0.5 text-muted-foreground opacity-0 transition-opacity hover:text-red-400 group-hover:opacity-100"
                    title="删除环境"
                    onClick={(e) => {
                      e.stopPropagation();
                      void removeEnv(s);
                    }}
                  >
                    <Trash2 className="h-3 w-3" />
                  </button>
                )}
              </div>
            ))}
            <button
              className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              onClick={addEnv}
            >
              <Plus className="h-3.5 w-3.5" /> 新建环境
            </button>
          </div>
        </div>

        {/* 右侧：当前选中内容 */}
        <div className="flex min-w-0 flex-1 flex-col pl-4">
          {target.kind === "gv" && settings && (
            <div>
              <p className="mb-2 text-xs text-muted-foreground">
                全局变量：参与 {"{{var}}"} 替换，优先级：接口级 &gt; 全局 &gt; 环境
              </p>
              <KvEditor rows={settings.globalVariables} onChange={patchGlobalVars} placeholderK="变量名" placeholderV="值" />
            </div>
          )}

          {target.kind === "gp" && settings && (
            <GlobalParamsEditor
              settings={settings}
              target={target}
              onPatch={patchGlobalParams}
              onTabChange={(tab) => setTarget({ kind: "gp", tab })}
            />
          )}

          {target.kind === "env" && selectedEnv && (
            <div className="space-y-4">
              <div className="flex items-center gap-2.5">
                <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-accent/15 text-sm font-bold text-accent">
                  {selectedEnv.name.slice(0, 2) || "环"}
                </span>
                <Input
                  className="h-8 flex-1 font-medium"
                  value={selectedEnv.name}
                  onChange={(e) => setEnv(selectedEnv.id, { name: e.target.value })}
                  placeholder="环境名称"
                  disabled={selectedEnv.builtin}
                />
                {selectedEnv.builtin && (
                  <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">内置</span>
                )}
                {selectedEnv.id === activeId ? (
                  <span className="shrink-0 rounded bg-green-500/15 px-1.5 py-0.5 text-[10px] text-green-500">当前生效</span>
                ) : (
                  <Button size="sm" variant="outline" onClick={() => void activate(selectedEnv.id)}>
                    设为当前
                  </Button>
                )}
              </div>

              <label className="flex items-baseline gap-3">
                <span className="w-16 shrink-0 text-xs text-muted-foreground">host</span>
                <Input
                  className="h-8"
                  value={selectedEnv.host}
                  onChange={(e) => setEnv(selectedEnv.id, { host: e.target.value })}
                  placeholder="https://api.example.com（可在接口中以 {{host}} 引用）"
                />
              </label>

              <div>
                <p className="mb-1.5 text-xs text-muted-foreground">环境变量（参与 {"{{var}}"} 替换）</p>
                <KvEditor
                  rows={selectedEnv.variables}
                  onChange={(variables) => setEnv(selectedEnv.id, { variables })}
                  placeholderK="变量名"
                  placeholderV="值"
                />
              </div>
            </div>
          )}
        </div>
      </div>

      {err && <p className="mt-2 text-xs text-red-400">{err}</p>}

      <DialogFooter>
        <Button
          variant="outline"
          onClick={async () => {
            if (await saveCurrent()) onClose();
          }}
          disabled={!currentDirty || saving}
        >
          保存并关闭
        </Button>
        <Button onClick={() => void saveCurrent()} disabled={!currentDirty || saving}>
          <Save className="h-4 w-4" /> 保存
        </Button>
      </DialogFooter>
    </Dialog>
  );
}

function SideItem({
  icon,
  label,
  active,
  dirty,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  active: boolean;
  dirty: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={cn(
        "flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 text-sm transition-colors",
        active ? "bg-accent/15 text-accent" : "text-foreground hover:bg-muted",
      )}
      onClick={onClick}
    >
      {icon}
      <span className="flex-1 text-left">{label}</span>
      {dirty && <span className="text-[10px] text-amber-400">●</span>}
    </button>
  );
}

function GlobalParamsEditor({
  settings,
  target,
  onPatch,
  onTabChange,
}: {
  settings: ProjectSettings;
  target: Extract<Target, { kind: "gp" }>;
  onPatch: (tab: keyof GlobalParams, list: KeyValue[]) => void;
  onTabChange: (tab: keyof GlobalParams) => void;
}) {
  const tab = target.tab;
  const list = settings.globalParams[tab];
  const placeholders: Record<keyof GlobalParams, [string, string]> = {
    headers: ["Header 名", "值（支持 {{var}}）"],
    cookies: ["Cookie 名", "值（支持 {{var}}）"],
    query: ["参数名", "值（支持 {{var}}）"],
  };
  return (
    <div>
      <p className="mb-2 text-xs text-muted-foreground">全局参数：向每个请求注入以下内容</p>
      <div className="mb-3 flex items-center gap-1 border-b border-border">
        {GP_TABS.map((t) => (
          <button
            key={t.key}
            className={cn(
              "h-8 cursor-pointer border-b-2 px-3 text-sm transition-colors",
              tab === t.key
                ? "border-accent text-accent"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
            onClick={() => onTabChange(t.key)}
          >
            {t.label}
          </button>
        ))}
      </div>
      <KvEditor
        rows={list}
        onChange={(l) => onPatch(tab, l)}
        placeholderK={placeholders[tab][0]}
        placeholderV={placeholders[tab][1]}
      />
    </div>
  );
}

function KvEditor({
  rows,
  onChange,
  placeholderK = "Key",
  placeholderV = "Value",
}: {
  rows: KeyValue[];
  onChange: (rows: KeyValue[]) => void;
  placeholderK?: string;
  placeholderV?: string;
}) {
  const setRow = (i: number, patch: Partial<KeyValue>) =>
    onChange(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  return (
    <div className="space-y-1">
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <span className="w-5 shrink-0" />
        <span className="flex-1">{placeholderK}</span>
        <span className="flex-1">{placeholderV}</span>
        <span className="w-8 shrink-0" />
      </div>
      {rows.map((row, i) => (
        <div key={i} className="flex items-center gap-1.5">
          <input type="checkbox" className="h-3.5 w-3.5 shrink-0 cursor-pointer accent-(--ring)"
            checked={row.enabled}
            onChange={(e) => setRow(i, { enabled: e.target.checked })} />
          <Input className="h-7 flex-1" placeholder={placeholderK} value={row.key}
            onChange={(e) => setRow(i, { key: e.target.value })} />
          <Input className="h-7 flex-1" placeholder={placeholderV} value={row.value}
            onChange={(e) => setRow(i, { value: e.target.value })} />
          <Button size="icon" variant="ghost" title="删除" onClick={() => onChange(rows.filter((_, j) => j !== i))}>
            ×
          </Button>
        </div>
      ))}
      <Button size="sm" variant="ghost" onClick={() => onChange([...rows, { key: "", value: "", enabled: true }])}>
        + 添加
      </Button>
    </div>
  );
}