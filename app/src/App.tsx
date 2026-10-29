import { useCallback, useEffect, useState } from "react";
import { call, desktop, errorText, pickSavePath } from "./api";
import { FilterEditor } from "./FilterEditor";
import { Reports, Snapshots } from "./History";
import { ProjectEditor } from "./ProjectEditor";
import { TypeEditor } from "./TypeEditor";
import type { Overview, Selection, Severity } from "./types";
import { Coverage, Dashboard, Home, Impact, Problems } from "./views";

// Vues de l'espace de travail (docs/functional-spec.md §4).
const VIEWS = [
  { id: "home", label: "Accueil" },
  { id: "dashboard", label: "Tableau de bord" },
  { id: "coverage", label: "Couverture" },
  { id: "impact", label: "Impact" },
  { id: "problems", label: "Problèmes" },
  { id: "snapshots", label: "Instantanés" },
  { id: "reports", label: "Rapports" },
  { id: "project", label: "Projet" },
  { id: "types", label: "Types" },
  { id: "filters", label: "Filtres" },
] as const;

type ViewId = (typeof VIEWS)[number]["id"];

const RECENT_KEY = "reqticraft.recent";

function loadRecent(): string[] {
  try {
    return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
  } catch {
    return [];
  }
}

function saveRecent(list: string[]) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(list));
  } catch {
    // Stockage indisponible : la liste reste en mémoire.
  }
}

export function App() {
  const [view, setView] = useState<ViewId>("home");
  const [overview, setOverview] = useState<Overview | null>(null);
  const [version, setVersion] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [recent, setRecent] = useState<string[]>(loadRecent);
  const [document, setDocument] = useState(0);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [severity, setSeverity] = useState<Severity | "all">("all");
  const [changed, setChanged] = useState<string[]>([]);
  const [appVersion, setAppVersion] = useState("");

  useEffect(() => {
    call<{ version: string }>("version")
      .then((v) => setAppVersion(v.version))
      .catch(() => setAppVersion(""));
    // Projet déjà ouvert côté serveur (`reqticraft serve projet.rqtf`).
    call<Overview>("overview")
      .then((o) => {
        setOverview(o);
        setView("dashboard");
      })
      .catch(() => undefined);
  }, []);

  const open = useCallback(
    async (path: string) => {
      setBusy(true);
      setError(null);
      try {
        const o = await call<Overview>("open", { path });
        setOverview(o);
        setSelection(null);
        setDocument(0);
        setChanged([]);
        setVersion((v) => v + 1);
        setView("dashboard");
        const list = [path, ...recent.filter((r) => r !== path)].slice(0, 8);
        setRecent(list);
        saveRecent(list);
      } catch (e) {
        setError(errorText(e));
      } finally {
        setBusy(false);
      }
    },
    [recent],
  );

  const create = useCallback(
    async (path: string) => {
      setBusy(true);
      setError(null);
      try {
        const o = await call<Overview>("newProject", { path });
        setOverview(o);
        setSelection(null);
        setDocument(0);
        setVersion((v) => v + 1);
        setView("project");
        const list = [path, ...recent.filter((r) => r !== path)].slice(0, 8);
        setRecent(list);
        saveRecent(list);
      } catch (e) {
        setError(errorText(e));
      } finally {
        setBusy(false);
      }
    },
    [recent],
  );

  // Après une modification (projet, types, filtres) : nouvelle analyse.
  const applied = useCallback((o: Overview) => {
    setOverview(o);
    setSelection(null);
    setVersion((v) => v + 1);
  }, []);

  const save = useCallback(
    async (as: boolean) => {
      if (!overview) return;
      let path: string | null | undefined = undefined;
      if (as) {
        path = desktop() ? await pickSavePath(overview.path) : window.prompt("Enregistrer sous (.rqtf ou .rcproj)", overview.path);
        if (!path) return;
      }
      try {
        await call<{ path: string }>("save", path ? { path } : {});
        setOverview(await call<Overview>("overview"));
      } catch (e) {
        window.alert(errorText(e));
      }
    },
    [overview],
  );

  const reload = useCallback(async () => {
    setBusy(true);
    try {
      setOverview(await call<Overview>("reload"));
      setChanged([]);
      setVersion((v) => v + 1);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }, []);

  // Surveillance des fichiers : le cœur compare les dates de modification.
  useEffect(() => {
    if (!overview) return;
    const timer = window.setInterval(() => {
      call<string[]>("changes")
        .then(setChanged)
        .catch(() => undefined);
    }, 2000);
    return () => window.clearInterval(timer);
  }, [overview]);

  const select = useCallback((d: number, e: number) => {
    setSelection({ document: d, element: e });
    setDocument(d);
    setView((v) => (v === "impact" ? v : "coverage"));
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
            disabled={v.id !== "home" && !overview}
            onClick={() => setView(v.id)}
          >
            {v.label}
            {v.id === "problems" && overview && overview.errors > 0 && (
              <span className="nav-count">{overview.errors}</span>
            )}
          </button>
        ))}
        <div className="sidebar-foot muted">
          {overview && <div className="project-name">{overview.name}</div>}
          {appVersion && <div>Version {appVersion}</div>}
        </div>
      </nav>
      <main className="content">
        {overview && view !== "home" && (
          <div className="projectbar">
            <span className="projectbar-name">{overview.name}</span>
            {overview.dirty && <span className="badge change">modifié</span>}
            {overview.displayFilter && <span className="badge">affichage : {overview.displayFilter}</span>}
            {overview.analysisFilter && <span className="badge status-derived">analyse : {overview.analysisFilter}</span>}
            <span className="grow" />
            <button className="button secondary small" onClick={() => save(false)} disabled={!overview.dirty}>
              Enregistrer
            </button>
            <button className="button secondary small" onClick={() => save(true)}>
              Enregistrer sous…
            </button>
          </div>
        )}
        {changed.length > 0 && (
          <div className="banner" role="status">
            <span>
              {changed.length === 1 ? "Un document a été modifié" : `${changed.length} documents ont été modifiés`}{" "}
              sur le disque.
            </span>
            <button className="button" onClick={reload} disabled={busy}>
              {busy ? "Analyse…" : "Recharger"}
            </button>
          </div>
        )}
        {view === "home" || !overview ? (
          <Home recent={recent} onOpen={open} onCreate={create} busy={busy} error={error} />
        ) : view === "dashboard" ? (
          <Dashboard
            overview={overview}
            onDocument={(d) => {
              setDocument(d);
              setView("coverage");
            }}
            onProblems={(s) => {
              setSeverity(s);
              setView("problems");
            }}
          />
        ) : view === "coverage" ? (
          <Coverage
            overview={overview}
            document={document}
            setDocument={setDocument}
            selection={selection}
            onSelect={select}
            onImpact={() => setView("impact")}
            version={version}
          />
        ) : view === "impact" ? (
          <Impact selection={selection} onSelect={select} version={version} />
        ) : view === "project" ? (
          <ProjectEditor key={version} onApplied={applied} />
        ) : view === "types" ? (
          <TypeEditor onSaved={applied} />
        ) : view === "filters" ? (
          <FilterEditor onChanged={applied} />
        ) : view === "snapshots" ? (
          <Snapshots version={version} />
        ) : view === "reports" ? (
          <Reports overview={overview} />
        ) : (
          <Problems severity={severity} setSeverity={setSeverity} onSelect={select} version={version} />
        )}
      </main>
    </div>
  );
}
