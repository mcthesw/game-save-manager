import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { preview } from 'vite';

const jsonForScript = (value) => JSON.stringify(value).replaceAll('<', '\\u003c');

/** A local fixture panel and the real built GUI, sharing one ephemeral origin. */
export async function serveDevice(session, device, artifacts, host) {
  const index = await readFile(join(artifacts.outDir, 'index.html'), 'utf8');
  const panel = await readFile(new URL('./panel.html', import.meta.url), 'utf8');
  const server = await preview({
    configFile: false,
    root: artifacts.appRoot,
    build: { outDir: artifacts.outDir },
    preview: {
      host: '127.0.0.1',
      port: 0,
      open: false,
      proxy: {
        '/api/v1': {
          target: host.apiBaseUrl,
          configure(proxy) {
            proxy.on('proxyReq', (request) => request.removeHeader('origin'));
          },
        },
      },
    },
    plugins: [
      {
        name: 'acceptance-device',
        configurePreviewServer(previewServer) {
          previewServer.middlewares.use((req, res, next) => {
            const origin = `http://127.0.0.1:${previewServer.httpServer.address().port}`;
            const reject = (code) => {
              res.writeHead(code).end();
            };
            // Do not expose local credentials/files to foreign sites or stale tabs.
            if (
              req.headers.host !== new URL(origin).host ||
              (req.headers.origin && req.headers.origin !== origin) ||
              req.headers['sec-fetch-site'] === 'cross-site'
            )
              return reject(403);
            const path = new URL(req.url, origin).pathname;
            const control = path.startsWith('/__acceptance/');
            if (
              (control || path.startsWith('/api/v1')) &&
              req.headers.authorization !== `Bearer ${host.token}`
            )
              return reject(401);
            res.setHeader('Cache-Control', 'no-store');
            if (path.startsWith('/api/v1')) return next();
            const html = (body) =>
              res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }).end(body);
            if (req.method === 'GET' && path === '/__acceptance') {
              return html(
                panel.replace('__DEVICE__', jsonForScript({ name: device.name, token: host.token }))
              );
            }
            if (control) {
              void (async () => {
                if (path === '/__acceptance/stop' && req.method === 'POST') {
                  res.end('Stopped; data retained');
                  session.stop();
                } else if (path === '/__acceptance/text' && req.method === 'GET') {
                  const content = await readFile(device.savePath);
                  if (content.length > 65_536) return reject(413);
                  res.writeHead(200, { 'Content-Type': 'text/plain; charset=utf-8' }).end(content);
                } else if (path === '/__acceptance/text' && req.method === 'POST') {
                  const chunks = [];
                  let length = 0;
                  for await (const chunk of req) {
                    length += chunk.length;
                    if (length > 65_536) return reject(413);
                    chunks.push(chunk);
                  }
                  // Only this fixture's fixed file is writable; no path input or product API shortcuts.
                  await writeFile(device.savePath, Buffer.concat(chunks));
                  res.end('Written');
                } else reject(404);
              })().catch(() => {
                if (!res.headersSent) res.writeHead(500);
                res.end('Fixture operation failed; inspect local data');
              });
              return;
            }
            if (req.method === 'GET' && req.headers.accept?.includes('text/html')) {
              return html(
                index.replace(
                  '<head>',
                  `<head><script>window.__RGSM_RUNTIME__=${jsonForScript({ apiBaseUrl: origin, token: host.token })}</script>`
                )
              );
            }
            next();
          });
        },
      },
    ],
  });
  session.defer(() => server.close());
  session.signal.throwIfAborted();
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`;
  return { name: device.name, gui: origin, panel: `${origin}/__acceptance` };
}
