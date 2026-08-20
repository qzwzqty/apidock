import { FilePlus2, FolderOpen, GitBranch } from "lucide-react";

export function ProjectPage({ teamKey, projectKey }: { teamKey: string; projectKey: string }) {
  return (
    <div className="flex h-full">
      {/* 左侧：接口树（M1 实现） */}
      <aside className="flex w-64 shrink-0 flex-col border-r border-border bg-muted">
        <div className="flex items-center justify-between px-3 py-2">
          <span className="text-sm font-semibold text-foreground">接口</span>
          <div className="flex items-center gap-1">
            <button
              className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground cursor-pointer"
              title="新建分组（M1）"
            >
              <FolderOpen className="h-4 w-4" />
            </button>
            <button
              className="rounded-md p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground cursor-pointer"
              title="新建接口（M1）"
            >
              <FilePlus2 className="h-4 w-4" />
            </button>
          </div>
        </div>
        <div className="flex-1 overflow-y-auto px-2 pb-2">
          <p className="px-2 py-3 text-xs text-muted-foreground">
            接口树将在 M1 实现
            <br />
            （分组可多级、接口为叶子）
          </p>
        </div>
      </aside>

      {/* 右侧：接口定义/测试区（M1/M2） */}
      <main className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b border-border px-4 py-2">
          <GitBranch className="h-4 w-4 text-muted-foreground" />
          <span className="text-sm text-foreground">{teamKey} / {projectKey}</span>
          <span className="ml-auto text-xs text-muted-foreground">环境：正式环境（M1）</span>
        </div>
        <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          接口定义与调试区将在 M1/M2 实现
        </div>
      </main>
    </div>
  );
}