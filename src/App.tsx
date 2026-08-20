import { useEffect } from "react";
import { TabBar } from "@/components/TabBar";
import { MainWindow } from "@/components/MainWindow";
import { ProjectPage } from "@/components/ProjectPage";
import { useWorkspace, MAIN_TAB_ID } from "@/lib/workspace";

function App() {
  const bootstrapped = useWorkspace((s) => s.bootstrapped);
  const init = useWorkspace((s) => s.init);
  const openTabs = useWorkspace((s) => s.openTabs);
  const activeTab = useWorkspace((s) => s.activeTab);
  const error = useWorkspace((s) => s.error);

  useEffect(() => {
    void init();
  }, [init]);

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
    <div className="flex h-full flex-col">
      <TabBar />
      <div className="min-h-0 flex-1 overflow-hidden">
        {error ? (
          <div className="flex h-full items-center justify-center flex-col gap-2 text-sm text-red-400">
            <span>初始化失败</span>
            <span className="text-xs text-muted-foreground">{error}</span>
          </div>
        ) : active ? (
          <ProjectPage teamKey={active.teamKey} projectKey={active.projectKey} />
        ) : activeTab === MAIN_TAB_ID ? (
          <MainWindow />
        ) : (
          <MainWindow />
        )}
      </div>
    </div>
  );
}

export default App;