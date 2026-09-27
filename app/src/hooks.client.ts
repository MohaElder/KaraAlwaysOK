/** In development, sends the webview's errors to the terminal running `tauri dev`, so checks can read them. */
if (import.meta.env.DEV) {
  const send = (kind: string, text: string) => void fetch("/__webview-log", { method: "POST", body: `${kind}: ${text}` }).catch(() => {});
  addEventListener("error", (e) => send("error", String(e.error?.stack ?? e.message)));
  addEventListener("unhandledrejection", (e) => send("unhandled", e.reason instanceof Error ? String(e.reason.stack) : JSON.stringify(e.reason)));
  const error = console.error;
  console.error = (...args: unknown[]) => {
    error(...args);
    send("console.error", args.map(String).join(" "));
  };
}
