import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The setup window (setup/src). It shares components and CSS with the app in ../src.
export default defineConfig({
  root: __dirname,
  plugins: [react()],
  clearScreen: false,
  build: { outDir: "dist", emptyOutDir: true },
  server: {
    port: 1430,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
