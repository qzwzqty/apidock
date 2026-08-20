import { useEffect, useState } from "react";
import { Save } from "lucide-react";
import type { GlobalParams, KeyValue, ProjectSettings } from "@/lib/api";
import { api } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogFooter } from "@/components/ui/dialog";

export function ProjectSettingsDialog({
  teamKey,
  projectKey,
  open,
  onClose,
}: {
  teamKey: string;
  projectKey: string;
  open: boolean;
  onClose: () => void;
}) {
  const [settings, setSettings] = useState<ProjectSettings | null>(null);
  const [err, setErr] = useState("");
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    if (open) {
      void api.getProjectSettings(teamKey, projectKey).then((s) => setSettings(s));
    }
  }, [open, teamKey, projectKey]);

  const patchGlobals = (g: KeyValue[]) => {
    setSettings((s) => (s ? { ...s, globalVariables: g } : s));
    setDirty(true);
  };
  const patchParams = (field: keyof GlobalParams, list: KeyValue[]) => {
    setSettings((s) => (s ? { ...s, globalParams: { ...s.globalParams, [field]: list } } : s));
    setDirty(true);
  };

  const save = async () => {
    if (!settings) return;
    try {
      await api.saveProjectSettings(teamKey, projectKey, settings);
      setErr("");
      setDirty(false);
    } catch (e) {
      setErr(String(e));
    }
  };

  return (
    <Dialog open={open} onClose={onClose} title="项目设置" className="w-[720px]">
      {settings && (
        <div className="max-h-[60vh] space-y-4 overflow-y-auto">
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">
              全局变量（参与 {"{{var}}"} 替换，优先级：接口级 &gt; 全局 &gt; 环境）
            </label>
            <KvEditor rows={settings.globalVariables} onChange={patchGlobals} />
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">
              全局参数 - Header（向每个请求注入请求头）
            </label>
            <KvEditor rows={settings.globalParams.headers} onChange={(l) => patchParams("headers", l)} />
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">
              全局参数 - Cookie（注入 Cookie 头）
            </label>
            <KvEditor rows={settings.globalParams.cookies} onChange={(l) => patchParams("cookies", l)} />
          </div>
          <div>
            <label className="mb-1 block text-xs text-muted-foreground">
              全局参数 - Query（附加到每个请求的查询参数）
            </label>
            <KvEditor rows={settings.globalParams.query} onChange={(l) => patchParams("query", l)} />
          </div>
        </div>
      )}
      {err && <p className="mt-2 text-xs text-red-400">{err}</p>}
      <DialogFooter>
        <Button variant="outline" onClick={onClose}>关闭</Button>
        <Button onClick={() => void save()} disabled={!dirty || !settings}>
          <Save className="h-4 w-4" /> 保存
        </Button>
      </DialogFooter>
    </Dialog>
  );
}

function KvEditor({ rows, onChange }: { rows: KeyValue[]; onChange: (rows: KeyValue[]) => void }) {
  const setRow = (i: number, patch: Partial<KeyValue>) =>
    onChange(rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  return (
    <div className="space-y-1">
      {rows.map((row, i) => (
        <div key={i} className="flex items-center gap-1.5">
          <input type="checkbox" className="h-3.5 w-3.5 cursor-pointer" checked={row.enabled}
            onChange={(e) => setRow(i, { enabled: e.target.checked })} />
          <Input className="h-7 flex-1" placeholder="Key" value={row.key}
            onChange={(e) => setRow(i, { key: e.target.value })} />
          <Input className="h-7 flex-1" placeholder="Value" value={row.value}
            onChange={(e) => setRow(i, { value: e.target.value })} />
          <Button size="icon" variant="ghost" onClick={() => onChange(rows.filter((_, j) => j !== i))}>×</Button>
        </div>
      ))}
      <Button size="sm" variant="ghost" onClick={() => onChange([...rows, { key: "", value: "", enabled: true }])}>
        + 添加
      </Button>
    </div>
  );
}