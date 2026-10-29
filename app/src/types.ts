// Vues renvoyées par le cœur (crates/rc-core/src/session.rs).

export type Status = "covered" | "uncovered" | "derived" | "none";
export type Change = "new" | "modified";
export type Severity = "error" | "warning" | "information";
export type ElementClass =
  | "section"
  | "macro_requirement"
  | "requirement"
  | "entity"
  | "reference"
  | "attribute"
  | "reference_attribute"
  | "link"
  | "value"
  | "text"
  | "picture"
  | "table";

export interface DocumentInfo {
  index: number;
  name: string;
  typeName: string;
  path: string;
  requirements: number;
  covered: number | null;
  ratio: number | null;
  derived: number;
  errors: number;
  warnings: number;
  changed: number;
}

export interface ChannelInfo {
  covered: number;
  covering: number[];
  coveredCount: number;
  total: number;
  ratio: number;
}

export interface Overview {
  name: string;
  path: string;
  documents: DocumentInfo[];
  channels: ChannelInfo[];
  requirements: number;
  covered: number;
  coverable: number;
  ratio: number | null;
  derived: number;
  errors: number;
  warnings: number;
  informations: number;
}

export interface ElementRef {
  document: number;
  element: number | null;
  name: string;
  documentName: string;
  status: Status;
}

export interface Row {
  id: number;
  name: string;
  ident: string | null;
  label: string | null;
  typeName: string;
  class: ElementClass;
  depth: number;
  status: Status;
  change: Change | null;
  problems: number;
}

export interface Page {
  total: number;
  rows: Row[];
}

export interface Problem {
  rule: string;
  severity: Severity;
  text: string;
  document: number | null;
  documentName: string | null;
  element: number | null;
  elementName: string | null;
  line: number | null;
}

export interface Details {
  document: number;
  documentName: string;
  id: number;
  name: string;
  ident: string | null;
  label: string | null;
  typeName: string;
  class: ElementClass;
  text: string;
  line: number;
  status: Status;
  change: Change | null;
  added: boolean;
  path: ElementRef[];
  attributes: [string, string | null][];
  references: string[];
  covers: ElementRef[];
  coveredBy: ElementRef[];
  problems: Problem[];
}

export interface ImpactNode extends ElementRef {
  cycle: boolean;
  children: ImpactNode[];
}

export interface Impact {
  root: ElementRef;
  upstream: ImpactNode[];
  downstream: ImpactNode[];
}

export interface Selection {
  document: number;
  element: number;
}
