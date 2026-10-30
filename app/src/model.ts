// Modèle de projet, de types et de filtres tel que le cœur le sérialise
// (crates/rc-model), pour les éditeurs.

export interface DocumentNode {
  kind: "document";
  name: string;
  doc_type: string;
  path: string;
  absolute_path: string | null;
  category: string | null;
  params: [string, string][];
  modification_docs: { path: string; doc_type: string }[];
  ignored_types: string[];
  position: [number, number] | null;
}

export interface FolderNode {
  kind: "folder";
  name: string;
  default_type: string | null;
  children: number[];
  position: [number, number] | null;
  size: [number, number] | null;
  category: string | null;
}

export type ProjectNode =
  | DocumentNode
  | FolderNode
  | { kind: "sub_project"; name: string; path: string };

export interface Channel {
  kind: "cover" | "mirror" | "dependency";
  covered: number;
  covering: number[];
  category: string | null;
}

export interface Project {
  name: string;
  properties: [string, string][];
  nodes: ProjectNode[];
  channels: Channel[];
  path_replacements: unknown[];
}

export interface TypeInfo {
  name: string;
  kind: "text" | "xml" | "added_elements";
  converter: string;
  origin: "project" | "modified" | "configuration" | "library";
  elements: number;
}

export interface ProjectInfo {
  project: Project;
  path: string;
  format: "rqtf" | "rcproj";
  dirty: boolean;
  types: TypeInfo[];
}

export type CaptureRule =
  | {
      kind: "regex";
      expression: string;
      sub_expression: string | null;
      end_expression: string | null;
      row_expression: string | null;
    }
  | { kind: "xml_tree"; pattern: string; sub_expression: string | null }
  | { kind: "none" };

export type Role = "none" | "ident" | "label" | "text" | "guid" | "parent" | "depth";

export interface ElementType {
  name: string;
  class: string;
  rule: CaptureRule;
  parents: string[];
  display: string | null;
  roles: Role[];
  flags: Record<string, boolean>;
  enum_values: string[];
  properties: [string, string][];
}

export interface DocumentType {
  name: string;
  kind: "text" | "xml" | "added_elements";
  converter: string;
  edit_tool: string | null;
  file_extensions: string[];
  matching_file_names: string[];
  excluded_file_names: string[];
  variables: unknown[];
  properties: [string, string][];
  exclude_text: string[];
  delete_expression: string | null;
  element_types: ElementType[];
  source: string | null;
}

export interface TestRow {
  depth: number;
  class: string;
  typeName: string;
  name: string;
  text: string;
  references: string[];
  attributes: string[];
}

export interface TestResult {
  rows: TestRow[];
  counts: [string, number][];
  messages: string[];
  intermediate: string;
  intermediateLines: number;
}

export interface FilterCondition {
  application_class: string | null;
  receiver_class: string;
  receiver_document_type: string | null;
  receiver_element_type: string | null;
  operator: string;
  value: string | null;
}

export interface Filter {
  name: string;
  all: boolean;
  conditions: FilterCondition[];
  raw_flags: boolean[];
}

export interface FilterSet {
  filters: Filter[];
  current: string | null;
}

export interface FiltersInfo {
  filters: FilterSet;
  parameters: Record<string, string[]>;
  display: string | null;
  analysis: string | null;
  operators: [string, string, boolean][];
  receivers: [string, string][];
  applicationClasses: [string, string][];
}

export const ELEMENT_CLASSES: [string, string][] = [
  ["section", "Section"],
  ["macro_requirement", "Macro-exigence"],
  ["requirement", "Exigence"],
  ["entity", "Entité"],
  ["reference", "Référence (couvre)"],
  ["attribute", "Attribut"],
  ["reference_attribute", "Attribut de référence"],
  ["link", "Lien"],
  ["value", "Valeur"],
  ["text", "Texte"],
  ["picture", "Image"],
  ["table", "Tableau"],
];

export const ROLES: [Role, string][] = [
  ["none", "—"],
  ["ident", "Identifiant"],
  ["label", "Libellé"],
  ["text", "Texte"],
  ["guid", "GUID"],
  ["parent", "Parent"],
  ["depth", "Profondeur"],
];
