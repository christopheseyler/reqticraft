import type { Change, ElementRef, Severity, Status } from "./types";

const STATUS_LABEL: Record<Status, string> = {
  covered: "Couverte",
  uncovered: "Non couverte",
  derived: "Dérivée",
  none: "",
};

export function StatusBadge({ status }: { status: Status }) {
  if (status === "none") return null;
  return <span className={`badge status-${status}`}>{STATUS_LABEL[status]}</span>;
}

export function ChangeBadge({ change }: { change: Change | null }) {
  if (!change) return null;
  return (
    <span className="badge change">{change === "new" ? "Nouvelle" : "Modifiée"}</span>
  );
}

const SEVERITY_LABEL: Record<Severity, string> = {
  error: "Erreur",
  warning: "Avertissement",
  information: "Information",
};

export function SeverityBadge({ severity }: { severity: Severity }) {
  return <span className={`badge severity-${severity}`}>{SEVERITY_LABEL[severity]}</span>;
}

export function percent(ratio: number | null | undefined): string {
  if (ratio === null || ratio === undefined) return "—";
  return `${Math.round(ratio * 100)} %`;
}

export function Meter({ ratio }: { ratio: number | null }) {
  if (ratio === null) return <span className="muted">non couvert</span>;
  return (
    <span className="meter" role="img" aria-label={`couverture ${percent(ratio)}`}>
      <span className="meter-track">
        <span className="meter-fill" style={{ width: `${ratio * 100}%` }} />
      </span>
      <span className="meter-value">{percent(ratio)}</span>
    </span>
  );
}

export function ElementLink({
  target,
  onSelect,
  showDocument = true,
}: {
  target: ElementRef;
  onSelect: (document: number, element: number) => void;
  showDocument?: boolean;
}) {
  const label = (
    <>
      {showDocument && <span className="muted">{target.documentName} › </span>}
      {target.name}
    </>
  );
  if (target.element === null) return <span>{label}</span>;
  const element = target.element;
  return (
    <button className="link" onClick={() => onSelect(target.document, element)}>
      {label}
    </button>
  );
}
