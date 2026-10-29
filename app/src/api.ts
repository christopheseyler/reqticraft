import { invoke, isTauri } from "@tauri-apps/api/core";

// Les mêmes requêtes passent par Tauri (application de bureau) ou par le serveur
// local `reqticraft serve` (interface ouverte dans un navigateur).
export async function call<T>(method: string, args: Record<string, unknown> = {}): Promise<T> {
  if (isTauri()) {
    return invoke<T>("api", { method, args });
  }
  const res = await fetch(`/api/${method}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args),
  });
  const body = await res.json();
  if (!res.ok) throw new Error(body?.error ?? res.statusText);
  return body as T;
}

export const desktop = isTauri;

/** Sélecteur de fichier natif (application de bureau seulement). */
export async function pickProject(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "Projet ReqtiCraft ou Reqtify", extensions: ["rqtf", "rcproj"] }],
  });
  return typeof picked === "string" ? picked : null;
}

/** Fichier quelconque à tester (application de bureau seulement). */
export async function pickFile(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: false, directory: false });
  return typeof picked === "string" ? picked : null;
}

/** Emplacement d'enregistrement d'un projet (application de bureau seulement). */
export async function pickSavePath(defaultPath?: string): Promise<string | null> {
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save({
    defaultPath,
    filters: [
      { name: "Projet compatible Reqtify", extensions: ["rqtf"] },
      { name: "Projet ReqtiCraft natif", extensions: ["rcproj"] },
    ],
  });
}

export function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
