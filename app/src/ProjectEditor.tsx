import { useEffect, useMemo, useRef, useState } from "react";
import { call, errorText } from "./api";
import type { Channel, DocumentNode, Project, ProjectInfo } from "./model";
import type { Overview } from "./types";

// Éditeur de projet : diagramme des documents et des canaux, propriétés du
// document sélectionné. Les modifications sont appliquées par « Appliquer »,
// qui relance l'analyse ; l'enregistrement se fait depuis la barre du projet.

const NODE_W = 170;
const NODE_H = 54;

function isDocument(n: Project["nodes"][number]): n is DocumentNode {
  return n.kind === "document";
}

export function ProjectEditor({ onApplied }: { onApplied: (o: Overview) => void }) {
  const [info, setInfo] = useState<ProjectInfo | null>(null);
  const [project, setProject] = useState<Project | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [changed, setChanged] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () =>
    call<ProjectInfo>("project")
      .then((i) => {
        setInfo(i);
        setProject(i.project);
        setChanged(false);
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

  const addDocument = () =>
    update((p) => {
      const n = p.nodes.length;
      let name = "Document";
      for (let k = 2; p.nodes.some((x) => x.name === name); k++) name = `Document ${k}`;
      const xs = p.nodes.map((x) => (isDocument(x) && x.position ? x.position[0] : 0));
      p.nodes.push({
        kind: "document",
        name,
        doc_type: info.types.find((t) => t.name === "Text")?.name ?? info.types[0]?.name ?? "",
        path: "",
        absolute_path: null,
        category: null,
        params: [],
        modification_docs: [],
        ignored_types: [],
        position: [(xs.length ? Math.max(...xs) : 0) + 400, 200],
      });
      setSelected(n);
      return p;
    });

  const removeNode = (i: number) =>
    update((p) => {
      p.nodes.splice(i, 1);
      const fix = (k: number) => (k > i ? k - 1 : k);
      p.channels = p.channels
        .filter((c) => c.covered !== i)
        .map((c) => ({ ...c, covered: fix(c.covered), covering: c.covering.filter((k) => k !== i).map(fix) }))
        .filter((c) => c.covering.length > 0);
      setSelected(null);
      return p;
    });

  return (
    <div className="split editor">
      <div className="pane">
        <div className="toolbar">
          <h1 className="grow">Projet</h1>
          <button className="button secondary" onClick={addDocument}>
            Ajouter un document
          </button>
          <button className="button" onClick={apply} disabled={!changed || busy}>
            {busy ? "Analyse…" : "Appliquer"}
          </button>
        </div>
        {error && <p className="error-text">{error}</p>}
        <p className="muted">
          Glissez les documents pour les placer. Une flèche va du document couvrant au document couvert.
          {changed && " Modifications non appliquées."}
        </p>
        <Diagram
          project={project}
          selected={selected}
          onSelect={setSelected}
          onMove={(i, pos) =>
            update((p) => {
              const n = p.nodes[i];
              if (isDocument(n)) n.position = pos;
              return p;
            })
          }
        />
      </div>
      <aside className="details">
        {selected !== null && project.nodes[selected] && isDocument(project.nodes[selected]) ? (
          <DocumentForm
            key={selected}
            node={project.nodes[selected] as DocumentNode}
            index={selected}
            project={project}
            types={info.types.map((t) => t.name)}
            update={update}
            onRemove={() => removeNode(selected)}
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
  selected,
  onSelect,
  onMove,
}: {
  project: Project;
  selected: number | null;
  onSelect: (i: number | null) => void;
  onMove: (i: number, pos: [number, number]) => void;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<{ i: number; dx: number; dy: number } | null>(null);

  // Les positions des projets Reqtify sont grandes (≈ 800 à 2200) : le
  // diagramme se cale sur l'ensemble des documents.
  const pos = (i: number): [number, number] => {
    const n = project.nodes[i];
    return isDocument(n) && n.position ? n.position : [i * 300, 100];
  };
  const box = useMemo(() => {
    const all = project.nodes.map((_, i) => pos(i));
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
  }, [project.nodes.length]);
  const W = NODE_W * box.k;
  const H = NODE_H * box.k;

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
      {project.channels.flatMap((c, ci) =>
        c.covering.map((k) => {
          const [x1, y1] = pos(k);
          const [x2, y2] = pos(c.covered);
          const a = { x: x1 + W / 2, y: y1 + H / 2 };
          const b = { x: x2 + W / 2, y: y2 + H / 2 };
          // Arrêt au bord du rectangle couvert.
          const dx = a.x - b.x;
          const dy = a.y - b.y;
          const t = Math.min(Math.abs((W / 2 + 4 * box.k) / (dx || 1e-6)), Math.abs((H / 2 + 4 * box.k) / (dy || 1e-6)), 1);
          return (
            <line
              key={`${ci}-${k}`}
              x1={a.x}
              y1={a.y}
              x2={b.x + dx * t}
              y2={b.y + dy * t}
              className="channel-line"
              strokeWidth={2 * box.k}
              markerEnd="url(#arrow)"
            />
          );
        }),
      )}
      {project.nodes.map((n, i) => {
        const [x, y] = pos(i);
        return (
          <g
            key={i}
            className={`node${selected === i ? " selected" : ""}`}
            transform={`translate(${x} ${y}) scale(${box.k})`}
            tabIndex={0}
            role="button"
            aria-label={n.name}
            onKeyDown={(e) => e.key === "Enter" && onSelect(i)}
            onPointerDown={(e) => {
              const [mx, my] = toModel(e);
              drag.current = { i, dx: mx - x, dy: my - y };
              onSelect(i);
            }}
          >
            <rect width={NODE_W} height={NODE_H} rx={8} />
            <text x={12} y={22} className="node-name">
              {n.name.length > 20 ? `${n.name.slice(0, 19)}…` : n.name}
            </text>
            <text x={12} y={41} className="node-type">
              {isDocument(n) ? n.doc_type : n.kind === "sub_project" ? "sous-projet" : "dossier"}
            </text>
          </g>
        );
      })}
    </svg>
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

function ChannelsForm({ project, update }: { project: Project; update: (f: (p: Project) => Project) => void }) {
  const [covering, setCovering] = useState(0);
  const [covered, setCovered] = useState(1);
  const names = project.nodes.map((n) => n.name);
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
      <p className="muted">Sélectionnez un document dans le diagramme pour modifier ses propriétés.</p>
      {project.channels.length === 0 ? (
        <p className="muted">Aucun canal.</p>
      ) : (
        <ul className="plain channel-list">
          {project.channels.flatMap((c, ci) =>
            c.covering.map((k) => (
              <li key={`${ci}-${k}`}>
                <span>
                  {names[k]} → {names[c.covered]}
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
      {names.length >= 2 && (
        <>
          <h3>Nouveau canal</h3>
          <label>
            Document couvrant
            <select value={covering} onChange={(e) => setCovering(Number(e.target.value))}>
              {names.map((n, i) => (
                <option key={i} value={i}>
                  {n}
                </option>
              ))}
            </select>
          </label>
          <label>
            couvre
            <select value={covered} onChange={(e) => setCovered(Number(e.target.value))}>
              {names.map((n, i) => (
                <option key={i} value={i}>
                  {n}
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
