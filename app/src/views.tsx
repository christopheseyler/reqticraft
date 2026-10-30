import { useEffect, useState } from "react";
import { call, desktop, errorText, pickProject, pickSavePath } from "./api";
import { Details } from "./Details";
import type { Impact as ImpactData, ImpactNode, Overview, Page, Problem, Selection, Severity } from "./types";
import { ChangeBadge, ElementLink, Meter, percent, SeverityBadge, StatusBadge } from "./ui";

type SelectFn = (document: number, element: number) => void;

// ---------------------------------------------------------------- Accueil

export function Home({
  recent,
  onOpen,
  onCreate,
  busy,
  error,
}: {
  recent: string[];
  onOpen: (path: string) => void;
  onCreate: (path: string) => void;
  busy: boolean;
  error: string | null;
}) {
  const [path, setPath] = useState("");
  const [newPath, setNewPath] = useState("");
  const browse = async () => {
    const p = await pickProject();
    if (p) onOpen(p);
  };
  const browseNew = async () => {
    const p = await pickSavePath();
    if (p) onCreate(p);
  };
  return (
    <div className="page narrow">
      <h1>Bienvenue</h1>
      <p>
        Ouvrez un projet <code>.rqtf</code> (ReqtiCraft ou Reqtify) : ReqtiCraft capture les exigences
        de chaque document, calcule la couverture et contrôle les règles.
      </p>
      <div className="open-row">
        {desktop() && (
          <button className="button" onClick={browse} disabled={busy}>
            Ouvrir un projet…
          </button>
        )}
        <form
          className="path-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (path.trim()) onOpen(path.trim());
          }}
        >
          <label className="sr-only" htmlFor="project-path">
            Chemin du projet
          </label>
          <input
            id="project-path"
            placeholder="Chemin du projet, par exemple C:\projets\thermostat.rqtf"
            value={path}
            onChange={(e) => setPath(e.target.value)}
          />
          <button className="button secondary" type="submit" disabled={busy || !path.trim()}>
            Ouvrir
          </button>
        </form>
      </div>
      <h2>Nouveau projet</h2>
      <p className="muted">
        Un projet <code>.rqtf</code> reste lisible par Reqtify ; le format natif <code>.rcproj</code> regroupe projet,
        types et filtres dans un seul fichier JSON.
      </p>
      <div className="open-row">
        {desktop() && (
          <button className="button secondary" onClick={browseNew} disabled={busy}>
            Créer un projet…
          </button>
        )}
        <form
          className="path-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (newPath.trim()) onCreate(newPath.trim());
          }}
        >
          <input
            aria-label="Chemin du nouveau projet"
            placeholder="Chemin du nouveau projet, par exemple C:\projets\tutoriel.rqtf"
            value={newPath}
            onChange={(e) => setNewPath(e.target.value)}
          />
          <button className="button secondary" type="submit" disabled={busy || !newPath.trim()}>
            Créer
          </button>
        </form>
      </div>
      {busy && <p className="muted">Analyse en cours…</p>}
      {error && <p className="error-text">{error}</p>}
      {recent.length > 0 && (
        <section>
          <h2>Projets récents</h2>
          <ul className="plain recent">
            {recent.map((r) => (
              <li key={r}>
                <button className="link" onClick={() => onOpen(r)} disabled={busy}>
                  {r}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- Tableau de bord

export function Dashboard({
  overview,
  onDocument,
  onProblems,
}: {
  overview: Overview;
  onDocument: (d: number) => void;
  onProblems: (severity: Severity) => void;
}) {
  const docs = overview.documents;
  return (
    <div className="page">
      <h1>{overview.name}</h1>
      <p className="muted path">{overview.path}</p>
      <div className="tiles">
        <Tile label="Documents" value={String(docs.length)} />
        <Tile label="Exigences" value={String(overview.requirements)} />
        <Tile
          label="Couverture"
          value={percent(overview.ratio)}
          hint={overview.coverable ? `${overview.covered} / ${overview.coverable}` : "aucun canal"}
        />
        <Tile label="Dérivées" value={String(overview.derived)} />
        <Tile label="Erreurs" value={String(overview.errors)} tone={overview.errors ? "bad" : undefined} onClick={() => onProblems("error")} />
        <Tile label="Avertissements" value={String(overview.warnings)} tone={overview.warnings ? "warn" : undefined} onClick={() => onProblems("warning")} />
      </div>

      <h2>Documents</h2>
      <table className="grid">
        <thead>
          <tr>
            <th>Document</th>
            <th>Type</th>
            <th className="num">Exigences</th>
            <th>Couverture</th>
            <th className="num">Dérivées</th>
            <th className="num">Erreurs</th>
          </tr>
        </thead>
        <tbody>
          {docs.map((d) => (
            <tr key={d.index}>
              <td>
                <button className="link" onClick={() => onDocument(d.index)}>
                  {d.name}
                </button>
                {d.changed > 0 && <span className="badge change">{d.changed} modif.</span>}
              </td>
              <td className="muted">{d.typeName}</td>
              <td className="num">{d.requirements}</td>
              <td>
                <Meter ratio={d.ratio} />
              </td>
              <td className="num">{d.derived || ""}</td>
              <td className="num">{d.errors || ""}</td>
            </tr>
          ))}
        </tbody>
      </table>

      {overview.channels.length > 0 && (
        <>
          <h2>Canaux</h2>
          <ul className="plain channels">
            {overview.channels.map((c, i) => (
              <li key={i}>
                <span>
                  {c.covering.map((k) => docs[k]?.name).join(", ")} → {docs[c.covered]?.name}
                </span>
                <Meter ratio={c.ratio} />
                <span className="muted">
                  {c.coveredCount} / {c.total}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

function Tile({
  label,
  value,
  hint,
  tone,
  onClick,
}: {
  label: string;
  value: string;
  hint?: string;
  tone?: "bad" | "warn";
  onClick?: () => void;
}) {
  const body = (
    <>
      <span className="tile-label">{label}</span>
      <span className="tile-value">{value}</span>
      {hint && <span className="tile-hint muted">{hint}</span>}
    </>
  );
  const cls = `tile${tone ? ` tone-${tone}` : ""}`;
  return onClick ? (
    <button className={`${cls} clickable`} onClick={onClick}>
      {body}
    </button>
  ) : (
    <div className={cls}>{body}</div>
  );
}

// ---------------------------------------------------------------- Couverture

const FILTERS = [
  ["all", "Tout"],
  ["requirements", "Exigences"],
  ["uncovered", "Non couvertes"],
  ["derived", "Dérivées"],
  ["changed", "Modifiées"],
  ["problems", "Avec problème"],
] as const;

const PAGE = 200;

export function Coverage({
  overview,
  document,
  setDocument,
  selection,
  onSelect,
  onImpact,
  version,
}: {
  overview: Overview;
  document: number;
  setDocument: (d: number) => void;
  selection: Selection | null;
  onSelect: SelectFn;
  onImpact: () => void;
  version: number;
}) {
  const [filter, setFilter] = useState<string>("all");
  const [text, setText] = useState("");
  const [page, setPage] = useState<Page | null>(null);
  const [limit, setLimit] = useState(PAGE);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => setLimit(PAGE), [document, filter, text]);

  useEffect(() => {
    let live = true;
    call<Page>("elements", { document, status: filter, text, limit })
      .then((p) => live && (setPage(p), setError(null)))
      .catch((e) => live && setError(errorText(e)));
    return () => {
      live = false;
    };
  }, [document, filter, text, limit, version]);

  const doc = overview.documents[document];
  return (
    <div className="split">
      <div className="pane list-pane">
        <div className="toolbar">
          <label className="sr-only" htmlFor="doc-select">
            Document
          </label>
          <select id="doc-select" value={document} onChange={(e) => setDocument(Number(e.target.value))}>
            {overview.documents.map((d) => (
              <option key={d.index} value={d.index}>
                {d.name}
              </option>
            ))}
          </select>
          {doc && <Meter ratio={doc.ratio} />}
        </div>
        <div className="toolbar">
          <div className="segmented" role="group" aria-label="Filtre">
            {FILTERS.map(([id, label]) => (
              <button key={id} className={filter === id ? "active" : ""} aria-pressed={filter === id} onClick={() => setFilter(id)}>
                {label}
              </button>
            ))}
          </div>
          <input
            className="search"
            type="search"
            placeholder="Rechercher"
            aria-label="Rechercher un élément"
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        </div>
        {error && <p className="error-text">{error}</p>}
        {page && (
          <>
            <p className="muted count">{page.total} élément(s)</p>
            <ul className="tree" role="listbox" aria-label="Éléments">
              {page.rows.map((r) => {
                const selected = selection?.document === document && selection.element === r.id;
                const structural = r.class === "section";
                return (
                  <li
                    key={r.id}
                    role="option"
                    aria-selected={selected}
                    className={`row${selected ? " selected" : ""}${structural ? " section" : ""} st-${r.status}`}
                    style={{ paddingLeft: 12 + r.depth * 16 }}
                    tabIndex={0}
                    ref={selected ? (n) => n?.scrollIntoView({ block: "nearest" }) : undefined}
                    onClick={() => onSelect(document, r.id)}
                    onKeyDown={(e) => e.key === "Enter" && onSelect(document, r.id)}
                  >
                    <span className="row-name">{r.name}</span>
                    <StatusBadge status={r.status} />
                    <ChangeBadge change={r.change} />
                    {r.problems > 0 && <span className="badge severity-error">{r.problems}</span>}
                  </li>
                );
              })}
            </ul>
            {page.total > page.rows.length && (
              <button className="button secondary more" onClick={() => setLimit(limit + PAGE)}>
                Afficher plus ({page.total - page.rows.length} restants)
              </button>
            )}
          </>
        )}
      </div>
      <Details selection={selection} version={version} onSelect={onSelect} onImpact={onImpact} />
    </div>
  );
}

// ---------------------------------------------------------------- Impact

export function Impact({
  selection,
  onSelect,
  version,
}: {
  selection: Selection | null;
  onSelect: SelectFn;
  version: number;
}) {
  const [data, setData] = useState<ImpactData | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!selection) return;
    let live = true;
    call<ImpactData>("impact", { ...selection })
      .then((d) => live && (setData(d), setError(null)))
      .catch((e) => live && setError(errorText(e)));
    return () => {
      live = false;
    };
  }, [selection, version]);

  if (!selection) {
    return (
      <div className="page narrow">
        <h1>Impact</h1>
        <p className="muted">Sélectionnez un élément dans la vue Couverture, puis revenez ici.</p>
      </div>
    );
  }
  return (
    <div className="split">
      <div className="pane">
        <h1>Impact</h1>
        {error && <p className="error-text">{error}</p>}
        {data && (
          <>
            <p>
              <strong>{data.root.name}</strong> <span className="muted">({data.root.documentName})</span>
            </p>
            <h2>En amont : ce que l'élément couvre</h2>
            <ImpactTree nodes={data.upstream} onSelect={onSelect} empty="Aucune exigence couverte." />
            <h2>En aval : ce qui dépend de l'élément</h2>
            <ImpactTree nodes={data.downstream} onSelect={onSelect} empty="Aucun élément ne le couvre." />
          </>
        )}
      </div>
      <Details selection={selection} version={version} onSelect={onSelect} />
    </div>
  );
}

function ImpactTree({ nodes, onSelect, empty }: { nodes: ImpactNode[]; onSelect: SelectFn; empty: string }) {
  if (nodes.length === 0) return <p className="muted">{empty}</p>;
  return (
    <ul className="impact">
      {nodes.map((n, i) => (
        <li key={i}>
          <ElementLink target={n} onSelect={onSelect} /> <StatusBadge status={n.status} />
          {n.cycle && <span className="badge severity-warning">cycle</span>}
          {n.children.length > 0 && <ImpactTree nodes={n.children} onSelect={onSelect} empty="" />}
        </li>
      ))}
    </ul>
  );
}

// ---------------------------------------------------------------- Problèmes

export function Problems({
  severity,
  setSeverity,
  onSelect,
  version,
}: {
  severity: Severity | "all";
  setSeverity: (s: Severity | "all") => void;
  onSelect: SelectFn;
  version: number;
}) {
  const [items, setItems] = useState<Problem[]>([]);
  const [rule, setRule] = useState("");
  useEffect(() => {
    call<Problem[]>("problems", { severity: severity === "all" ? null : severity })
      .then(setItems)
      .catch(console.error);
  }, [severity, version]);
  const rules = [...new Set(items.map((p) => p.rule))].sort();
  const shown = rule ? items.filter((p) => p.rule === rule) : items;
  return (
    <div className="page">
      <h1>Problèmes</h1>
      <div className="toolbar">
        <div className="segmented" role="group" aria-label="Sévérité">
          {(
            [
              ["all", "Tous"],
              ["error", "Erreurs"],
              ["warning", "Avertissements"],
              ["information", "Informations"],
            ] as const
          ).map(([id, label]) => (
            <button key={id} className={severity === id ? "active" : ""} aria-pressed={severity === id} onClick={() => setSeverity(id)}>
              {label}
            </button>
          ))}
        </div>
        <select aria-label="Règle" value={rule} onChange={(e) => setRule(e.target.value)}>
          <option value="">Toutes les règles</option>
          {rules.map((r) => (
            <option key={r}>{r}</option>
          ))}
        </select>
      </div>
      {shown.length === 0 ? (
        <p className="muted">Aucun problème.</p>
      ) : (
        <table className="grid">
          <thead>
            <tr>
              <th>Sévérité</th>
              <th>Règle</th>
              <th>Message</th>
              <th>Emplacement</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((p, i) => (
              <tr key={i}>
                <td>
                  <SeverityBadge severity={p.severity} />
                </td>
                <td>
                  <code>{p.rule}</code>
                </td>
                <td>{p.text}</td>
                <td>
                  {p.document !== null && p.element !== null ? (
                    <ElementLink
                      target={{
                        document: p.document,
                        element: p.element,
                        name: p.elementName ?? "",
                        documentName: p.documentName ?? "",
                        status: "none",
                      }}
                      onSelect={onSelect}
                    />
                  ) : (
                    <span className="muted">{p.documentName ?? "projet"}</span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
