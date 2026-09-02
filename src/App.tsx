import { useEffect, useState } from "react";
import { Boxes, History } from "lucide-react";
import { TabBar } from "@/components/TabBar";
import { MainWindow } from "@/components/MainWindow";
import { ProjectPage } from "@/components/ProjectPage";
import { RequestHistoryPage } from "@/components/RequestHistoryPage";
import { useWorkspace } from "@/lib/workspace";
import { cn } from "@/lib/utils";

/** 最左侧模块菜单：接口管理 / 请求历史 */
type Section = "api" | "history";

const SECTION_KEY = "apidock.section";

function App() {
  const bootstrapped = useWorkspace((s) => s.bootstrapped);
  const init = useWorkspace((s) => s.init);
  const openTabs = useWorkspace((s) => s.openTabs);
  const activeTab = useWorkspace((s) => s.activeTab);
  const error = useWorkspace((s) => s.error);
  const [section, setSection] = useState<Section>(
    () => (localStorage.getItem(SECTION_KEY) === "history" ? "history" : "api"),
  );

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => {
    localStorage.setItem(SECTION_KEY, section);
  }, [section]);

  if (!bootstrapped) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        正在加载…
      </div>
    );
  }

  const active = openTabs.find(
    (t) => `project:${t.teamKey}:${t.projectKey}` === activeTab,
  );

  return (
    <div className="flex h-full">
      {/* 最左侧模块菜单 */}
      <nav className="flex w-14 shrink-0 flex-col items-center gap-1 border-r border-border bg-muted py-2">
        <RailButton
          label="接口管理"
          active={section === "api"}
          onClick={() => setSection("api")}
        >
          <Boxes className="h-4.5 w-4.5" />
        </RailButton>
        <RailButton
          label="请求历史"
          active={section === "history"}
          onClick={() => setSection("history")}
        >
          <History className="h-4.5 w-4.5" />
        </RailButton>
      </nav>

      <div className="flex min-w-0 flex-1 flex-col">
        {section === "api" ? (
          <>
            <TabBar />
            <div className="min-h-0 flex-1 overflow-hidden">
              {error ? (
                <div className="flex h-full items-center justify-center flex-col gap-2 text-sm text-red-400">
                  <span>初始化失败</span>
                  <span className="text-xs text-muted-foreground">{error}</span>
                </div>
              ) : active ? (
                <ProjectPage teamKey={active.teamKey} projectKey={active.projectKey} />
              ) : (
                <MainWindow />
              )}
            </div>
          </>
        ) : (
          <RequestHistoryPage />
        )}
      </div>
    </div>
  );
}

function RailButton({
  label,
  active,
  onClick,
  children,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      className={cn(
        "flex w-12 cursor-pointer flex-col items-center gap-1 rounded-md py-1.5 transition-colors",
        active
          ? "bg-accent/15 text-accent"
          : "text-muted-foreground hover:bg-muted hover:text-foreground",
      )}
      onClick={onClick}
      title={label}
    >
      {children}
      <span className="text-[10px] leading-none">{label}</span>
    </button>
  );
}

export default App;