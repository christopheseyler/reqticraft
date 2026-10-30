import { useEffect, useMemo, useRef, useState } from "react";
import { call, errorText } from "./api";
import type { Channel, DocumentNode, FolderNode, Project, ProjectInfo, ProjectNode } from "./model";
import type { Overview } from "./types";

// Éditeur de projet : diagramme des documents, des dossiers (folders) et des
// canaux, propriétés du nœud sélectionné. Un dossier s'ouvre par un double
// clic ; le fil d'Ariane permet d'en sortir. Un canal vers un dossier vaut pour
// tous ses documents. Les modifications sont appliquées
// par « Appliquer », qui relance l'analyse ; l'enregistrement se fait depuis
// la barre du projet.

const NODE_W = 170;
const NODE_H = 54;

function isDocument(n: ProjectNode): n is DocumentNode {
  return n.kind === "document";
}

function isFolder(n: ProjectNode): n is FolderNode {
  return n.kind === "folder";
}

/** Dossier qui contient le nœud, ou null au niveau du projet. */
function parentOf(p: Project, i: number): number | null {
  const f = p.nodes.findIndex((n) => isFolder(n) && n.children.includes(i));
  return f < 0 ? null : f;
}

/** Représentant du nœud au niveau affiché : lui-même ou le dossier qui le contient. */
function proxy(p: Project, i: number, level: number | null): number | null {
  let at = i;
  for (let guard = 0; guard <= p.nodes.length; guard++) {
    const up = parentOf(p, at);
    if (up === level) return at;
    if (up === null) return null;
    at = up;
  }
  return null;
}

/** Chemin des dossiers, du projet jusqu'au niveau donné. */
function trail(p: Project, level: number | null): number[] {
  const out: number[] = [];
  for (let at = level; at !== null && out.length <= p.nodes.length; at = parentOf(p, at)) out.unshift(at);
  return out;
}

function position(n: ProjectNode): [number, number] | null {
  return isDocument(n) || isFolder(n) ? n.position : null;
}

function freeName(p: Project, base: string): string {
  let name = base;
  for (let k = 2; p.nodes.some((x) => x.name === name); k++) name = `${base} ${k}`;
  return name;
}

/** Retire un nœud ; le contenu d'un dossier retiré remonte d'un niveau. */
function removeAt(p: Project, i: number): Project {
  const node = p.nodes[i];
  const up = parentOf(p, i);
  if (isFolder(node) && up !== null) (p.nodes[up] as FolderNode).children.push(...node.children);
  for (const n of p.nodes) if (isFolder(n)) n.children = n.children.filter((k) => k !== i);
  p.nodes.splice(i, 1);
  const fix = (k: number) => (k > i ? k - 1 : k);
  for (const n of p.nodes) if (isFolder(n)) n.children = n.children.map(fix);
  p.channels = p.channels
    .filter((c) => c.covered !== i)
    .map((c) => ({ ...c, covered: fix(c.covered), covering: c.covering.filter((k) => k !== i).map(fix) }))
    .filter((c) => c.covering.length > 0);
  return p;
}

/** Range le nœud dans un dossier (null : au niveau du projet). */
function moveTo(p: Project, i: number, target: number | null): Project {
  for (const n of p.nodes) if (isFolder(n)) n.children = n.children.filter((k) => k !== i);
  if (target !== null) (p.nodes[target] as FolderNode).children.push(i);
  return p;
}

// Dossier ouvert, retrouvé par son nom quand l'éditeur est recréé après « Appliquer ».
let openFolder: string | null = null;

export function ProjectEditor({ onApplied }: { onApplied: (o: Overview) => void }) {
  const [info, setInfo] = useState<ProjectInfo | null>(null);
  const [project, setProject] = useState<Project | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [level, setLevel] = useState<number | null>(null);
  const [changed, setChanged] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () =>
    call<ProjectInfo>("project")
      .then((i) => {
        setInfo(i);
        setProject(i.project);
        setChanged(false);
        const f = i.project.nodes.findIndex((n) => isFolder(n) && n.name === openFolder);
        setLevel(f < 0 ? null : f);
      })
      .catch((e) => setError(errorText(e)));
  useEffect(() => {
    load();
  }, []);

  const update = (f: (p: Project) => Project) => {
    setProject((p) => (p ? f(structuredClone(p)) : p));
    setChanged(true);
  };

  const apply = async () => {
    if (!project) return;
    setBusy(true);
    setError(null);
    try {
      onApplied(await call<Overview>("updateProject", { project }));
      await load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (!project || !info) {
    return <div className="page">{error ? <p className="error-text">{error}</p> : <p className="muted">Chargement…</p>}</div>;
  }

  const enter = (f: number | null) => {
    openFolder = f === null ? null : project.nodes[f].name;
    setLevel(f);
    setSelected(null);
  };

  // Nouveau nœud placé à droite des nœuds du niveau affiché.
  const place = (p: Project): [number, number] => {
    const xs = p.nodes.flatMap((n, i) => (parentOf(p, i) === level ? [position(n)?.[0] ?? 0] : []));
    return [(xs.length ? Math.max(...xs) : 0) + 400, 200];
  };

  const addDocument = () =>
    update((p) => {
      const n = p.nodes.length;
      p.nodes.push({
        kind: "document",
        name: freeName(p, "Document"),
        doc_type: info.types.find((t) => t.name === "Text")?.name ?? info.types[0]?.name ?? "",
        path: "",
        absolute_path: null,
        category: null,
        params: [],
        modification_docs: [],
        ignored_types: [],
        position: place(p),
      });
      moveTo(p, n, level);
      setSelected(n);
      return p;
    });

  const addFolder = () =>
    update((p) => {
      const n = p.nodes.length;
      p.nodes.push({
        kind: "folder",
        name: freeName(p, "Dossier"),
        default_type: null,
        children: [],
        position: place(p),
        size: null,
        category: null,
      });
      moveTo(p, n, level);
      setSelected(n);
      return p;
    });

  const remove = (i: number) => {
    update((p) => removeAt(p, i));
    setSelected(null);
  };

  const node = selected !== null ? project.nodes[selected] : undefined;
  return (
    <div className="split editor">
      <div className="pane">
        <div className="toolbar">
          <h1 className="grow">Projet</h1>
          <button className="button secondary" onClick={addFolder}>
            Ajouter un dossier
          </button>
          <button className="button secondary" onClick={addDocument}>
            Ajouter un document
          </button>
          <button className="button" onClick={apply} disabled={!changed || busy}>
            {busy ? "Analyse…" : "Appliquer"}
          </button>
        </div>
        {error && <p className="error-text">{error}</p>}
        <nav className="crumbs-bar" aria-label="Niveau du projet">
          <button className="link" onClick={() => enter(null)} disabled={level === null}>
            {project.name || "Projet"}
          </button>
          {trail(project, level).map((f) => (
            <span key={f}>
              {" › "}
              <button className="link" onClick={() => enter(f)} disabled={f === level}>
                {project.nodes[f].name}
              </button>
            </span>
          ))}
          {level !== null && (
            <button className="button secondary small" onClick={() => enter(parentOf(project, level))}>
              ↑ Sortir du dossier
            </button>
          )}
        </nav>
        <p className="muted">
          Glissez les nœuds pour les placer ; double-cliquez sur un dossier pour l'ouvrir. Une flèche va du nœud
          couvrant au nœud couvert ; un canal vers un dossier vaut pour tous ses documents.
          {changed && " Modifications non appliquées."}
        </p>
        <Diagram
          project={project}
          level={level}
          selected={selected}
          onSelect={setSelected}
          onOpen={enter}
          onMove={(i, pos) =>
            update((p) => {
              const n = p.nodes[i];
              if (isDocument(n) || isFolder(n)) n.position = pos;
              return p;
            })
          }
        />
      </div>
      <aside className="details">
        {selected !== null && node && isDocument(node) ? (
          <DocumentForm
            key={selected}
            node={node}
            index={selected}
            project={project}
            types={info.types.map((t) => t.name)}
            update={update}
            onRemove={() => remove(selected)}
          />
        ) : selected !== null && node && isFolder(node) ? (
          <FolderForm
            key={selected}
            node={node}
            index={selected}
            update={update}
            onOpen={() => enter(selected)}
            onRemove={() => remove(selected)}
            project={project}
          />
        ) : (
          <ChannelsForm project={project} update={update} />
        )}
      </aside>
    </div>
  );
}

function Diagram({
  project,
  level,
  selected,
  onSelect,
  onOpen,
  onMove,
}: {
  project: Project;
  level: number | null;
  selected: number | null;
  onSelect: (i: number | null) => void;
  onOpen: (i: number) => void;
  onMove: (i: number, pos: [number, number]) => void;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<{ i: number; dx: number; dy: number } | null>(null);

  const shown = project.nodes.flatMap((_, i) => (parentOf(project, i) === level ? [i] : []));

  // Les positions des projets Reqtify sont grandes (≈ 800 à 2200) : le
  // diagramme se cale sur l'ensemble des nœuds affichés.
  const pos = (i: number): [number, number] => position(project.nodes[i]) ?? [shown.indexOf(i) * 300, 100];
  const box = useMemo(() => {
    const all = shown.map(pos);
    if (all.length === 0) return { x: 0, y: 0, w: 800, h: 400, k: 1 };
    const xs = all.map((p) => p[0]);
    const ys = all.map((p) => p[1]);
    // Échelle des boîtes : lisibles quel que soit l'étendue des positions.
    const k = Math.max(1, (Math.max(...xs) - Math.min(...xs)) / 700, (Math.max(...ys) - Math.min(...ys)) / 450);
    const m = 40 * k;
    const x = Math.min(...xs) - m;
    const y = Math.min(...ys) - m;
    return {
      x,
      y,
      w: Math.max(800, Math.max(...xs) - x + NODE_W * k + m),
      h: Math.max(400, Math.max(...ys) - y + NODE_H * k + m),
      k,
    };
  }, [shown.length, level]);
  const W = NODE_W * box.k;
  const H = NODE_H * box.k;

  // Canaux vus à ce niveau : un nœud rangé dans un dossier est représenté par
  // le dossier ; les canaux internes à un dossier fermé ne sont pas tracés.
  const lines = new Map<string, [number, number]>();
  for (const c of project.channels) {
    const to = proxy(project, c.covered, level);
    for (const k of c.covering) {
      const from = proxy(project, k, level);
      if (from !== null && to !== null && from !== to) lines.set(`${from}-${to}`, [from, to]);
    }
  }

  const toModel = (e: React.PointerEvent) => {
    const r = svg.current!.getBoundingClientRect();
    return [box.x + ((e.clientX - r.left) / r.width) * box.w, box.y + ((e.clientY - r.top) / r.height) * box.h];
  };

  return (
    <svg
      ref={svg}
      className="diagram"
      viewBox={`${box.x} ${box.y} ${box.w} ${box.h}`}
      role="img"
      aria-label="Diagramme du projet"
      onPointerMove={(e) => {
        if (!drag.current) return;
        const [x, y] = toModel(e);
        onMove(drag.current.i, [Math.round(x - drag.current.dx), Math.round(y - drag.current.dy)]);
      }}
      onPointerUp={() => (drag.current = null)}
      onPointerLeave={() => (drag.current = null)}
      onClick={(e) => e.target === svg.current && onSelect(null)}
    >
      <defs>
        <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
          <path d="M0,0 L10,5 L0,10 z" className="arrow-head" />
        </marker>
      </defs>
      {[...lines].map(([key, [from, to]]) => {
        const [x1, y1] = pos(from);
        const [x2, y2] = pos(to);
        const a = { x: x1 + W / 2, y: y1 + H / 2 };
        const b = { x: x2 + W / 2, y: y2 + H / 2 };
        // Arrêt au bord du rectangle couvert.
        const dx = a.x - b.x;
        const dy = a.y - b.y;
        const t = Math.min(Math.abs((W / 2 + 4 * box.k) / (dx || 1e-6)), Math.abs((H / 2 + 4 * box.k) / (dy || 1e-6)), 1);
        return (
          <line
            key={key}
            x1={a.x}
            y1={a.y}
            x2={b.x + dx * t}
            y2={b.y + dy * t}
            className="channel-line"
            strokeWidth={2 * box.k}
            markerEnd="url(#arrow)"
          />
        );
      })}
      {shown.map((i) => {
        const n = project.nodes[i];
        const [x, y] = pos(i);
        const folder = isFolder(n);
        return (
          <g
            key={i}
            className={`node${folder ? " folder" : ""}${selected === i ? " selected" : ""}`}
            transform={`translate(${x} ${y}) scale(${box.k})`}
            tabIndex={0}
            role="button"
            aria-label={folder ? `Dossier ${n.name}` : n.name}
            onKeyDown={(e) => e.key === "Enter" && (folder ? onOpen(i) : onSelect(i))}
            onDoubleClick={() => folder && onOpen(i)}
            onPointerDown={(e) => {
              const [mx, my] = toModel(e);
              drag.current = { i, dx: mx - x, dy: my - y };
              onSelect(i);
            }}
          >
            {folder && <path d="M0,-10 L0,0 L70,0 L62,-10 Z" className="folder-tab" />}
            <rect width={NODE_W} height={NODE_H} rx={folder ? 3 : 8} />
            <text x={12} y={22} className="node-name">
              {n.name.length > 20 ? `${n.name.slice(0, 19)}…` : n.name}
            </text>
            <text x={12} y={41} className="node-type">
              {isDocument(n)
                ? n.doc_type
                : folder
                  ? `dossier · ${n.children.length} élément${n.children.length > 1 ? "s" : ""}`
                  : "sous-projet"}
            </text>
          </g>
        );
      })}
      {shown.length === 0 && (
        <text x={box.x + 40} y={box.y + 60} className="node-type">
          Dossier vide : ajoutez un document ou un dossier.
        </text>
      )}
    </svg>
  );
}

/** Choix du dossier d'un nœud, avec le chemin complet de chaque dossier. */
function FolderSelect({
  project,
  index,
  update,
}: {
  project: Project;
  index: number;
  update: (f: (p: Project) => Project) => void;
}) {
  const current = parentOf(project, index);
  // Un dossier ne peut pas être rangé dans lui-même ni dans un de ses sous-dossiers.
  const inside = (f: number) => trail(project, f).includes(index);
  const folders = project.nodes.flatMap((n, f) => (isFolder(n) && !inside(f) ? [f] : []));
  return (
    <label>
      Dossier
      <select
        value={current ?? ""}
        onChange={(e) => update((p) => moveTo(p, index, e.target.value === "" ? null : Number(e.target.value)))}
      >
        <option value="">(aucun : niveau du projet)</option>
        {folders.map((f) => (
          <option key={f} value={f}>
            {trail(project, f)
              .map((k) => project.nodes[k].name)
              .join(" › ")}
          </option>
        ))}
      </select>
    </label>
  );
}

function DocumentForm({
  node,
  index,
  project,
  types,
  update,
  onRemove,
}: {
  node: DocumentNode;
  index: number;
  project: Project;
  types: string[];
  update: (f: (p: Project) => Project) => void;
  onRemove: () => void;
}) {
  const set = (patch: Partial<DocumentNode>) =>
    update((p) => {
      Object.assign(p.nodes[index], patch);
      return p;
    });
  const covers = project.channels.filter((c) => c.covering.includes(index));
  return (
    <div className="form">
      <h2>Document</h2>
      <label>
        Nom
        <input value={node.name} onChange={(e) => set({ name: e.target.value })} />
      </label>
      <label>
        Type d'analyse
        <input list="type-names" value={node.doc_type} onChange={(e) => set({ doc_type: e.target.value })} />
        <datalist id="type-names">
          {types.map((t) => (
            <option key={t} value={t} />
          ))}
        </datalist>
      </label>
      <label>
        Chemin (relatif au projet, URL ou requête)
        <input value={node.path} onChange={(e) => set({ path: e.target.value })} />
      </label>
      <label>
        Catégorie
        <input value={node.category ?? ""} onChange={(e) => set({ category: e.target.value || null })} />
      </label>
      <FolderSelect project={project} index={index} update={update} />
      <h3>Couvre</h3>
      {covers.length === 0 ? (
        <p className="muted">Aucun document.</p>
      ) : (
        <ul className="plain">
          {covers.map((c, i) => (
            <li key={i}>{project.nodes[c.covered]?.name}</li>
          ))}
        </ul>
      )}
      <div className="actions">
        <button className="button danger" onClick={onRemove}>
          Retirer du projet
        </button>
      </div>
    </div>
  );
}

function FolderForm({
  node,
  index,
  project,
  update,
  onOpen,
  onRemove,
}: {
  node: FolderNode;
  index: number;
  project: Project;
  update: (f: (p: Project) => Project) => void;
  onOpen: () => void;
  onRemove: () => void;
}) {
  return (
    <div className="form">
      <h2>Dossier</h2>
      <label>
        Nom
        <input
          value={node.name}
          onChange={(e) =>
            update((p) => {
              p.nodes[index].name = e.target.value;
              return p;
            })
          }
        />
      </label>
      <label>
        Catégorie
        <input
          value={node.category ?? ""}
          onChange={(e) =>
            update((p) => {
              (p.nodes[index] as FolderNode).category = e.target.value || null;
              return p;
            })
          }
        />
      </label>
      <FolderSelect project={project} index={index} update={update} />
      <h3>Contenu ({node.children.length})</h3>
      {node.children.length === 0 ? (
        <p className="muted">Vide.</p>
      ) : (
        <ul className="plain">
          {node.children.map((k) => (
            <li key={k}>
              {project.nodes[k].name}{" "}
              <span className="muted">{isFolder(project.nodes[k]) ? "(dossier)" : ""}</span>
            </li>
          ))}
        </ul>
      )}
      <div className="actions">
        <button className="button" onClick={onOpen}>
          Ouvrir le dossier
        </button>
        <button className="button danger" onClick={onRemove}>
          Supprimer le dossier
        </button>
      </div>
      <p className="muted">Supprimer le dossier garde ses documents : ils remontent d'un niveau.</p>
    </div>
  );
}

function ChannelsForm({ project, update }: { project: Project; update: (f: (p: Project) => Project) => void }) {
  // Les canaux relient des documents ou des dossiers, où qu'ils soient rangés.
  const docs = project.nodes.flatMap((n, i) => (isDocument(n) || isFolder(n) ? [i] : []));
  const [covering, setCovering] = useState(docs[0] ?? 0);
  const [covered, setCovered] = useState(docs[1] ?? 0);
  const names = project.nodes.map((n) => n.name);
  const label = (i: number) => (isFolder(project.nodes[i]) ? `${names[i]} (dossier)` : names[i]);
  const add = () =>
    update((p) => {
      const existing = p.channels.find((c) => c.covered === covered && c.kind === "cover");
      if (existing) {
        if (!existing.covering.includes(covering)) existing.covering.push(covering);
      } else {
        p.channels.push({ kind: "cover", covered, covering: [covering], category: null } as Channel);
      }
      return p;
    });
  const remove = (ci: number, k: number) =>
    update((p) => {
      const c = p.channels[ci];
      c.covering = c.covering.filter((x) => x !== k);
      if (c.covering.length === 0) p.channels.splice(ci, 1);
      return p;
    });
  return (
    <div className="form">
      <h2>Canaux</h2>
      <p className="muted">Sélectionnez un document ou un dossier dans le diagramme pour modifier ses propriétés.</p>
      {project.channels.length === 0 ? (
        <p className="muted">Aucun canal.</p>
      ) : (
        <ul className="plain channel-list">
          {project.channels.flatMap((c, ci) =>
            c.covering.map((k) => (
              <li key={`${ci}-${k}`}>
                <span>
                  {label(k)} → {label(c.covered)}
                  {c.covering.length > 1 && <span className="badge">OU</span>}
                </span>
                <button className="link" onClick={() => remove(ci, k)} aria-label={`Retirer le canal ${names[k]} vers ${names[c.covered]}`}>
                  Retirer
                </button>
              </li>
            )),
          )}
        </ul>
      )}
      {docs.length >= 2 && (
        <>
          <h3>Nouveau canal</h3>
          <label>
            Couvrant
            <select value={covering} onChange={(e) => setCovering(Number(e.target.value))}>
              {docs.map((i) => (
                <option key={i} value={i}>
                  {label(i)}
                </option>
              ))}
            </select>
          </label>
          <label>
            couvre
            <select value={covered} onChange={(e) => setCovered(Number(e.target.value))}>
              {docs.map((i) => (
                <option key={i} value={i}>
                  {label(i)}
                </option>
              ))}
            </select>
          </label>
          <div className="actions">
            <button className="button secondary" onClick={add} disabled={covering === covered}>
              Ajouter le canal
            </button>
          </div>
        </>
      )}
    </div>
  );
}
