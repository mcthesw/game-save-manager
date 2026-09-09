import { createServer, request } from "node:http";
import { createWriteStream } from "node:fs";
import {
  copyFile,
  mkdir,
  readFile,
  realpath,
  rmdir,
  stat,
} from "node:fs/promises";
import { registerHooks } from "node:module";
import { dirname, extname, join, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { setTimeout as delay } from "node:timers/promises";

// Existing E2E helpers use extensionless TS imports. Node 24 strips their types.
registerHooks({
  resolve(specifier, context, next) {
    try {
      return next(specifier, context);
    } catch (error) {
      if (
        error.code !== "ERR_MODULE_NOT_FOUND" ||
        !specifier.startsWith(".") ||
        extname(specifier)
      )
        throw error;
      return next(`${specifier}.ts`, context);
    }
  },
});
const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");
const { spawnTestProcess, hostCommand } = await import(
  pathToFileURL(join(repo, "apps/rgsm-gui/e2e/support/process.ts"))
);
const dist = join(repo, "apps/rgsm-gui/dist");

export async function validateDataDir(dataDir, workspace = repo) {
  const root = await realpath(join(workspace, ".rgsm-dev/acceptance"));
  const actual = await realpath(resolve(dataDir));
  if (!actual.startsWith(root + sep))
    throw new Error("Data must be inside .rgsm-dev/acceptance/<task>/");
  if (!(await stat(join(actual, "GameSaveManager.config.json"))).isFile())
    throw new Error("Seed the test configuration first");
  return actual;
}
export function validateOptions({ deviceId, port }) {
  if (!/^[a-zA-Z0-9_-]{1,80}$/.test(deviceId ?? ""))
    throw new Error("Use a short test device ID");
  if (!Number.isInteger(port) || port < 1024 || port > 65535)
    throw new Error("Choose a port between 1024 and 65535");
}

export async function startAcceptanceDevice(options) {
  validateOptions(options);
  const data = await validateDataDir(options.dataDir);
  const lock = join(data, ".acceptance-running");
  await mkdir(lock); // Exclusive ownership; never clear another launcher's lock.
  const origin = `http://127.0.0.1:${options.port}`;
  let runtime, host, log, stopped;
  const server = createServer(async (req, res) => {
    const fail = (status, text) => {
      res.writeHead(status, { "Content-Type": "text/plain" });
      res.end(text);
    };
    try {
      if (
        req.headers.host !== `127.0.0.1:${options.port}` ||
        (req.headers.origin && req.headers.origin !== origin)
      )
        return fail(403, "Forbidden");
      if (!runtime) return fail(503, "Starting test host");
      const url = new URL(req.url, origin);
      if (url.pathname === "/__acceptance" && req.method === "GET") {
        res.writeHead(200, {
          "Content-Type": "text/html; charset=utf-8",
          "Cache-Control": "no-store",
        });
        res.end(
          `<title>Acceptance device</title><a href="/">Open GUI</a> <button id="stop">Stop this test device</button><script>document.getElementById('stop').onclick=async()=>{await fetch('/__acceptance/stop',{method:'POST',headers:{Authorization:${JSON.stringify(`Bearer ${runtime.api_token}`)}}});document.body.textContent='Test device stopped; data preserved'}</script>`,
        );
        return;
      }
      if (url.pathname === "/__acceptance/stop" && req.method === "POST") {
        if (req.headers.authorization !== `Bearer ${runtime.api_token}`)
          return fail(401, "Unauthorized");
        res.writeHead(200);
        res.end("Stopping");
        setTimeout(() => {
          void stop().catch((error) => {
            console.error(error.message);
            process.exitCode = 1;
          });
        }, 100);
        return;
      }
      if (url.pathname.startsWith("/api/v1/")) {
        if (req.headers.authorization !== `Bearer ${runtime.api_token}`)
          return fail(401, "Reload the test page");
        const upstream = request(
          `http://127.0.0.1:${runtime.port}${url.pathname}${url.search}`,
          {
            method: req.method,
            headers: {
              Authorization: `Bearer ${runtime.api_token}`,
              Origin: "http://localhost:5173",
              "Content-Type": req.headers["content-type"] ?? "application/json",
              ...(req.headers["last-event-id"]
                ? { "Last-Event-ID": req.headers["last-event-id"] }
                : {}),
            },
          },
          (response) => {
            res.writeHead(response.statusCode, response.headers);
            response.pipe(res);
          },
        );
        upstream.on("error", () => {
          if (!res.headersSent) fail(502, "Test host unavailable");
          else res.destroy();
        });
        res.on("close", () => upstream.destroy());
        req.pipe(upstream);
        return;
      }
      let path = resolve(dist, "." + decodeURIComponent(url.pathname));
      if (path !== dist && !path.startsWith(dist + sep))
        return fail(403, "Forbidden");
      if (!(await stat(path).catch(() => null))?.isFile())
        path = join(dist, "index.html");
      let bytes = await readFile(path);
      if (path === join(dist, "index.html")) {
        const injection = JSON.stringify({
          apiBaseUrl: origin,
          token: runtime.api_token,
        }).replaceAll("<", "\\u003c");
        bytes = Buffer.from(
          bytes
            .toString()
            .replace(
              "</head>",
              `<script>window.__RGSM_RUNTIME__=${injection}</script></head>`,
            ),
        );
      }
      const mime =
        {
          ".html": "text/html; charset=utf-8",
          ".js": "text/javascript",
          ".css": "text/css",
          ".svg": "image/svg+xml",
          ".png": "image/png",
          ".woff": "font/woff",
          ".woff2": "font/woff2",
        }[extname(path)] ?? "application/octet-stream";
      res.writeHead(200, { "Content-Type": mime, "Cache-Control": "no-store" });
      res.end(bytes);
    } catch {
      if (!res.headersSent) fail(500, "Could not serve the test page");
      else res.destroy();
    }
  });
  const stop = () =>
    (stopped ??= (async () => {
      runtime = undefined;
      server.closeAllConnections();
      await new Promise((resolve) => server.close(resolve));
      await host?.stop();
      log?.end();
      await rmdir(lock);
    })());
  try {
    // Reserve the user's requested port before starting any backend process.
    await new Promise((ok, fail) => {
      server.once("error", fail);
      server.listen(options.port, "127.0.0.1", ok);
    });
    await stat(join(dist, "index.html"));
    const executable = process.platform === "win32" ? "rgsm.exe" : "rgsm";
    const binary = join(data, `.acceptance-${executable}`);
    await copyFile(join(repo, "target/debug", executable), binary);
    log = createWriteStream(join(data, ".acceptance-host.log"), { flags: "a" });
    const launch = hostCommand(binary, process.platform);
    host = spawnTestProcess(launch.command, launch.args, {
      cwd: repo,
      env: {
        ...process.env,
        RGSM_HTTP_HOST_ONLY: "1",
        RGSM_E2E_APP_DATA_DIR: data,
        RGSM_E2E_DEVICE_ID: options.deviceId,
      },
    });
    host.child.stdout.pipe(log);
    host.child.stderr.pipe(log);
    const deadline = Date.now() + 60000;
    while (Date.now() < deadline) {
      options.signal?.throwIfAborted();
      if (host.failure()) throw host.failure();
      try {
        const candidate = JSON.parse(
          await readFile(join(data, "GameSaveManager.host.json"), "utf8"),
        );
        const response = await fetch(
          `http://127.0.0.1:${candidate.port}/api/v1/get-build-info`,
          {
            method: "POST",
            headers: { Authorization: `Bearer ${candidate.api_token}` },
            signal: AbortSignal.timeout(1000),
          },
        );
        if (response.ok) {
          runtime = candidate;
          return { url: origin, stop };
        }
      } catch {
        /* Host is still starting. */
      }
      await delay(100);
    }
    throw new Error(
      "Test host did not become ready; inspect .acceptance-host.log",
    );
  } catch (error) {
    await stop();
    throw error;
  }
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  let session;
  const controller = new AbortController();
  const stop = async () => {
    controller.abort();
    await session?.stop();
  };
  process.once("SIGINT", stop);
  process.once("SIGTERM", stop);
  try {
    const [dataDir, deviceId, port] = process.argv.slice(2);
    session = await startAcceptanceDevice({
      dataDir,
      deviceId,
      port: Number(port),
      signal: controller.signal,
    });
    console.log(
      `Ready: ${session.url}\nStop: ${session.url}/__acceptance\nData is preserved when this device stops`,
    );
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
