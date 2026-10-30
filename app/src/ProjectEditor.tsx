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

/** Vrai si un canal dit déjà « from couvre to ». */
function covers(p: Project, from: number, to: number): boolean {
  return p.channels.some((c) => c.covered === to && c.covering.includes(from));
}

/** Un lien dans les deux sens (A couvre B et B couvre A) n'est pas permis. */
function reverseOf(p: Project, from: number, to: number): string | null {
  return from !== to && covers(p, to, from)
    ? `« ${p.nodes[to].name} » couvre déjà « ${p.nodes[from].name} » : un lien dans les deux sens n'est pas permis.`
    : null;
}

/**
 * Ajoute le canal « from couvre to » s'il n'existe pas déjà (from = to : lien vers soi-même),
 * sauf si le lien inverse existe.
 */
function addChannel(p: Project, from: number, to: number): Project {
  if (!covers(p, from, to) && !reverseOf(p, from, to)) {
    p.channels.push({ kind: "cover", covered: to, covering: [from], category: null } as Channel);
  }
  return p;
}

/** Icônes de la barre d'outils du diagramme (trait, 20 × 20). */
const ICONS = {
  folder: (
    <>
      <path d="M2.5 5.5h5l1.5 2h8.5v8.5h-15z" />
      <path d="M10 10v4M8 12h4" />
    </>
  ),
  document: (
    <>
      <path d="M5 2.5h6.5l3.5 3.5v11.5h-10z" />
      <path d="M11.5 2.5v3.5h3.5M10 9.5v5M7.5 12h5" />
    </>
  ),
  link: (
    <>
      <rect x="2" y="12" width="6" height="5" rx="1" />
      <rect x="12" y="3" width="6" height="5" rx="1" />
      <path d="M6 12L14 8.5M14 8.5h-3.5M14 8.5l-1.5 3" />
    </>
  ),
};

function ToolButton({
  icon,
  label,
  active,
  onClick,
}: {
  icon: keyof typeof ICONS;
  label: string;
  active?: boolean;
  onClick: () => void;
}) {
  return (
    <button className={`icon-button${active ? " active" : ""}`} title={label} aria-label={label} aria-pressed={active} onClick={onClick}>
      <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
        {ICONS[icon]}
      </svg>
    </button>
  );
}

/** Range le nœud dans un dossier (null : au niveau du projet). */
function moveTo(p: Project, i: number, target: number | null): Project {
  for (const n of p.nodes) if (isFolder(n)) n.children = n.children.filter((k) => k !== i);
  if (target !== null) (p.nodes[target] as FolderNode).children.push(i);
  return p;
}

/** Le nœud et, pour un dossier, tout ce qu'il contient. */
function subtree(p: Project, i: number): Set<number> {
  const out = new Set<number>();
  const stack = [i];
  while (stack.length && out.size <= p.nodes.length) {
    const n = stack.pop()!;
    if (out.has(n)) continue;
    out.add(n);
    const node = p.nodes[n];
    if (isFolder(node)) stack.push(...node.children);
  }
  return out;
}

/**
 * Déplace un nœud dans un autre dossier (null : niveau du projet). Les liens
 * vers ce qui est déplacé disparaissent ; ceux internes à un dossier déplacé restent.
 */
function relocate(p: Project, i: number, target: number | null): Project {
  const moved = subtree(p, i);
  p.channels = p.channels.flatMap((c) => {
    const inside = moved.has(c.covered);
    if (inside && c.covering.every((k) => moved.has(k))) return [c];
    if (inside) return [];
    const covering = c.covering.filter((k) => !moved.has(k));
    if (covering.length === 0) return [];
    return [{ ...c, covering, position: covering.length > 1 ? c.position : null }];
  });
  return moveTo(p, i, target);
}

/** Hauteur de l'arbre du projet, gardée d'une visite à l'autre. */
const TREE_KEY = "reqticraft.projectTreeHeight";
function savedTreeHeight(): number {
  try {
    const v = Number(localStorage.getItem(TREE_KEY));
    return v > 0 ? v : 240;
  } catch {
    return 240;
  }
}

/** Séparateur horizontal entre l'arbre et le formulaire, à glisser ou au clavier. */
function Splitter({ height, onResize }: { height: number; onResize: (h: number) => void }) {
  const start = useRef<{ y: number; h: number } | null>(null);
  return (
    <div
      className="splitter"
      role="separator"
      aria-orientation="horizontal"
      aria-label="Redimensionner l'arbre du projet"
      aria-valuenow={Math.round(height)}
      tabIndex={0}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        start.current = { y: e.clientY, h: height };
      }}
      onPointerMove={(e) => start.current && onResize(start.current.h + e.clientY - start.current.y)}
      onPointerUp={() => (start.current = null)}
      onKeyDown={(e) => {
        if (e.key === "ArrowUp") onResize(height - 20);
        if (e.key === "ArrowDown") onResize(height + 20);
      }}
    />
  );
}

// Dossier ouvert, retrouvé par son nom quand l'éditeur est recréé après « Appliquer ».
let openFolder: string | null = null;

export function ProjectEditor({ onApplied }: { onApplied: (o: Overview) => void }) {
  const [info, setInfo] = useState<ProjectInfo | null>(null);
  const [project, setProject] = useState<Project | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [level, setLevel] = useState<number | null>(null);
  const [treeHeight, setTreeHeight] = useState(savedTreeHeight);
  const side = useRef<HTMLElement>(null);
  const resizeTree = (h: number) => {
    // L'arbre et le formulaire gardent chacun au moins 80 px.
    const max = (side.current?.clientHeight ?? 600) - 86;
    const v = Math.round(Math.min(Math.max(h, 80), Math.max(max, 80)));
    setTreeHeight(v);
    try {
      localStorage.setItem(TREE_KEY, String(v));
    } catch {
      // Stockage indisponible : la hauteur n'est pas gardée.
    }
  };
  // Canal sélectionné par son cercle de jonction.
  const [channel, setChannel] = useState<number | null>(null);
  // Création d'un lien : false hors mode, null en attente du nœud couvrant.
  const [linking, setLinking] = useState<number | null | false>(false);
  const [changed, setChanged] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Lien refusé pendant la création, affiché dans la barre du diagramme.
  const [refused, setRefused] = useState<string | null>(null);
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

  // Échap annule la création d'un lien, quel que soit l'élément actif.
  useEffect(() => {
    if (linking === false) return;
    const cancel = (e: KeyboardEvent) => e.key === "Escape" && setLinking(false);
    window.addEventListener("keydown", cancel);
    return () => window.removeEventListener("keydown", cancel);
  }, [linking]);

  // Suppr supprime le lien, le document ou le dossier sélectionné (hors champ de saisie).
  useEffect(() => {
    if (channel === null && selected === null) return;
    const del = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (e.key !== "Delete" || t?.closest("input, textarea, select, [contenteditable]")) return;
      e.preventDefault();
      if (channel !== null) removeChannel(channel);
      else if (selected !== null) confirmRemove(selected);
    };
    window.addEventListener("keydown", del);
    return () => window.removeEventListener("keydown", del);
  });

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

  // Mode lien : un clic sur le nœud couvrant, puis sur le nœud couvert.
  const linkTo = (i: number) => {
    if (linking === null) {
      setRefused(null);
      setLinking(i);
    } else if (linking !== false) {
      const why = reverseOf(project, linking, i);
      setRefused(why);
      if (!why) update((p) => addChannel(p, linking, i));
      setLinking(false);
    }
  };

  const removeChannel = (ci: number) => {
    update((p) => {
      p.channels.splice(ci, 1);
      return p;
    });
    setChannel(null);
  };

  // Lien accroché au cercle d'un canal : le nœud rejoint ses couvrants (combinaison OU).
  const linkToChannel = (ci: number) => {
    if (linking === null || linking === false) return;
    const why = reverseOf(project, linking, project.channels[ci].covered);
    setRefused(why);
    if (why) {
      setLinking(false);
      return;
    }
    update((p) => {
      const c = p.channels[ci];
      if (!c.covering.includes(linking)) {
        c.covering.push(linking);
        // Le cercle se place au barycentre des couvrants, jusqu'à ce qu'on le déplace.
        c.position = null;
      }
      return p;
    });
    setLinking(false);
    setSelected(null);
    setChannel(ci);
  };

  const selectNode = (i: number | null) => {
    setSelected(i);
    setChannel(null);
  };

  const enter = (f: number | null) => {
    openFolder = f === null ? null : project.nodes[f].name;
    setLevel(f);
    setSelected(null);
    setChannel(null);
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

  const confirmRemove = (i: number) => {
    const n = project.nodes[i];
    const question = isFolder(n)
      ? `Supprimer le dossier « ${n.name} » ? Son contenu remonte d'un niveau ; les liens vers le dossier sont supprimés.`
      : `Supprimer « ${n.name} » ? Les liens qui le touchent sont supprimés.`;
    if (window.confirm(question)) remove(i);
  };

  const node = selected !== null ? project.nodes[selected] : undefined;
  return (
    <div className="split editor">
      <div className="pane">
        <div className="toolbar">
          <h1 className="grow">Projet</h1>
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
        <div className="diagram-frame">
          <div className="diagram-tools" role="toolbar" aria-label="Outils du diagramme">
            <ToolButton icon="folder" label="Ajouter un dossier" onClick={addFolder} />
            <ToolButton icon="document" label="Ajouter un document" onClick={addDocument} />
            <ToolButton
              icon="link"
              label="Créer un lien de couverture"
              active={linking !== false}
              onClick={() => {
                setRefused(null);
                setLinking(linking === false ? null : false);
              }}
            />
            {linking === false && refused && <span className="diagram-hint error-text">{refused}</span>}
            {linking !== false && (
              <span className="diagram-hint">
                {linking === null
                  ? "Cliquez sur le nœud couvrant (bas niveau)…"
                  : `${project.nodes[linking].name} couvre… cliquez sur le nœud couvert.`}{" "}
                Échap pour annuler.
              </span>
            )}
          </div>
          <Diagram
            project={project}
            level={level}
            selected={selected}
            channel={channel}
            linking={linking}
            onLink={linkTo}
            onLinkChannel={linkToChannel}
            onCancelLink={() => setLinking(false)}
            onSelect={selectNode}
            onSelectChannel={(ci) => {
              setChannel(ci);
              setSelected(null);
            }}
            onMoveChannel={(ci, pos) =>
              update((p) => {
                p.channels[ci].position = pos;
                return p;
              })
            }
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
      </div>
      <aside className="details editor-side" ref={side}>
        <section className="side-tree" aria-label="Arbre du projet" style={{ height: treeHeight }}>
          <ProjectTree
            project={project}
            selected={selected}
            onSelect={(i) => {
              // Affiche le niveau du nœud dans le diagramme.
              const up = parentOf(project, i);
              if (up !== level) {
                openFolder = up === null ? null : project.nodes[up].name;
                setLevel(up);
              }
              selectNode(i);
            }}
            onOpen={enter}
            onMove={(i, target) => update((p) => relocate(p, i, target))}
          />
        </section>
        <Splitter height={treeHeight} onResize={resizeTree} />
        <div className="side-form">
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
          ) : channel !== null && project.channels[channel] ? (
            <ChannelForm
              key={channel}
              index={channel}
              project={project}
              update={update}
              onRemove={() => removeChannel(channel)}
            />
          ) : (
            <ChannelsForm project={project} update={update} />
          )}
        </div>
      </aside>
    </div>
  );
}

/** Arbre du projet : dossiers et documents, avec glisser-déposer entre dossiers. */
function ProjectTree({
  project,
  selected,
  onSelect,
  onOpen,
  onMove,
}: {
  project: Project;
  selected: number | null;
  onSelect: (i: number) => void;
  onOpen: (i: number) => void;
  onMove: (i: number, target: number | null) => void;
}) {
  const [dragged, setDragged] = useState<number | null>(null);
  const [over, setOver] = useState<number | "root" | null>(null);
  const roots = project.nodes.flatMap((_, i) => (parentOf(project, i) === null ? [i] : []));

  // Cible valide : un dossier hors de ce qui est déplacé, différent du dossier actuel.
  const accepts = (target: number | null) =>
    dragged !== null &&
    parentOf(project, dragged) !== target &&
    (target === null || !subtree(project, dragged).has(target));

  const dropProps = (target: number | null) => ({
    onDragOver: (e: React.DragEvent) => {
      if (!accepts(target)) return;
      e.preventDefault();
      e.stopPropagation();
      setOver(target ?? "root");
    },
    onDragLeave: () => setOver((o) => (o === (target ?? "root") ? null : o)),
    onDrop: (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (dragged !== null && accepts(target)) onMove(dragged, target);
      setDragged(null);
      setOver(null);
    },
  });

  const item = (i: number, depth: number): React.ReactNode => {
    const n = project.nodes[i];
    const folder = isFolder(n);
    return (
      <li key={i}>
        <div
          className={`tree-item${selected === i ? " selected" : ""}${over === i ? " drop" : ""}`}
          style={{ paddingLeft: 8 + depth * 16 }}
          draggable
          role="treeitem"
          aria-selected={selected === i}
          tabIndex={0}
          onDragStart={(e) => {
            e.dataTransfer.effectAllowed = "move";
            e.dataTransfer.setData("text/plain", n.name);
            setDragged(i);
          }}
          onDragEnd={() => {
            setDragged(null);
            setOver(null);
          }}
          onClick={() => onSelect(i)}
          onDoubleClick={() => folder && onOpen(i)}
          onKeyDown={(e) => e.key === "Enter" && onSelect(i)}
          {...(folder ? dropProps(i) : {})}
        >
          <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true" className="tree-icon">
            {folder ? <path d="M2.5 5.5h5l1.5 2h8.5v8.5h-15z" /> : <path d="M5 2.5h6.5l3.5 3.5v11.5h-10zM11.5 2.5v3.5h3.5" />}
          </svg>
          <span className="tree-name">{n.name}</span>
          {isDocument(n) && <span className="muted tree-type">{n.doc_type}</span>}
        </div>
        {folder && n.children.length > 0 && <ul role="group">{n.children.map((k) => item(k, depth + 1))}</ul>}
      </li>
    );
  };

  return (
    <div className="project-tree">
      <div className={`tree-root${over === "root" ? " drop" : ""}`} {...dropProps(null)}>
        {project.name || "Projet"}
      </div>
      {roots.length === 0 ? (
        <p className="muted">Projet vide.</p>
      ) : (
        <ul role="tree" aria-label="Arbre du projet">
          {roots.map((i) => item(i, 0))}
        </ul>
      )}
      <p className="muted tree-help">Glissez un document ou un dossier sur un dossier, ou sur le nom du projet.</p>
    </div>
  );
}

/** Premier niveau du projet, en lecture seule (tableau de bord) ; un clic sur un document l'ouvre. */
export function ProjectOverview({ version, onDocument }: { version: number; onDocument: (name: string) => void }) {
  const [project, setProject] = useState<Project | null>(null);
  useEffect(() => {
    call<ProjectInfo>("project")
      .then((i) => setProject(i.project))
      .catch(console.error);
  }, [version]);
  if (!project || project.nodes.length === 0) return null;
  const none = () => {};
  return (
    <Diagram
      project={project}
      level={null}
      selected={null}
      channel={null}
      linking={false}
      readOnly
      onLink={none}
      onLinkChannel={none}
      onCancelLink={none}
      onSelect={(i) => {
        const n = i === null ? null : project.nodes[i];
        if (n && isDocument(n)) onDocument(n.name);
      }}
      onSelectChannel={none}
      onMoveChannel={none}
      onOpen={none}
      onMove={none}
    />
  );
}

function Diagram({
  project,
  level,
  selected,
  channel,
  linking,
  onLink,
  onLinkChannel,
  onCancelLink,
  onSelect,
  onSelectChannel,
  onMoveChannel,
  onOpen,
  onMove,
  readOnly = false,
}: {
  project: Project;
  level: number | null;
  selected: number | null;
  channel: number | null;
  linking: number | null | false;
  onLink: (i: number) => void;
  onLinkChannel: (ci: number) => void;
  onCancelLink: () => void;
  onSelect: (i: number | null) => void;
  onSelectChannel: (ci: number) => void;
  onMoveChannel: (ci: number, pos: [number, number]) => void;
  onOpen: (i: number) => void;
  onMove: (i: number, pos: [number, number]) => void;
  /** Ni déplacement ni sélection de lien : seul le clic sur un nœud est transmis. */
  readOnly?: boolean;
}) {
  const svg = useRef<SVGSVGElement>(null);
  // Glissement d'un nœud (i) ou du cercle de jonction d'un canal (ci).
  const drag = useRef<{ i: number; dx: number; dy: number } | { ci: number } | null>(null);
  // Création d'un lien : cible survolée, position du curseur, et bouton enfoncé
  // depuis le choix de la source (glisser-relâcher) ou non (clic, puis clic).
  type Target = { node: number } | { ci: number };
  const [hover, setHover] = useState<Target | null>(null);
  const [cursor, setCursor] = useState<{ x: number; y: number } | null>(null);
  const press = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  const finish = (t: Target | null) => {
    if (t === null) onCancelLink();
    else if ("node" in t) onLink(t.node);
    else onLinkChannel(t.ci);
    press.current = null;
    setCursor(null);
  };
  const hovered = (t: Target) =>
    hover !== null && ("node" in t ? "node" in hover && hover.node === t.node : "ci" in hover && hover.ci === t.ci);
  const enterTarget = (t: Target) => () => linking !== false && setHover(t);
  // Cible sous le pointeur : un nœud, ou le cercle d'un canal une fois la source choisie.
  const targetAt = (e: React.PointerEvent): Target | null => {
    const el = (e.target as Element).closest("[data-node], [data-channel]");
    if (!el) return null;
    if (el.hasAttribute("data-node")) return { node: Number(el.getAttribute("data-node")) };
    return typeof linking === "number" ? { ci: Number(el.getAttribute("data-channel")) } : null;
  };
  const leaveTarget = (t: Target) => () => setHover((h) => (h && hovered(t) ? null : h));

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
  // Chaque canal passe par un cercle de jonction : au milieu de la flèche pour
  // un canal simple, à la position enregistrée (ou au barycentre) pour une
  // combinaison OU de plusieurs couvrants.
  const center = (i: number) => {
    const [x, y] = pos(i);
    return { x: x + W / 2, y: y + H / 2 };
  };
  const drawn = project.channels.flatMap((c, ci) => {
    const to = proxy(project, c.covered, level);
    const froms = [...new Set(c.covering.map((k) => proxy(project, k, level)))].filter(
      (f): f is number => f !== null && f !== to,
    );
    // Lien vers soi-même : le nœud se couvre lui-même (pas un canal interne à un dossier fermé).
    const self = to === c.covered && c.covering.includes(c.covered);
    if (to === null || (froms.length === 0 && !self)) return [];
    if (froms.length === 0) {
      const [x, y] = pos(to);
      const s0 = { x: x + W * 0.75, y };
      const c1 = { x: x + W * 0.75, y: y - 45 * box.k };
      const c2 = { x: x + W + 45 * box.k, y: y + H * 0.35 };
      const e = { x: x + W + 2 * box.k, y: y + H * 0.35 };
      const loop = `M${s0.x},${s0.y} C${c1.x},${c1.y} ${c2.x},${c2.y} ${e.x},${e.y}`;
      // Cercle au milieu de la courbe (t = 0,5).
      const j = {
        x: 0.125 * s0.x + 0.375 * c1.x + 0.375 * c2.x + 0.125 * e.x,
        y: 0.125 * s0.y + 0.375 * c1.y + 0.375 * c2.y + 0.125 * e.y,
      };
      return [{ ci, to, froms: [to], combined: false, j, b: center(to), loop }];
    }
    const combined = c.covering.length > 1;
    const direct = to === c.covered && c.covering.every((k) => proxy(project, k, level) === k);
    const b = center(to);
    const fx = froms.reduce((s, f) => s + center(f).x, 0) / froms.length;
    const fy = froms.reduce((s, f) => s + center(f).y, 0) / froms.length;
    const j =
      combined && direct && c.position
        ? { x: c.position[0], y: c.position[1] }
        : { x: (fx + b.x) / 2, y: (fy + b.y) / 2 };
    return [{ ci, to, froms: self ? [...froms, to] : froms, combined, j, b, loop: null as string | null }];
  });

  const toModel = (e: React.PointerEvent) => {
    const r = svg.current!.getBoundingClientRect();
    return [box.x + ((e.clientX - r.left) / r.width) * box.w, box.y + ((e.clientY - r.top) / r.height) * box.h];
  };

  return (
    <svg
      ref={svg}
      className={`diagram${linking !== false ? " linking" : ""}${readOnly ? " readonly" : ""}`}
      viewBox={`${box.x} ${box.y} ${box.w} ${box.h}`}
      role="img"
      aria-label="Diagramme du projet"
      onPointerDown={(e) => {
        if (linking === false) return;
        const [x, y] = toModel(e);
        setCursor({ x, y });
        const t = targetAt(e);
        if (linking === null) {
          // Choix de la source : un nœud.
          if (t && "node" in t) {
            onLink(t.node);
            setHover(t);
            press.current = { x: e.clientX, y: e.clientY, moved: false };
          }
        } else {
          // Second clic : sur une cible, le lien est créé ; ailleurs, il est annulé.
          finish(t);
        }
      }}
      onPointerMove={(e) => {
        if (linking !== false) {
          const [x, y] = toModel(e);
          setCursor({ x, y });
          const p = press.current;
          if (p && Math.hypot(e.clientX - p.x, e.clientY - p.y) > 4) p.moved = true;
          return;
        }
        const d = drag.current;
        if (!d) return;
        const [x, y] = toModel(e);
        if ("ci" in d) onMoveChannel(d.ci, [Math.round(x), Math.round(y)]);
        else onMove(d.i, [Math.round(x - d.dx), Math.round(y - d.dy)]);
      }}
      onPointerUp={(e) => {
        drag.current = null;
        // Glisser-relâcher : relâché sur une cible, le lien est créé ; ailleurs, annulé.
        // Sans mouvement, c'est un clic : on attend le second clic.
        const p = press.current;
        if (p) {
          press.current = null;
          if (p.moved) finish(targetAt(e));
        }
      }}
      onPointerLeave={() => {
        drag.current = null;
        setHover(null);
      }}
      onClick={(e) => e.target === svg.current && onSelect(null)}
    >
      <defs>
        <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
          <path d="M0,0 L10,5 L0,10 z" className="arrow-head" />
        </marker>
        <marker id="arrow-selected" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
          <path d="M0,0 L10,5 L0,10 z" className="ghost-head" />
        </marker>
        <marker id="arrow-ghost" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
          <path d="M0,0 L10,5 L0,10 z" className="ghost-head" />
        </marker>
      </defs>
      {drawn.map(({ ci, froms, j, b, combined, loop }) => {
        // Un clic sur le trait sélectionne le lien (zone de clic élargie).
        const pick = (e: React.PointerEvent) => {
          if (linking !== false || readOnly) return;
          e.stopPropagation();
          onSelectChannel(ci);
        };
        const marker = channel === ci ? "url(#arrow-selected)" : "url(#arrow)";
        if (loop) {
          return (
            <g key={ci} className={`channel${channel === ci ? " selected" : ""}`} onPointerDown={pick}>
              <path d={loop} className="channel-hit" fill="none" strokeWidth={12 * box.k} />
              <path d={loop} className="channel-line" fill="none" strokeWidth={2 * box.k} markerEnd={marker} />
            </g>
          );
        }
        // Arrêt de la flèche au bord du rectangle couvert.
        const dx = j.x - b.x;
        const dy = j.y - b.y;
        const t = Math.min(Math.abs((W / 2 + 4 * box.k) / (dx || 1e-6)), Math.abs((H / 2 + 4 * box.k) / (dy || 1e-6)), 1);
        return (
          <g key={ci} className={`channel${combined ? " combined" : ""}${channel === ci ? " selected" : ""}`} onPointerDown={pick}>
            {froms.map((f) => {
              const a = center(f);
              return (
                <g key={f}>
                  <line x1={a.x} y1={a.y} x2={j.x} y2={j.y} className="channel-hit" strokeWidth={12 * box.k} />
                  <line x1={a.x} y1={a.y} x2={j.x} y2={j.y} className="channel-line" strokeWidth={2 * box.k} />
                </g>
              );
            })}
            <line x1={j.x} y1={j.y} x2={b.x + dx * t} y2={b.y + dy * t} className="channel-hit" strokeWidth={12 * box.k} />
            <line
              x1={j.x}
              y1={j.y}
              x2={b.x + dx * t}
              y2={b.y + dy * t}
              className="channel-line"
              strokeWidth={2 * box.k}
              markerEnd={marker}
            />
          </g>
        );
      })}
      {shown.map((i) => {
        const n = project.nodes[i];
        const [x, y] = pos(i);
        const folder = isFolder(n);
        return (
          <g
            key={i}
            className={`node${folder ? " folder" : ""}${selected === i ? " selected" : ""}${linking === i ? " link-from" : ""}${
              linking !== false && hovered({ node: i }) ? " link-target" : ""
            }`}
            transform={`translate(${x} ${y}) scale(${box.k})`}
            tabIndex={0}
            role="button"
            aria-label={folder ? `Dossier ${n.name}` : n.name}
            onKeyDown={(e) => e.key === "Enter" && (folder ? onOpen(i) : onSelect(i))}
            onDoubleClick={() => folder && linking === false && onOpen(i)}
            data-node={i}
            onPointerEnter={enterTarget({ node: i })}
            onPointerLeave={leaveTarget({ node: i })}
            onPointerDown={(e) => {
              // En mode lien, le diagramme gère le clic.
              if (linking !== false) return;
              const [mx, my] = toModel(e);
              if (!readOnly) drag.current = { i, dx: mx - x, dy: my - y };
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
      {drawn.map(({ ci, j, combined, froms, to }) => (
        <circle
          key={`j${ci}`}
          cx={j.x}
          cy={j.y}
          r={(combined ? 7 : 5) * box.k}
          strokeWidth={2 * box.k}
          className={`junction${combined ? " combined" : ""}${channel === ci ? " selected" : ""}${
            typeof linking === "number" && hovered({ ci }) ? " link-target" : ""
          }`}
          role="button"
          aria-label={`Canal ${froms.map((f) => project.nodes[f].name).join(" OU ")} → ${project.nodes[to].name}`}
          data-channel={ci}
          onPointerEnter={() => typeof linking === "number" && setHover({ ci })}
          onPointerLeave={leaveTarget({ ci })}
          onPointerDown={(e) => {
            // En mode lien, le diagramme gère le clic.
            if (linking !== false || readOnly) return;
            e.stopPropagation();
            onSelectChannel(ci);
            if (combined) drag.current = { ci };
          }}
        />
      ))}
      {typeof linking === "number" && cursor && shown.includes(linking) && (
        <line
          x1={center(linking).x}
          y1={center(linking).y}
          x2={cursor.x}
          y2={cursor.y}
          className="ghost-line"
          strokeWidth={2 * box.k}
          strokeDasharray={`${6 * box.k} ${4 * box.k}`}
          markerEnd="url(#arrow-ghost)"
          pointerEvents="none"
        />
      )}
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
        onChange={(e) => update((p) => relocate(p, index, e.target.value === "" ? null : Number(e.target.value)))}
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

function ChannelForm({
  index,
  project,
  update,
  onRemove,
}: {
  index: number;
  project: Project;
  update: (f: (p: Project) => Project) => void;
  onRemove: () => void;
}) {
  const c = project.channels[index];
  return (
    <div className="form">
      <h2>{c.covering.length > 1 ? "Canal combiné" : "Lien de couverture"}</h2>
      <p className="muted">
        {c.covering.length > 1
          ? `Une exigence de « ${project.nodes[c.covered].name} » est couverte dès qu'un de ces nœuds la référence. Glissez le cercle pour le déplacer. `
          : ""}
        Pour ajouter un couvrant, créez un lien vers le cercle. Suppr supprime le lien.
      </p>
      <h3>Couvrants ({c.covering.length})</h3>
      <ul className="plain channel-list">
        {c.covering.map((k) => (
          <li key={k}>
            <span>{project.nodes[k].name}</span>
            <button
              className="link"
              onClick={() =>
                update((p) => {
                  const ch = p.channels[index];
                  ch.covering = ch.covering.filter((x) => x !== k);
                  if (ch.covering.length < 2) ch.position = null;
                  return p;
                })
              }
              disabled={c.covering.length < 2}
            >
              Retirer
            </button>
          </li>
        ))}
      </ul>
      <h3>Couvert</h3>
      <p>{project.nodes[c.covered].name}</p>
      <div className="actions">
        <button className="button danger" onClick={onRemove}>
          Supprimer le canal
        </button>
      </div>
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
  const add = () => update((p) => addChannel(p, covering, covered));
  const why = reverseOf(project, covering, covered);
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
            <button className="button secondary" onClick={add} disabled={covering === covered || why !== null}>
              Ajouter le canal
            </button>
          </div>
          {why && <p className="error-text">{why}</p>}
        </>
      )}
    </div>
  );
}
