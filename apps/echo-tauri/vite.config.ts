import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  // Tauri dev server expects a fixed port
  server: {
    port: 5173,
    strictPort: true,
    host: "0.0.0.0",
  },
  // 让 Tauri 内嵌的 webview 可以走相对路径
  base: "./",
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "es2021",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
