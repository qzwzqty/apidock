import { useEffect, useState } from "react";
import { Save } from "lucide-react";
import type { ProxyConfig } from "@/lib/api";
import { useWorkspace } from "@/lib/workspace";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogFooter } from "@/components/ui/dialog";

export function WorkspaceSettingsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const proxy = useWorkspace((s) => s.proxy);
  const saveProxy = useWorkspace((s) => s.saveProxy);
  const [enabled, setEnabled] = useState(proxy.enabled);
  const [kind, setKind] = useState(proxy.kind);
  const [url, setUrl] = useState(proxy.url);

  useEffect(() => {
    if (open) {
      setEnabled(proxy.enabled);
      setKind(proxy.kind);
      setUrl(proxy.url);
    }
  }, [open, proxy]);

  const save = async () => {
    const next: ProxyConfig = { enabled, kind, url: url.trim() };
    await saveProxy(next);
    onClose();
  };

  return (
    <Dialog open={open} onClose={onClose} title="工作区设置 · 代理（全局生效）">
      <div className="space-y-3">
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
          启用代理
        </label>
        {enabled && (
          <div className="space-y-3">
            <select
              className="h-8 w-full rounded-md border border-border bg-muted px-2 text-sm outline-none cursor-pointer"
              value={kind}
              onChange={(e) => setKind(e.target.value)}
            >
              <option value="system">系统代理</option>
              <option value="custom">自定义代理</option>
            </select>
            {kind === "custom" && (
              <div>
                <label className="mb-1 block text-xs text-muted-foreground">代理地址</label>
                <Input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="http://127.0.0.1:7890" />
              </div>
            )}
          </div>
        )}
        <p className="text-xs text-muted-foreground">
          默认关闭代理（直连）。Apidock 自身不联网，此配置仅影响你测试目标接口时的出站连接。
        </p>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>取消</Button>
          <Button onClick={() => void save()}>
            <Save className="h-4 w-4" /> 保存
          </Button>
        </DialogFooter>
      </div>
    </Dialog>
  );
}