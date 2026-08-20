import { useState } from "react";
import { Save, Check } from "lucide-react";
import type { InterfaceFile } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

export function InterfaceEditor({
  doc,
  onSave,
}: {
  doc: InterfaceFile;
  onSave: (doc: InterfaceFile) => Promise<void>;
}) {
  const [local, setLocal] = useState<InterfaceFile>({ ...doc, headers: [...doc.headers], query: [...doc.query] });
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  const update = (patch: Partial<InterfaceFile>) => {
    setLocal((d) => ({ ...d, ...patch }));
    setDirty(true);
    setSaved(false);
  };

  const save = async () => {
    setSaving(true);
    try {
      await onSave(local);
      setDirty(false);
      setSaved(true);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* 请求行 */}
      <div className="flex items-center gap-2 px-4 py-3">
        <select
          className="h-9 rounded-md border border-border bg-muted px-2 text-sm font-semibold text-accent outline-none focus:border-ring cursor-pointer"
          value={local.method}
          onChange={(e) => update({ method: e.target.value })}
        >
          {METHODS.map((m) => (
            <option key={m} value={m}>{m}</option>
          ))}
        </select>
        <Input
          className="h-9"
          placeholder="请求地址，如 {{host}}/api/login"
          value={local.url}
          onChange={(e) => update({ url: e.target.value })}
        />
        <Button onClick={save} disabled={saving || (!dirty && !saved)} style={{ padding: "0 12px" }}>
          <Save className="h-4 w-4" /> 保存
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto px-4 pb-4">
        <div className="mb-3">
          <label className="mb-1 block text-xs text-muted-foreground">接口名称</label>
          <Input value={local.name} onChange={(e) => update({ name: e.target.value })} />
        </div>

        <div className="mb-3">
          <label className="mb-1 block text-xs text-muted-foreground">接口说明（支持 Markdown，M2 渲染）</label>
          <textarea
            className="h-28 w-full rounded-md border border-border bg-muted p-2.5 text-sm text-foreground placeholder:text-muted-foreground outline-none focus:border-ring resize-y"
            value={local.description}
            onChange={(e) => update({ description: e.target.value })}
          />
        </div>

        <div className="mb-3">
          <div className="mb-1 flex items-center gap-2">
            <label className="text-xs text-muted-foreground">查询参数</label>
            <Button size="sm" variant="ghost" onClick={() => update({ query: [...local.query, { key: "", value: "", enabled: true }] })}>
              + 添加
            </Button>
          </div>
          {local.query.map((row, i) => (
            <div key={i} className="mb-1 flex items-center gap-1">
              <input
                type="checkbox"
                checked={row.enabled}
                onChange={(e) => {
                  const query = local.query.map((r, j) => (j === i ? { ...r, enabled: e.target.checked } : r));
                  update({ query });
                }}
              />
              <Input className="h-7 flex-1" placeholder="key"
                value={row.key}
                onChange={(e) => {
                  const query = local.query.map((r, j) => (j === i ? { ...r, key: e.target.value } : r));
                  update({ query });
                }}
              />
              <Input className="h-7 flex-1" placeholder="value"
                value={row.value}
                onChange={(e) => {
                  const query = local.query.map((r, j) => (j === i ? { ...r, value: e.target.value } : r));
                  update({ query });
                }}
              />
              <Button size="icon" variant="ghost" onClick={() => {
                const query = local.query.filter((_, j) => j !== i);
                update({ query });
              }}>
                删除
              </Button>
            </div>
          ))}
        </div>

        {saved && !dirty && (
          <p className="flex items-center gap-1 text-xs text-green-500">
            <Check className="h-3 w-3" /> 已保存
          </p>
        )}
      </div>
    </div>
  );
}