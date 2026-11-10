// Live preview of the Google OAuth callback page (no real sign-in needed).
//   pnpm oauth:preview            -> http://127.0.0.1:8766
// The page's toolbar switches state/theme/accent/icon/language; edits to the HTML reload the tab.
import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, join, normalize, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("../src-tauri", import.meta.url)));
const page = join(root, "assets", "oauth-callback.html");
const port = Number(process.env.PORT) || 8766;
const types = { ".html": "text/html; charset=utf-8", ".png": "image/png", ".ico": "image/x-icon" };

createServer(async (req, res) => {
  const { pathname } = new URL(req.url, "http://localhost");
  try {
    if (pathname === "/__mtime") {
      res.writeHead(200, { "Cache-Control": "no-store" }).end(String((await stat(page)).mtimeMs));
      return;
    }
    if (pathname === "/") {
      res.writeHead(302, { Location: "/assets/oauth-callback.html" }).end();
      return;
    }
    const file = normalize(join(root, pathname));
    if (!file.startsWith(root + sep) || !pathname.startsWith("/icons/") && !pathname.startsWith("/assets/")) {
      res.writeHead(404).end("Not found");
      return;
    }
    res.writeHead(200, { "Content-Type": types[extname(file)] ?? "application/octet-stream", "Cache-Control": "no-store" });
    res.end(await readFile(file));
  } catch {
    res.writeHead(404).end("Not found");
  }
}).listen(port, "127.0.0.1", () => {
  console.log(`OAuth callback preview: http://127.0.0.1:${port}`);
});
