import { useEffect, useState } from "react";
import { open as openDialog, save } from "@tauri-apps/plugin-dialog";
import { Upload, Download, Loader2, Check, AlertTriangle } from "lucide-react";
import type { ImportReport } from "@/lib/api";
import { api } from "@/lib/api";
import { useWorkspace } from "@/lib/workspace";
import { Button } from "@/components/ui/button";
import { Dialog, DialogFooter } from "@/components/ui/dialog";

export function ImportExportDialog({
  teamKey,
  projectKey,
  open,
  initialMode = "import",
  onClose,
  onImported,
}: {
  teamKey: string;
  projectKey: string;
  open: boolean;
  initialMode?: "import" | "export";
  onClose: () => void;
  onImported: () => void;
}) {
  const teams = useWorkspace((s) => s.teams);
  const [mode, setMode] = useState<"import" | "export">(initialMode);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [result, setResult] = useState<{ title: string; report?: ImportReport; warnings?: string[] } | null>(null);

  useEffect(() => {
    if (open) {
      setMode(initialMode);
      setBusy(false);
      setErr("");
      setResult(null);
    }
  }, [open, initialMode]);

  const pickFile = (): Promise<string | null> =>
    openDialog({ multiple: false, filters: [{ name: "规范文件", extensions: ["json", "yaml", "yml"] }] });

  const doImport = async (target: "project" | "team") => {
    const path = await pickFile();
    if (!path) return;
    setBusy(true);
    setErr("");
    setResult(null);
    try {
      if (target === "project") {
        const [report, name] = await api.importSpecIntoProject(path, teamKey, projectKey);
        setResult({ title: `导入到当前项目完成：${name}`, report });
      } else {
        const [report, name] = await api.importSpecNewProject(path, teams[0]?.key ?? "");
        setResult({ title: `新建项目完成：${name}`, report });
      }
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const doExport = async (yaml: boolean) => {
    const ext = yaml ? "yaml" : "json";
    const path = await save({
      defaultPath: `${projectKey}.${ext}`,
      filters: [{ name: yaml ? "OpenAPI YAML" : "OpenAPI JSON", extensions: [ext] }],
    });
    if (!path) return;
    setBusy(true);
    setErr("");
    setResult(null);
    try {
      const warnings = await api.exportOpenapiFile(path, teamKey, projectKey, yaml);
      setResult({ title: `已导出到 ${path}`, warnings });
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onClose={onClose} title="导入 / 导出" className="w-[560px]">
      <div className="mb-3 flex gap-2">
        <Button variant={mode === "import" ? "default" : "outline"} size="sm" onClick={() => { setMode("import"); setResult(null); }} disabled={busy}>
          <Upload className="h-3.5 w-3.5" /> 导入
        </Button>
        <Button variant={mode === "export" ? "default" : "outline"} size="sm" onClick={() => { setMode("export"); setResult(null); }} disabled={busy}>
          <Download className="h-3.5 w-3.5" /> 导出
        </Button>
      </div>

      {mode === "import" && (
        <div className="space-y-2">
          <p className="text-sm text-foreground">支持 OpenAPI 3.0/3.1（JSON/YAML）与 Postman Collection v2（JSON）。</p>
          <div className="flex gap-2">
            <Button onClick={() => void doImport("project")} disabled={busy}>
              <Upload className="h-4 w-4" /> 导入到当前项目
            </Button>
            <Button variant="outline" onClick={() => void doImport("team")} disabled={busy || teams.length === 0} title="以规范名新建一个项目">
              <Upload className="h-4 w-4" /> 导入并新建项目
            </Button>
          </div>
        </div>
      )}

      {mode === "export" && (
        <div className="flex gap-2">
          <Button onClick={() => void doExport(false)} disabled={busy}>
            <Download className="h-4 w-4" /> 导出 OpenAPI JSON
          </Button>
          <Button variant="outline" onClick={() => void doExport(true)} disabled={busy}>
            <Download className="h-4 w-4" /> 导出 OpenAPI YAML
          </Button>
        </div>
      )}

      {busy && <p className="mt-3 flex items-center gap-2 text-sm text-accent"><Loader2 className="h-4 w-4 animate-spin" /> 处理中…</p>}
      {err && <p className="mt-3 text-xs text-red-400">{err}</p>}
      {result && (
        <div className="mt-3 space-y-2 rounded-md border border-border bg-muted p-3">
          <p className="flex items-center gap-1.5 text-sm text-green-500">
            <Check className="h-4 w-4" /> {result.title}
          </p>
          {result.report && (
            <p className="text-xs text-muted-foreground">
              导入 {result.report.total} 条，跳过 {result.report.skipped}
              {result.report.warnings.length > 0 && `；告警 ${result.report.warnings.length} 条`}
            </p>
          )}
          {(result.report?.warnings ?? result.warnings ?? []).length > 0 && (
            <div className="max-h-40 space-y-1 overflow-y-auto text-xs">
              {(result.report ? result.report.warnings : (result.warnings ?? [])).map((w, i) => (
                <p key={i} className="flex gap-1 text-yellow-500/90">
                  <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0" /> {w}
                </p>
              ))}
            </div>
          )}
        </div>
      )}

      <DialogFooter>
        {result && (result.report || mode === "import") && (
          <Button variant="outline" onClick={onImported} className="mr-auto">
            刷新接口树
          </Button>
        )}
        <Button variant="outline" onClick={onClose}>关闭</Button>
      </DialogFooter>
    </Dialog>
  );
}