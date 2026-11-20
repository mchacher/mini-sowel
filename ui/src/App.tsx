import { Sidebar } from "./components/layout/Sidebar";
import { APP_NAME } from "./constants";

export function App() {
  return (
    <div className="flex h-screen bg-background text-text">
      <Sidebar />
      <main className="flex-1 overflow-y-auto p-6">
        <h1 className="text-display font-semibold text-text">{APP_NAME}</h1>
      </main>
    </div>
  );
}
