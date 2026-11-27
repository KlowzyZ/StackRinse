import assert from "node:assert/strict";
import { setTimeout as pause } from "node:timers/promises";

const port = process.argv[2];
const deadline = Date.now() + 45_000;
const expression = `(async () => {
  if (document.querySelector("h1")?.textContent.trim() !== "Storage overview") return false;
  return Array.isArray((await window.__TAURI_INTERNALS__.invoke("get_settings")).roots);
})()`;

async function checkPage(url) {
  const websocket = new WebSocket(url);
  try {
    return await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("DevTools response timed out")), 4_000);
      websocket.addEventListener("open", () => {
        websocket.send(JSON.stringify({
          id: 1,
          method: "Runtime.evaluate",
          params: { expression, awaitPromise: true, returnByValue: true },
        }));
      }, { once: true });
      websocket.addEventListener("error", () => {
        clearTimeout(timer);
        reject(new Error("Could not connect to WebView2 DevTools"));
      }, { once: true });
      websocket.addEventListener("message", ({ data }) => {
        const response = JSON.parse(data);
        if (response.id !== 1) return;
        clearTimeout(timer);
        resolve(response.result?.result?.value === true);
      });
    });
  } finally {
    websocket.close();
  }
}

let ready = false;
let lastError;
while (!ready && Date.now() < deadline) {
  try {
    const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`, {
      signal: AbortSignal.timeout(2_000),
    })).json();
    const app = pages.find(page =>
      page.type === "page" && /^https?:\/\/tauri\.localhost\//.test(page.url));
    if (app) ready = await checkPage(app.webSocketDebuggerUrl);
  } catch (error) {
    lastError = error;
  }
  if (!ready) await pause(500);
}
assert(ready, `Embedded Overview and native settings IPC were unavailable. ${lastError ?? ""}`);
console.log("Packaged WebView2 rendered the embedded Overview and answered native settings IPC.");
