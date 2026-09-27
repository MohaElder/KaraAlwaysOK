/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

const host = process.env.TAURI_DEV_HOST;

/** Prints what the webview posts to /__webview-log (development only; see hooks.client.ts and +layout.svelte). */
const webviewLog = {
  name: "webview-log",
  /** @param {import("vite").ViteDevServer} server */
  configureServer(server) {
    server.middlewares.use("/__webview-log", (req, res) => {
      let body = "";
      req.on("data", (chunk) => (body += chunk));
      req.on("end", () => {
        console.error(body === "ready" ? "[webview-ready]" : `[webview] ${body}`);
        res.end();
      });
    });
  },
};

export default defineConfig({
  plugins: [sveltekit(), webviewLog],
  clearScreen: false,
  optimizeDeps: { exclude: ["phosphor-svelte"] },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  test: { include: ["src/**/*.test.ts"] },
});
