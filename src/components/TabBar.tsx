import { useState } from "react";
import { Home, Settings, X } from "lucide-react";
import { useWorkspace, MAIN_TAB_ID } from "@/lib/workspace";
import { WorkspaceSettingsDialog } from "@/components/WorkspaceSettingsDialog";
import { cn } from "@/lib/utils";

export function TabBar() {
  const openTabs = useWorkspace((s) => s.openTabs);
  const teams = useWorkspace((s) => s.teams);
  const activeTab = useWorkspace((s) => s.activeTab);
  const setActiveTab = useWorkspace((s) => s.setActiveTab);
  const closeTab = useWorkspace((s) => s.closeTab);
  const [showSettings, setShowSettings] = useState(false);

  const tabLabel = (teamKey: string, projectKey: string) => {
    const team = teams.find((t) => t.key === teamKey);
    return `${team?.name ?? teamKey} / ${projectKey}`;
  };

  return (
    <div className="flex h-9 shrink-0 items-stretch border-b border-border bg-muted">
      <button
        className={cn(
          "flex items-center gap-1.5 px-4 text-sm transition-colors cursor-pointer select-none border-r border-border",
          activeTab === MAIN_TAB_ID
            ? "bg-background text-accent"
            : "text-muted-foreground hover:bg-muted",
        )}
        onClick={() => setActiveTab(MAIN_TAB_ID)}
        title="主窗口"
      >
        <Home className="h-4 w-4" />
        主窗口
      </button>

      {openTabs.map((tab, i) => {
        const id = `project:${tab.teamKey}:${tab.projectKey}`;
        const active = activeTab === id;
        return (
          <div
            key={id}
            className={cn(
              "group relative flex max-w-[220px] items-center gap-1 border-r border-border px-3 cursor-pointer select-none transition-colors",
              active
                ? "bg-background text-foreground"
                : "text-muted-foreground hover:bg-muted",
            )}
            onClick={() => setActiveTab(id)}
            onMouseDown={(e) => {
              if (e.button === 1) {
                e.preventDefault();
                closeTab(id);
              }
            }}
          >
            <span
              className="truncate text-sm"
              title={`${tab.teamKey} / ${tab.projectKey}`}
            >
              {i + 1}. {tabLabel(tab.teamKey, tab.projectKey)}
            </span>
            <button
              className="ml-1 rounded-sm p-0.5 text-muted-foreground opacity-0 transition-opacity hover:bg-border hover:text-foreground group-hover:opacity-100 cursor-pointer"
              onClick={(e) => {
                e.stopPropagation();
                closeTab(id);
              }}
              title="关闭"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        );
      })}

      <button
        className="ml-auto flex self-center cursor-pointer items-center justify-center rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        onClick={() => setShowSettings(true)}
        title="工作区设置（代理）"
      >
        <Settings className="h-4 w-4" />
      </button>
      <WorkspaceSettingsDialog open={showSettings} onClose={() => setShowSettings(false)} />
    </div>
  );
}