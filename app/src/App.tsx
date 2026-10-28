import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";

// Vues prévues (docs/functional-spec.md §4). Seul l'accueil existe à l'étape M0.
const VIEWS = [
  { id: "home", label: "Accueil" },
  { id: "dashboard", label: "Tableau de bord" },
  { id: "coverage", label: "Couverture" },
  { id: "impact", label: "Impact" },
  { id: "graph", label: "Graphe" },
  { id: "requirements", label: "Exigences" },
  { id: "problems", label: "Problèmes" },
] as const;

type ViewId = (typeof VIEWS)[number]["id"];

interface AppInfo {
  version: string;
}

export function App() {
  const [view, setView] = useState<ViewId>("home");
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    // Hors de Tauri (`npm run dev` dans un navigateur), le cœur Rust est absent.
    if (isTauri()) {
      invoke<AppInfo>("app_info").then(setInfo).catch(console.error);
    }
  }, []);

  return (
    <div className="shell">
      <nav className="sidebar" aria-label="Vues">
        <div className="brand">ReqtiCraft</div>
        {VIEWS.map((v) => (
          <button
            key={v.id}
            className={v.id === view ? "nav-item active" : "nav-item"}
            aria-current={v.id === view ? "page" : undefined}
            disabled={v.id !== "home"}
            onClick={() => setView(v.id)}
          >
            {v.label}
          </button>
        ))}
      </nav>
      <main className="content">
        <h1>Bienvenue</h1>
        <p>
          Ouvrez un projet pour mesurer la couverture de vos exigences. L'ouverture de projets
          arrive avec l'étape M1.
        </p>
        <p className="muted">
          {info ? `Cœur ReqtiCraft ${info.version}` : "Cœur Rust non connecté (mode navigateur)"}
        </p>
      </main>
    </div>
  );
}
