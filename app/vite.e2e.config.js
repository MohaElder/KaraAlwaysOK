import base from "./vite.config.js";

/** The dev server for the browser tests: its own port and dependency cache so it runs beside `tauri:dev`, serving tests/. */
export default {
  ...base,
  cacheDir: "node_modules/.vite-e2e",
  server: { ...base.server, port: 1430, fs: { allow: ["tests"] } },
  optimizeDeps: { ...base.optimizeDeps, include: ["@tauri-apps/api/mocks", "@tauri-apps/api/event"] },
};
