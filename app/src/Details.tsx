import { useEffect, useState } from "react";
import { call, errorText } from "./api";
import type { Details as DetailsData, Selection } from "./types";
import { date, type ElementState, type SnapshotInfo } from "./History";
import { ChangeBadge, ElementLink, SeverityBadge, StatusBadge } from "./ui";

interface HistoryEntry {
  snapshot: SnapshotInfo | null;
  state: ElementState | null;
}

const STATUS: Record<string, string> = { covered: "couverte", uncovered: "non couverte", derived: "dérivée", none: "" };

// Panneau Détails : texte, attributs, couverture et problèmes d'un élément.
export function Details({
  selection,
  version,
  onSelect,
  onImpact,
}: {
  selection: Selection | null;
  version: number;
  onSelect: (document: number, element: number) => void;
  onImpact?: () => void;
}) {
  const [data, setData] = useState<DetailsData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<HistoryEntry[]>([]);

  useEffect(() => {
    if (!selection) return;
    let live = true;
    call<HistoryEntry[]>("history", { ...selection })
      .then((h) => live && setHistory(h))
      .catch(() => live && setHistory([]));
    return () => {
      live = false;
    };
  }, [selection, version]);

  useEffect(() => {
    if (!selection) {
      setData(null);
      return;
    }
    let live = true;
    call<DetailsData>("element", { ...selection })
      .then((d) => live && (setData(d), setError(null)))
      .catch((e) => live && (setData(null), setError(errorText(e))));
    return () => {
      live = false;
    };
  }, [selection, version]);

  if (!selection) {
    return <aside className="details empty muted">Sélectionnez un élément.</aside>;
  }
  if (error) return <aside className="details error-text">{error}</aside>;
  if (!data) return <aside className="details muted">Chargement…</aside>;

  return (
    <aside className="details" aria-label="Détails de l'élément">
      {data.path.length > 0 && (
        <div className="crumbs muted">
          {data.documentName}
          {data.path.map((p) => ` › ${p.name}`)}
        </div>
      )}
      <h2>{data.name}</h2>
      <div className="badges">
        <span className="badge">{data.typeName}</span>
        <StatusBadge status={data.status} />
        <ChangeBadge change={data.change} />
        {data.added && <span className="badge">Ajouté</span>}
      </div>
      {data.text && <p className="element-text">{data.text}</p>}

      {data.attributes.length > 0 && (
        <section>
          <h3>Attributs</h3>
          <dl className="attributes">
            {data.attributes.map(([name, value], i) => (
              <div key={i}>
                <dt>{name}</dt>
                <dd>{value ?? "oui"}</dd>
              </div>
            ))}
          </dl>
        </section>
      )}

      <section>
        <h3>Couvre</h3>
        {data.covers.length === 0 ? (
          <p className="muted">Rien{data.status === "derived" ? " : exigence dérivée" : ""}.</p>
        ) : (
          <ul className="plain">
            {data.covers.map((t, i) => (
              <li key={i}>
                <ElementLink target={t} onSelect={onSelect} /> <StatusBadge status={t.status} />
              </li>
            ))}
          </ul>
        )}
      </section>

      <section>
        <h3>Couvert par</h3>
        {data.coveredBy.length === 0 ? (
          <p className="muted">Personne.</p>
        ) : (
          <ul className="plain">
            {data.coveredBy.map((t, i) => (
              <li key={i}>
                <ElementLink target={t} onSelect={onSelect} />
              </li>
            ))}
          </ul>
        )}
      </section>

      {data.references.length > 0 && (
        <section>
          <h3>Références écrites</h3>
          <p>{data.references.join(", ")}</p>
        </section>
      )}

      {data.problems.length > 0 && (
        <section>
          <h3>Problèmes</h3>
          <ul className="plain">
            {data.problems.map((p, i) => (
              <li key={i}>
                <SeverityBadge severity={p.severity} /> {p.text}
              </li>
            ))}
          </ul>
        </section>
      )}

      {history.length > 1 && (
        <section>
          <h3>Historique</h3>
          <ul className="plain history">
            {history.map((h, i) => (
              <li key={i}>
                <span className="history-when">
                  {h.snapshot ? `${h.snapshot.name} (${date(h.snapshot.created)})` : "Maintenant"}
                </span>
                {h.state ? (
                  <span>
                    {STATUS[h.state.status] && <span className={`badge status-${h.state.status}`}>{STATUS[h.state.status]}</span>}{" "}
                    {i > 0 && history[i - 1].state && history[i - 1].state!.text !== h.state.text ? (
                      <span className="changed-text">{h.state.text}</span>
                    ) : (
                      <span className="muted">{i === 0 ? h.state.text : "texte inchangé"}</span>
                    )}
                  </span>
                ) : (
                  <span className="muted">absent</span>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}

      <div className="actions">
        {onImpact && (
          <button className="button" onClick={onImpact}>
            Analyse d'impact
          </button>
        )}
        <button
          className="button secondary"
          onClick={() => call("openSource", { document: data.document }).catch(console.error)}
        >
          Ouvrir le document source
        </button>
        {data.line > 0 && <span className="muted">ligne {data.line} de l'intermédiaire</span>}
      </div>
    </aside>
  );
}
