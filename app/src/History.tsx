import { useEffect, useState } from "react";
import { call, desktop, errorText } from "./api";
import type { Overview } from "./types";
import { percent } from "./ui";

// Instantanés (création, comparaison) et rapports.

export interface SnapshotInfo {
  id: number;
  name: string;
  created: string;
  requirements: number;
  covered: number;
  coverable: number;
  errors: number;
  warnings: number;
}

export interface ElementState {
  document: string;
  typeName: string;
  ident: string | null;
  label: string | null;
  text: string;
  status: string;
  attributes: [string, string | null][];
  references: string[];
  coveredBy: string[];
}

interface Diff {
  added: ElementState[];
  removed: ElementState[];
  modified: { before: ElementState; after: ElementState; fields: string[] }[];
  coverageLost: ElementState[];
  coverageGained: ElementState[];
  before: [number, number];
  after: [number, number];
}

const FIELD: Record<string, string> = {
  text: "texte",
  label: "libellé",
  attributes: "attributs",
  references: "références",
  status: "couverture",
};

export function stateName(e: ElementState): string {
  if (e.ident && e.label && e.ident !== e.label) return `${e.ident} ${e.label}`;
  return e.ident ?? e.label ?? e.typeName;
}

export function date(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" });
}

export function Snapshots({ version }: { version: number }) {
  const [list, setList] = useState<SnapshotInfo[]>([]);
  const [name, setName] = useState("");
  const [from, setFrom] = useState<number | null>(null);
  const [to, setTo] = useState<number | "current">("current");
  const [diff, setDiff] = useState<Diff | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = () =>
    call<SnapshotInfo[]>("snapshots")
      .then((l) => {
        setList(l);
        setFrom((f) => f ?? l[l.length - 1]?.id ?? null);
      })
      .catch((e) => setError(errorText(e)));
  useEffect(() => {
    load();
  }, [version]);

  const create = async () => {
    setError(null);
    try {
      const s = await call<SnapshotInfo>("createSnapshot", { name });
      setName("");
      setFrom(s.id);
      await load();
    } catch (e) {
      setError(errorText(e));
    }
  };

  const compare = async () => {
    if (from === null) return;
    setError(null);
    try {
      setDiff(await call<Diff>("compare", { from, to: to === "current" ? null : to }));
    } catch (e) {
      setError(errorText(e));
    }
  };

  const remove = async (id: number) => {
    if (!window.confirm("Supprimer cet instantané ?")) return;
    await call("deleteSnapshot", { id }).catch((e) => setError(errorText(e)));
    if (from === id) setFrom(null);
    setDiff(null);
    await load();
  };

  return (
    <div className="page">
      <h1>Instantanés</h1>
      <p className="muted">
        Un instantané enregistre l'état de l'analyse (exigences, textes, attributs, couverture) dans la base{" "}
        <code>.rcsnap</code> du projet, pour le comparer plus tard.
      </p>
      <form
        className="path-form narrow-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim()) create();
        }}
      >
        <input aria-label="Nom de l'instantané" placeholder="Nom, par exemple V1" value={name} onChange={(e) => setName(e.target.value)} />
        <button className="button" type="submit" disabled={!name.trim()}>
          Créer l'instantané
        </button>
      </form>
      {error && <p className="error-text">{error}</p>}
      {list.length > 0 && (
        <table className="grid">
          <thead>
            <tr>
              <th>Nom</th>
              <th>Date</th>
              <th className="num">Exigences</th>
              <th>Couverture</th>
              <th className="num">Erreurs</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {list.map((s) => (
              <tr key={s.id}>
                <td>{s.name}</td>
                <td className="muted">{date(s.created)}</td>
                <td className="num">{s.requirements}</td>
                <td>
                  {percent(s.coverable ? s.covered / s.coverable : null)}{" "}
                  <span className="muted">
                    ({s.covered}/{s.coverable})
                  </span>
                </td>
                <td className="num">{s.errors}</td>
                <td>
                  <button className="link" onClick={() => remove(s.id)}>
                    Supprimer
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {list.length > 0 && (
        <>
          <h2>Comparer</h2>
          <div className="toolbar">
            <select aria-label="Instantané de départ" value={from ?? ""} onChange={(e) => setFrom(Number(e.target.value))}>
              {list.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
            <span>→</span>
            <select
              aria-label="Comparé à"
              value={to}
              onChange={(e) => setTo(e.target.value === "current" ? "current" : Number(e.target.value))}
            >
              <option value="current">État courant</option>
              {list.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
            <button className="button" onClick={compare} disabled={from === null}>
              Comparer
            </button>
          </div>
        </>
      )}
      {diff && <DiffView diff={diff} />}
    </div>
  );
}

function DiffView({ diff }: { diff: Diff }) {
  const ratio = ([c, t]: [number, number]) => `${percent(t ? c / t : null)} (${c}/${t})`;
  const block = (title: string, items: ElementState[], tone?: string) =>
    items.length > 0 && (
      <section>
        <h3>
          {title} ({items.length})
        </h3>
        <ul className="plain">
          {items.map((e, i) => (
            <li key={i}>
              <span className="muted">{e.document} › </span>
              <span className={tone}>{stateName(e)}</span>
            </li>
          ))}
        </ul>
      </section>
    );
  const empty =
    diff.added.length + diff.removed.length + diff.modified.length === 0;
  return (
    <div className="diff">
      <p>
        Couverture : {ratio(diff.before)} → <strong>{ratio(diff.after)}</strong>
      </p>
      {empty && <p className="muted">Aucune différence.</p>}
      {block("Ajoutées", diff.added)}
      {block("Supprimées", diff.removed, "error-text")}
      {diff.modified.length > 0 && (
        <section>
          <h3>Modifiées ({diff.modified.length})</h3>
          <ul className="plain">
            {diff.modified.map((m, i) => (
              <li key={i} className="modified">
                <div>
                  <span className="muted">{m.after.document} › </span>
                  {stateName(m.after)} <span className="badge change">{m.fields.map((f) => FIELD[f] ?? f).join(", ")}</span>
                </div>
                {m.fields.includes("text") && (
                  <div className="text-diff">
                    <del>{m.before.text}</del>
                    <ins>{m.after.text}</ins>
                  </div>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}
      {block("Couvertures perdues", diff.coverageLost, "error-text")}
      {block("Couvertures gagnées", diff.coverageGained)}
    </div>
  );
}

interface Templates {
  templates: { id: string; label: string }[];
  formats: string[];
}

const FORMAT_LABEL: Record<string, string> = {
  html: "HTML",
  xlsx: "Excel (XLSX)",
  docx: "Word (DOCX)",
  csv: "CSV",
  md: "Markdown",
};

export function Reports({ overview }: { overview: Overview }) {
  const [info, setInfo] = useState<Templates | null>(null);
  const [template, setTemplate] = useState("matrix");
  const [format, setFormat] = useState("html");
  const [path, setPath] = useState("");
  const [written, setWritten] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    call<Templates>("reportTemplates").then(setInfo).catch((e) => setError(errorText(e)));
  }, []);
  useEffect(() => setPath(`${overview.name}-${template}.${format}`), [template, format, overview.name]);

  const generate = async (open: boolean) => {
    setBusy(true);
    setError(null);
    try {
      let target = path;
      if (desktop()) {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const picked = await save({ defaultPath: path, filters: [{ name: FORMAT_LABEL[format] ?? format, extensions: [format] }] });
        if (!picked) return;
        target = picked;
      }
      const r = await call<{ path: string }>("report", { template, format, path: target, open });
      setWritten(r.path);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (!info) return <div className="page">{error && <p className="error-text">{error}</p>}</div>;
  return (
    <div className="page narrow">
      <h1>Rapports</h1>
      <div className="form">
        <fieldset className="choice">
          <legend>Modèle</legend>
          {info.templates.map((t) => (
            <label key={t.id} className="inline">
              <input type="radio" name="template" value={t.id} checked={template === t.id} onChange={() => setTemplate(t.id)} />
              {t.label}
            </label>
          ))}
        </fieldset>
        <fieldset className="choice">
          <legend>Format</legend>
          {info.formats.map((f) => (
            <label key={f} className="inline">
              <input type="radio" name="format" value={f} checked={format === f} onChange={() => setFormat(f)} />
              {FORMAT_LABEL[f] ?? f}
            </label>
          ))}
        </fieldset>
        {!desktop() && (
          <label>
            Fichier (relatif au répertoire du projet ou absolu)
            <input value={path} onChange={(e) => setPath(e.target.value)} />
          </label>
        )}
        <div className="actions">
          <button className="button" onClick={() => generate(true)} disabled={busy || !path.trim()}>
            Générer et ouvrir
          </button>
          <button className="button secondary" onClick={() => generate(false)} disabled={busy || !path.trim()}>
            Générer
          </button>
        </div>
        {error && <p className="error-text">{error}</p>}
        {written && <p className="notice">Rapport écrit : {written}</p>}
        {overview.analysisFilter && (
          <p className="muted">Le filtre d'analyse « {overview.analysisFilter} » s'applique au rapport.</p>
        )}
      </div>
    </div>
  );
}
