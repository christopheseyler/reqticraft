import { useEffect, useState } from "react";
import { call, errorText } from "./api";
import type { Filter, FilterCondition, FiltersInfo } from "./model";
import type { Overview } from "./types";

// Filtres : définition (conditions en ET ou en OU) et application à
// l'affichage (masquage) ou à l'analyse (taux recalculés).

const EMPTY_CONDITION: FilterCondition = {
  application_class: "AbstractRequirement",
  receiver_class: "Attribute",
  receiver_document_type: null,
  receiver_element_type: "",
  operator: "equal",
  value: "",
};

export function FilterEditor({ onChanged }: { onChanged: (o: Overview) => void }) {
  const [info, setInfo] = useState<FiltersInfo | null>(null);
  const [filters, setFilters] = useState<Filter[]>([]);
  const [selected, setSelected] = useState(0);
  const [changed, setChanged] = useState(false);
  const [params, setParams] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);

  const load = () =>
    call<FiltersInfo>("filters")
      .then((i) => {
        setInfo(i);
        setFilters(i.filters.filters);
        setChanged(false);
      })
      .catch((e) => setError(errorText(e)));
  useEffect(() => {
    load();
  }, []);

  if (!info) return <div className="page">{error ? <p className="error-text">{error}</p> : null}</div>;

  const current = filters[selected];
  const edit = (f: (x: Filter) => Filter) => {
    setFilters(filters.map((x, i) => (i === selected ? f(structuredClone(x)) : x)));
    setChanged(true);
  };
  const save = async () => {
    setError(null);
    try {
      onChanged(await call<Overview>("saveFilters", { filters: { ...info.filters, filters } }));
      await load();
    } catch (e) {
      setError(errorText(e));
    }
  };
  const apply = async (mode: "display" | "analysis", name: string | null) => {
    setError(null);
    try {
      onChanged(await call<Overview>("applyFilter", { mode, name, params }));
      await load();
    } catch (e) {
      setError(errorText(e));
    }
  };
  const parameters = current ? (info.parameters[current.name] ?? []) : [];
  const unary = (op: string) => info.operators.find(([id]) => id === op)?.[2] === false;

  return (
    <div className="split types-split">
      <div className="pane type-list">
        <h1>Filtres</h1>
        <p className="muted">
          Affichage : {info.display ?? "aucun"}
          <br />
          Analyse : {info.analysis ?? "aucun"}
        </p>
        <ul className="tree" role="listbox" aria-label="Filtres">
          {filters.map((f, i) => (
            <li
              key={i}
              role="option"
              aria-selected={i === selected}
              tabIndex={0}
              className={`row${i === selected ? " selected" : ""}`}
              onClick={() => setSelected(i)}
              onKeyDown={(e) => e.key === "Enter" && setSelected(i)}
            >
              <span className="row-name">{f.name}</span>
              {info.display === f.name && <span className="badge change">affichage</span>}
              {info.analysis === f.name && <span className="badge status-derived">analyse</span>}
            </li>
          ))}
        </ul>
        <div className="actions">
          <button
            className="button secondary"
            onClick={() => {
              let name = "Nouveau filtre";
              for (let k = 2; filters.some((f) => f.name === name); k++) name = `Nouveau filtre ${k}`;
              setFilters([...filters, { name, all: true, conditions: [{ ...EMPTY_CONDITION }], raw_flags: [] }]);
              setSelected(filters.length);
              setChanged(true);
            }}
          >
            Nouveau filtre
          </button>
          {(info.display || info.analysis) && (
            <button
              className="button secondary"
              onClick={async () => {
                await apply("display", null);
                await apply("analysis", null);
              }}
            >
              Retirer les filtres
            </button>
          )}
        </div>
      </div>
      <div className="pane type-detail">
        {error && <p className="error-text pre">{error}</p>}
        {!current ? (
          <p className="muted">Aucun filtre. Créez-en un.</p>
        ) : (
          <div className="form">
            <div className="toolbar">
              <h2 className="grow">{current.name}</h2>
              <button className="button" onClick={save} disabled={!changed}>
                Enregistrer les filtres
              </button>
              <button
                className="button danger"
                onClick={() => {
                  setFilters(filters.filter((_, i) => i !== selected));
                  setSelected(0);
                  setChanged(true);
                }}
              >
                Supprimer
              </button>
            </div>
            <div className="grid-2">
              <label>
                Nom
                <input value={current.name} onChange={(e) => edit((f) => ({ ...f, name: e.target.value }))} />
              </label>
              <label>
                Combinaison
                <select value={current.all ? "all" : "any"} onChange={(e) => edit((f) => ({ ...f, all: e.target.value === "all" }))}>
                  <option value="all">Toutes les conditions (ET)</option>
                  <option value="any">Au moins une condition (OU)</option>
                </select>
              </label>
            </div>
            <h3>Conditions</h3>
            {current.conditions.map((c, ci) => {
              const setC = (patch: Partial<FilterCondition>) =>
                edit((f) => {
                  f.conditions[ci] = { ...f.conditions[ci], ...patch };
                  return f;
                });
              return (
                <div key={ci} className="element-card condition">
                  <select aria-label="Éléments concernés" value={c.application_class ?? "AbstractRequirement"} onChange={(e) => setC({ application_class: e.target.value })}>
                    {info.applicationClasses.map(([id, label]) => (
                      <option key={id} value={id}>
                        {label}
                      </option>
                    ))}
                  </select>
                  <select aria-label="Propriété examinée" value={c.receiver_class} onChange={(e) => setC({ receiver_class: e.target.value })}>
                    {info.receivers.map(([id, label]) => (
                      <option key={id} value={id}>
                        {label}
                      </option>
                    ))}
                  </select>
                  {c.receiver_class === "Attribute" && (
                    <input
                      aria-label="Nom de l'attribut"
                      placeholder="Attribut"
                      value={c.receiver_element_type ?? ""}
                      onChange={(e) => setC({ receiver_element_type: e.target.value })}
                    />
                  )}
                  <select aria-label="Opérateur" value={c.operator} onChange={(e) => setC({ operator: e.target.value })}>
                    {info.operators.map(([id, label]) => (
                      <option key={id} value={id}>
                        {label}
                      </option>
                    ))}
                  </select>
                  {!unary(c.operator) && (
                    <input
                      aria-label="Valeur"
                      placeholder="Valeur ou $paramètre"
                      value={c.value ?? ""}
                      onChange={(e) => setC({ value: e.target.value })}
                    />
                  )}
                  <button
                    className="link"
                    aria-label="Retirer la condition"
                    onClick={() =>
                      edit((f) => {
                        f.conditions.splice(ci, 1);
                        return f;
                      })
                    }
                  >
                    Retirer
                  </button>
                </div>
              );
            })}
            <button
              className="button secondary"
              onClick={() =>
                edit((f) => {
                  f.conditions.push({ ...EMPTY_CONDITION });
                  return f;
                })
              }
            >
              Ajouter une condition
            </button>

            <h3>Appliquer</h3>
            {changed && <p className="muted">Enregistrez les filtres avant de les appliquer.</p>}
            {parameters.map((p) => (
              <label key={p}>
                Paramètre ${p}
                <input value={params[p] ?? ""} onChange={(e) => setParams({ ...params, [p]: e.target.value })} />
              </label>
            ))}
            <div className="actions">
              <button className="button secondary" disabled={changed} onClick={() => apply("display", current.name)}>
                À l'affichage
              </button>
              <button className="button secondary" disabled={changed} onClick={() => apply("analysis", current.name)}>
                À l'analyse
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
