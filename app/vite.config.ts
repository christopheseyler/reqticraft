import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Configuration recommandée par Tauri : port fixe, pas d'écran effacé.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
    // Hors de Tauri, les requêtes vont au serveur local `reqticraft serve`.
    proxy: { "/api": "http://127.0.0.1:7878" },
  },
});
