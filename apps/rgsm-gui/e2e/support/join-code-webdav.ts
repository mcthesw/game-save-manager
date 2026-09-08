import { createServer } from 'node:http';
import { once } from 'node:events';

/** Loopback-only WebDAV fixture for joining metadata. Not a provider compatibility suite. */
export async function startJoinCodeWebDav() {
  const files = new Map<string, Buffer>();
  const directories = new Set(['/']);
  const writes: string[] = [];
  const username = 'join-test';
  const password = 'join-test-secret';
  const authorization = `Basic ${Buffer.from(`${username}:${password}`).toString('base64')}`;
  const xml = (text: string) => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;');
  const server = createServer(async (request, response) => {
    try {
      if (request.headers.authorization !== authorization) {
        response.writeHead(401, { 'WWW-Authenticate': 'Basic realm="test"' }).end();
        return;
      }
      const path = decodeURIComponent(new URL(request.url!, 'http://localhost').pathname);
      const directory = path.endsWith('/') ? path : `${path}/`;
      switch (request.method) {
        case 'GET':
        case 'HEAD': {
          const content = files.get(path);
          if (!content) {
            response.writeHead(404).end();
            return;
          }
          response.writeHead(200, { 'Content-Length': content.length });
          response.end(request.method === 'HEAD' ? undefined : content);
          return;
        }
        case 'PUT': {
          if (!directories.has(path.slice(0, path.lastIndexOf('/') + 1))) {
            response.writeHead(409).end();
            return;
          }
          const chunks: Buffer[] = [];
          for await (const chunk of request) chunks.push(Buffer.from(chunk));
          files.set(path, Buffer.concat(chunks));
          writes.push(path);
          response.writeHead(201).end();
          return;
        }
        case 'MKCOL':
          directories.add(directory);
          writes.push(directory);
          response.writeHead(201).end();
          return;
        case 'DELETE':
          files.delete(path);
          directories.delete(directory);
          writes.push(path);
          response.writeHead(204).end();
          return;
        case 'PROPFIND': {
          const isDirectory = directories.has(directory);
          if (!isDirectory && !files.has(path)) {
            response.writeHead(404).end();
            return;
          }
          const paths = [isDirectory ? directory : path];
          if (isDirectory && request.headers.depth !== '0') {
            paths.push(
              ...[...directories, ...files.keys()].filter(
                (entry) =>
                  entry !== directory &&
                  entry.startsWith(directory) &&
                  !entry.slice(directory.length).replace(/\/$/, '').includes('/')
              )
            );
          }
          const body = paths
            .map(
              (entry) =>
                `<D:response><D:href>${xml(encodeURI(entry))}</D:href><D:propstat><D:prop><D:resourcetype>${directories.has(entry) ? '<D:collection/>' : ''}</D:resourcetype><D:getcontentlength>${files.get(entry)?.length ?? 0}</D:getcontentlength><D:getlastmodified>Tue, 01 Sep 2026 00:00:00 GMT</D:getlastmodified></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>`
            )
            .join('');
          response.writeHead(207, { 'Content-Type': 'application/xml' });
          response.end(
            `<?xml version="1.0"?><D:multistatus xmlns:D="DAV:">${body}</D:multistatus>`
          );
          return;
        }
        default:
          response.writeHead(405).end();
      }
    } catch {
      response.writeHead(500).end();
    }
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Missing loopback address');
  return {
    backend: {
      type: 'WebDAV' as const,
      endpoint: `http://127.0.0.1:${address.port}`,
      username,
      password,
    },
    files,
    writes,
    close: () =>
      new Promise<void>((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
        server.closeAllConnections();
      }),
  };
}
