import { useEffect, useState } from "react";
import { call, errorText } from "./api";
import type { Details as DetailsData, ElementRef, Selection } from "./types";
import { date, type ElementState, type SnapshotInfo } from "./History";
import { ChangeBadge, ElementLink, SeverityBadge, StatusBadge } from "./ui";

interface HistoryEntry {
  snapshot: SnapshotInfo | null;
  state: ElementState | null;
}

const STATUS: Record<string, string> = { covered: "couverte", uncovered: "non couverte", derived: "dérivée", none: "" };

// Éléments liés, groupés par document (exigences, conception, tests…) avec leur texte.
function Links({
  links,
  onFollow,
  empty,
}: {
  links: ElementRef[];
  onFollow: (document: number, element: number) => void;
  empty: string;
}) {
  if (links.length === 0) return <p className="muted">{empty}</p>;
  const groups = new Map<string, ElementRef[]>();
  for (const l of links) groups.set(l.documentName, [...(groups.get(l.documentName) ?? []), l]);
  return (
    <>
      {[...groups].map(([doc, items]) => (
        <div key={doc} className="link-group">
          <h4>
            {doc} <span className="muted">({items.length})</span>
          </h4>
          <ul className="plain links">
            {items.map((t, i) => (
              <li key={i} className={`link-card st-${t.status}`}>
                <div className="link-head">
                  <ElementLink target={t} onSelect={onFollow} showDocument={false} />
                  {t.typeName && <span className="badge">{t.typeName}</span>}
                  <StatusBadge status={t.status} />
                </div>
                {t.text && <p className="link-text">{t.text}</p>}
              </li>
            ))}
          </ul>
        </div>
      ))}
    </>
  );
}

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
  const [trail, setTrail] = useState<Selection[]>([]);

  // Suivre un lien garde l'élément courant pour pouvoir y revenir.
  const follow = (document: number, element: number) => {
    if (selection) setTrail((t) => [...t, selection]);
    onSelect(document, element);
  };
  const back = () => {
    const prev = trail[trail.length - 1];
    if (!prev) return;
    setTrail(trail.slice(0, -1));
    onSelect(prev.document, prev.element);
  };

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
    <aside className="details zones" aria-label="Détails de l'élément">
      {/* En haut : ce que l'élément couvre ; au milieu : l'élément ; en bas : ce qui le référence. */}
      <section className="zone zone-links" aria-label="Éléments couverts">
        <h3>Couvre ({data.covers.length})</h3>
        <Links
          links={data.covers}
          onFollow={follow}
          empty={`Rien${data.status === "derived" ? " : exigence dérivée" : ""}.`}
        />
      </section>
      <div className="zone zone-main">
        {trail.length > 0 && (
          <button className="link back" onClick={back}>
            ← Précédent
          </button>
        )}
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
      </div>
      <section className="zone zone-links" aria-label="Éléments qui le référencent">
        <h3>Couvert par ({data.coveredBy.length})</h3>
        <Links links={data.coveredBy} onFollow={follow} empty="Aucun élément ne la référence." />
      </section>
    </aside>
  );
}
