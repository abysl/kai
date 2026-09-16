import { createServer } from "node:http";
import { createReadStream, statSync } from "node:fs";
import { extname, join, normalize, resolve } from "node:path";

const root = resolve(process.argv[2] ?? join(import.meta.dirname, "..", "..", "web", "dist"));
const port = Number(process.argv[3] ?? process.env.KAI_WEB_PORT ?? 8123);

const types = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".wasm": "application/wasm",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".webp": "image/webp",
  ".css": "text/css",
  ".ttf": "font/ttf",
};

const answer = (request, response) => {
  const url = new URL(request.url ?? "/", "http://localhost");
  const relative = normalize(decodeURIComponent(url.pathname)).replace(/^(\.\.[/\\])+/, "");
  let path = join(root, relative);
  if (!path.startsWith(root)) {
    response.writeHead(403).end();
    return;
  }
  let info;
  try {
    info = statSync(path);
    if (info.isDirectory()) {
      path = join(path, "index.html");
      info = statSync(path);
    }
  } catch {
    response.writeHead(404, { "content-type": "text/plain" }).end("not found: " + relative);
    return;
  }
  response.writeHead(200, {
    "content-type": types[extname(path)] ?? "application/octet-stream",
    "content-length": info.size,
    "cache-control": "no-store",
  });
  createReadStream(path).pipe(response);
};

const server = createServer(answer);
server.listen(port, "127.0.0.1", () => {
  console.log(`serving ${root} on http://127.0.0.1:${port}/`);
});
