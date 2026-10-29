import { useEffect, useState } from "react";
import { call, desktop, errorText, pickFile } from "./api";
import type { CaptureRule, DocumentType, ElementType, TestResult, TypeInfo } from "./model";
import { ELEMENT_CLASSES, ROLES } from "./model";
import type { Overview } from "./types";

// Éditeur de types : liste, propriétés et éléments d'un type, testeur sur un
// fichier. Les types de la bibliothèque sont en lecture seule : on les duplique.

const ORIGIN: Record<TypeInfo["origin"], string> = {
  project: "projet",
  modified: "modifié",
  configuration: "configuration",
  library: "bibliothèque",
};

export function TypeEditor({ onSaved }: { onSaved: (o: Overview) => void }) {
  const [types, setTypes] = useState<TypeInfo[]>([]);
  const [search, setSearch] = useState("");
  const [current, setCurrent] = useState<DocumentType | null>(null);
  const [original, setOriginal] = useState<string | null>(null);
  const [origin, setOrigin] = useState<TypeInfo["origin"] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const refresh = () => call<TypeInfo[]>("types").then(setTypes).catch((e) => setError(errorText(e)));
  useEffect(() => {
    refresh();
  }, []);

  const open = async (name: string) => {
    setError(null);
    setMessage(null);
    try {
      setCurrent(await call<DocumentType>("type", { name }));
      setOriginal(name);
      setOrigin(types.find((t) => t.name === name)?.origin ?? null);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const duplicate = () => {
    if (!current) return;
    let name = `${current.name} (copie)`;
    for (let k = 2; types.some((t) => t.name === name); k++) name = `${current.name} (copie ${k})`;
    setCurrent({ ...structuredClone(current), name, source: null });
    setOriginal(null);
    setOrigin("modified");
    setMessage("Copie non enregistrée : modifiez-la, testez-la puis enregistrez-la dans le projet.");
  };

  const save = async () => {
    if (!current) return;
    setError(null);
    try {
      onSaved(await call<Overview>("saveType", { type: current, previous: original }));
      setOriginal(current.name);
      setOrigin("modified");
      setMessage("Type enregistré dans le projet ; l'analyse a été relancée.");
      refresh();
    } catch (e) {
      setError(errorText(e));
    }
  };

  const remove = async () => {
    if (!original) return;
    try {
      onSaved(await call<Overview>("deleteType", { name: original }));
      setCurrent(null);
      setOriginal(null);
      refresh();
    } catch (e) {
      setError(errorText(e));
    }
  };

  const readOnly = origin === "library" || origin === "configuration";
  const shown = types.filter((t) => t.name.toLowerCase().includes(search.toLowerCase()));

  return (
    <div className="split types-split">
      <div className="pane type-list">
        <h1>Types</h1>
        <input
          type="search"
          className="search"
          placeholder="Rechercher un type"
          aria-label="Rechercher un type"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <ul className="tree" role="listbox" aria-label="Types d'analyse">
          {shown.map((t) => (
            <li
              key={t.name}
              role="option"
              aria-selected={current?.name === t.name}
              tabIndex={0}
              className={`row${current?.name === t.name ? " selected" : ""}`}
              onClick={() => open(t.name)}
              onKeyDown={(e) => e.key === "Enter" && open(t.name)}
            >
              <span className="row-name">{t.name}</span>
              <span className={`badge origin-${t.origin}`}>{ORIGIN[t.origin]}</span>
            </li>
          ))}
        </ul>
      </div>
      <div className="pane type-detail">
        {error && <p className="error-text pre">{error}</p>}
        {message && <p className="notice">{message}</p>}
        {!current ? (
          <p className="muted">Sélectionnez un type.</p>
        ) : (
          <>
            <div className="toolbar">
              <h2 className="grow">{current.name}</h2>
              <button className="button secondary" onClick={duplicate}>
                Dupliquer
              </button>
              {!readOnly && (
                <button className="button" onClick={save}>
                  Enregistrer dans le projet
                </button>
              )}
              {origin === "modified" && original && (
                <button className="button danger" onClick={remove}>
                  Supprimer
                </button>
              )}
            </div>
            {readOnly && (
              <p className="muted">Type de la {ORIGIN[origin!]}, en lecture seule : dupliquez-le pour le modifier.</p>
            )}
            <TypeForm value={current} readOnly={readOnly} onChange={setCurrent} />
            <Tester type={current} />
          </>
        )}
      </div>
    </div>
  );
}

function list(v: string): string[] {
  return v
    .split(",")
    .map((x) => x.trim())
    .filter(Boolean);
}

function TypeForm({
  value,
  readOnly,
  onChange,
}: {
  value: DocumentType;
  readOnly: boolean;
  onChange: (t: DocumentType) => void;
}) {
  const set = (patch: Partial<DocumentType>) => onChange({ ...value, ...patch });
  const setElement = (i: number, e: ElementType) => {
    const element_types = [...value.element_types];
    element_types[i] = e;
    set({ element_types });
  };
  const addElement = () =>
    set({
      element_types: [
        ...value.element_types,
        {
          name: `Élément ${value.element_types.length + 1}`,
          class: "requirement",
          rule: { kind: "regex", expression: "", sub_expression: null, end_expression: null, row_expression: null },
          parents: [],
          display: null,
          roles: [],
          flags: {},
          enum_values: [],
          properties: [],
        },
      ],
    });
  return (
    <fieldset className="form type-form" disabled={readOnly}>
      <div className="grid-2">
        <label>
          Nom
          <input value={value.name} onChange={(e) => set({ name: e.target.value })} />
        </label>
        <label>
          Convertisseur
          <input value={value.converter} onChange={(e) => set({ converter: e.target.value })} />
        </label>
        <label>
          Extensions (séparées par des virgules)
          <input
            value={value.file_extensions.join(", ")}
            onChange={(e) => set({ file_extensions: list(e.target.value) })}
          />
        </label>
        <label>
          Famille
          <select value={value.kind} onChange={(e) => set({ kind: e.target.value as DocumentType["kind"] })}>
            <option value="text">Texte (expressions régulières)</option>
            <option value="xml">XML (arbres d'analyse)</option>
            <option value="added_elements">Éléments ajoutés</option>
          </select>
        </label>
      </div>
      <h3>Éléments</h3>
      <div className="elements">
        {value.element_types.map((e, i) => (
          <ElementForm
            key={i}
            value={e}
            onChange={(x) => setElement(i, x)}
            onRemove={() => set({ element_types: value.element_types.filter((_, k) => k !== i) })}
          />
        ))}
      </div>
      <button className="button secondary" type="button" onClick={addElement}>
        Ajouter un élément
      </button>
    </fieldset>
  );
}

function ElementForm({
  value,
  onChange,
  onRemove,
}: {
  value: ElementType;
  onChange: (e: ElementType) => void;
  onRemove: () => void;
}) {
  const set = (patch: Partial<ElementType>) => onChange({ ...value, ...patch });
  const rule = value.rule;
  const setRule = (patch: Record<string, string | null>) => set({ rule: { ...rule, ...patch } as CaptureRule });
  const roles = value.roles.length ? value.roles : (["ident", "label"] as ElementType["roles"]);
  return (
    <div className="element-card">
      <div className="grid-3">
        <label>
          Nom
          <input value={value.name} onChange={(e) => set({ name: e.target.value })} />
        </label>
        <label>
          Classe
          <select value={value.class} onChange={(e) => set({ class: e.target.value })}>
            {ELEMENT_CLASSES.map(([id, label]) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Parents (séparés par des virgules)
          <input value={value.parents.join(", ")} onChange={(e) => set({ parents: list(e.target.value) })} />
        </label>
      </div>
      {rule.kind === "regex" && (
        <>
          <label>
            Expression
            <input className="mono" value={rule.expression} onChange={(e) => setRule({ expression: e.target.value })} />
          </label>
          <div className="grid-2">
            <label>
              Sous-expression (découpe en plusieurs valeurs)
              <input
                className="mono"
                value={rule.sub_expression ?? ""}
                onChange={(e) => setRule({ sub_expression: e.target.value || null })}
              />
            </label>
            <label>
              Expression de fin
              <input
                className="mono"
                value={rule.end_expression ?? ""}
                onChange={(e) => setRule({ end_expression: e.target.value || null })}
              />
            </label>
          </div>
          <div className="roles">
            <span className="muted">Groupes :</span>
            {[0, 1, 2, 3].map((k) => (
              <label key={k} className="inline">
                {k + 1}
                <select
                  value={roles[k] ?? "none"}
                  onChange={(e) => {
                    const r = [...roles];
                    while (r.length <= k) r.push("none");
                    r[k] = e.target.value as ElementType["roles"][number];
                    while (r.length && r[r.length - 1] === "none") r.pop();
                    set({ roles: r });
                  }}
                >
                  {ROLES.map(([id, label]) => (
                    <option key={id} value={id}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          </div>
        </>
      )}
      {rule.kind === "xml_tree" && (
        <label>
          Arbre d'analyse
          <textarea className="mono" rows={3} value={rule.pattern} onChange={(e) => setRule({ pattern: e.target.value })} />
        </label>
      )}
      <div className="actions compact">
        <button className="link" type="button" onClick={onRemove}>
          Retirer l'élément
        </button>
      </div>
    </div>
  );
}

function Tester({ type }: { type: DocumentType }) {
  const [file, setFile] = useState("");
  const [result, setResult] = useState<TestResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [showIntermediate, setShowIntermediate] = useState(false);

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      setResult(await call<TestResult>("testType", { type, file }));
    } catch (e) {
      setResult(null);
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="tester">
      <h3>Testeur</h3>
      <form
        className="path-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (file.trim()) run();
        }}
      >
        <input
          aria-label="Fichier à tester"
          placeholder="Fichier à tester (chemin relatif au projet ou absolu)"
          value={file}
          onChange={(e) => setFile(e.target.value)}
        />
        {desktop() && (
          <button
            type="button"
            className="button secondary"
            onClick={async () => {
              const f = await pickFile();
              if (f) setFile(f);
            }}
          >
            Parcourir…
          </button>
        )}
        <button className="button" type="submit" disabled={busy || !file.trim()}>
          {busy ? "Test…" : "Tester"}
        </button>
      </form>
      {error && <p className="error-text pre">{error}</p>}
      {result && (
        <>
          <p>
            {result.counts.length === 0
              ? "Aucun élément capturé."
              : result.counts.map(([n, c]) => `${n} : ${c}`).join(" · ")}
          </p>
          {result.messages.map((m, i) => (
            <p key={i} className="error-text">
              {m}
            </p>
          ))}
          <ul className="tree test-tree">
            {result.rows
              .filter((r) => r.class !== "text" || r.text)
              .slice(0, 500)
              .map((r, i) => (
                <li key={i} className={`row class-${r.class}`} style={{ paddingLeft: 12 + r.depth * 16 }}>
                  <span className="badge">{r.typeName}</span>
                  <span className="row-name">{r.class === "text" ? r.text : r.name}</span>
                  {r.references.length > 0 && <span className="muted">→ {r.references.join(", ")}</span>}
                  {r.attributes.length > 0 && <span className="muted">{r.attributes.join(" ; ")}</span>}
                </li>
              ))}
          </ul>
          <button className="link" onClick={() => setShowIntermediate(!showIntermediate)}>
            {showIntermediate ? "Masquer" : "Afficher"} l'intermédiaire ({result.intermediateLines} lignes)
          </button>
          {showIntermediate && <pre className="intermediate">{result.intermediate}</pre>}
        </>
      )}
    </section>
  );
}
