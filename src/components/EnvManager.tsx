import { useEffect, useState } from "react";
import { Plus, Save, Trash2 } from "lucide-react";
import type { EnvironmentFile, EnvironmentSummary, KeyValue } from "@/lib/api";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogFooter } from "@/components/ui/dialog";

export function EnvManager({
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
  const [err, setErr] = useState("");
  const [dirty, setDirty] = useState(false);

  const reload = async () => {
    const list = await api.listEnvironments(teamKey, projectKey);
    const map: Record<string, EnvironmentFile> = {};
    for (const s of list) {
      map[s.id] = await api.getEnvironment(teamKey, projectKey, s.id);
    }
    setSummary(list);
    setEnvs(map);
    setDirty(false);
  };

  useEffect(() => {
    if (open) void reload();
  }, [open]);

  const setEnv = (id: string, patch: Partial<EnvironmentFile>) => {
    setEnvs((m) => ({ ...m, [id]: { ...m[id], ...patch } }));
    setDirty(true);
  };

  const saveAll = async () => {
    try {
      for (const e of Object.values(envs)) {
        await api.saveEnvironment(teamKey, projectKey, e);
      }
      setDirty(false);
      setErr("");
    } catch (e) {
      setErr(String(e));
    }
  };

  const addEnv = () => {
    const name = "新环境";
    const file = `env-${Date.now()}`;
    const id = `env-${Date.now()}`;
    const env: EnvironmentFile = { version: 1, id, file, name, host: "", builtin: false, variables: [] };
    setEnvs((m) => ({ ...m, [id]: env }));
    setSummary((l) => [...l, { id, file, name, host: "", builtin: false, active: false }]);
    setDirty(true);
  };

  const removeEnv = async (s: EnvironmentSummary) => {
    if (s.builtin) return;
    await api.deleteEnvironment(teamKey, projectKey, s.id);
    void reload();
  };

  const activate = async (id: string) => {
    await api.setActiveEnvironment(teamKey, projectKey, id);
    onChanged(id);
    void reload();
  };

  return (
    <Dialog open={open} onClose={onClose} title="环境管理（以项目为单位）" className="w-[680px]">
      <div className="max-h-[60vh] space-y-2 overflow-y-auto">
        {summary.map((s) => (
          <div
            key={s.id}
            className={cn(
              "flex items-start gap-2 rounded-md border border-border p-3",
              s.id === activeId && "border-accent/60",
            )}
          >
            <div className="min-w-0 flex-1 space-y-2">
              <div className="flex items-center gap-2">
                <Input
                  className={cn("h-7 font-medium", s.builtin && "bg-background")}
                  value={envs[s.id]?.name ?? ""}
                  onChange={(e) => setEnv(s.id, { name: e.target.value })}
                />
                {s.builtin && (
                  <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">内置</span>
                )}
                {s.active && (
                  <span className="shrink-0 rounded bg-accent px-1.5 py-0.5 text-[10px] text-accent-foreground">生效中</span>
                )}
                {s.id !== activeId && (
                  <Button size="sm" variant="outline" onClick={() => void activate(s.id)}>
                    设为当前
                  </Button>
                )}
                {!s.builtin && (
                  <Button size="icon" variant="ghost" onClick={() => void removeEnv(s)} title="删除环境">
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                )}
              </div>
              <div className="flex items-center gap-2">
                <label className="w-8 shrink-0 text-xs text-accent">host</label>
                <Input
                  className="h-7"
                  value={envs[s.id]?.host ?? ""}
                  onChange={(e) => setEnv(s.id, { host: e.target.value })}
                  placeholder="https://api.example.com（可在接口中以 {{host}} 引用）"
                />
              </div>
              <EnvVars rows={envs[s.id]?.variables ?? []} onChange={(variables: KeyValue[]) => setEnv(s.id, { variables })} />
            </div>
          </div>
        ))}
        {summary.length === 0 && <p className="py-3 text-center text-xs text-muted-foreground">暂无环境</p>}
      </div>
      {err && <p className="mt-2 text-xs text-red-400">{err}</p>}
      <DialogFooter>
        <Button variant="outline" onClick={() => void addEnv()}>
          <Plus className="h-4 w-4" /> 新增环境
        </Button>
        <span className="ml-auto" />
        <Button variant="outline" onClick={onClose}>关闭</Button>
        <Button onClick={() => void saveAll()} disabled={!dirty}>
          <Save className="h-4 w-4" /> 保存全部
        </Button>
      </DialogFooter>
    </Dialog>
  );
}

function EnvVars({ rows, onChange }: { rows: KeyValue[]; onChange: (rows: KeyValue[]) => void }) {
  const setRow = (i: number, patch: Partial<KeyValue>) =>
    onChange(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  return (
    <div className="space-y-1">
      {rows.map((row, i) => (
        <div key={i} className="flex items-center gap-1.5 pl-8">
          <input type="checkbox" className="h-3.5 w-3.5 cursor-pointer" checked={row.enabled}
            onChange={(e) => setRow(i, { enabled: e.target.checked })} />
          <Input className="h-7 flex-1" placeholder="变量名" value={row.key}
            onChange={(e) => setRow(i, { key: e.target.value })} />
          <Input className="h-7 flex-1" placeholder="值" value={row.value}
            onChange={(e) => setRow(i, { value: e.target.value })} />
          <Button size="icon" variant="ghost" title="删除" onClick={() => onChange(rows.filter((_, j) => j !== i))}>×</Button>
        </div>
      ))}
      <Button size="sm" variant="ghost" className="ml-8" onClick={() => onChange([...rows, { key: "", value: "", enabled: true }])}>
        + 添加变量
      </Button>
    </div>
  );
}