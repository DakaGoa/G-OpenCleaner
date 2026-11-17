'use strict';

/**
 * OpenClean local server.
 *
 * Binds to loopback only. Every API call must carry the per-run session token
 * injected into index.html, which stops other local pages (or a stray browser
 * tab) from driving deletions through this process.
 */

const http = require('http');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const api = require('./api');

const PUBLIC_DIR = path.join(__dirname, '..', 'public');
const TOKEN = crypto.randomBytes(24).toString('base64url');
const HOST = '127.0.0.1';
// Deliberately NOT process.env.PORT: launchers and hosting runners set that to
// 0 or to a service port. OpenClean owns its own fixed loopback port.
const requestedPort = Number(process.env.OPENCLEAN_PORT || 0);
const BASE_PORT = Number.isInteger(requestedPort) && requestedPort >= 1024 ? requestedPort : 7317;

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
};

function sendJson(res, status, payload) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {
    'content-type': 'application/json; charset=utf-8',
    'cache-control': 'no-store',
    'content-length': Buffer.byteLength(body),
  });
  res.end(body);
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    let size = 0;
    const chunks = [];
    req.on('data', (chunk) => {
      size += chunk.length;
      if (size > 8 * 1024 * 1024) {
        reject(new Error('Request body too large.'));
        req.destroy();
        return;
      }
      chunks.push(chunk);
    });
    req.on('end', () => {
      if (!chunks.length) return resolve({});
      try {
        resolve(JSON.parse(Buffer.concat(chunks).toString('utf8')));
      } catch {
        reject(new Error('Invalid JSON body.'));
      }
    });
    req.on('error', reject);
  });
}

function serveFile(res, filePath, status = 200) {
  const ext = path.extname(filePath).toLowerCase();
  fs.readFile(filePath, (err, data) => {
    if (err) {
      sendJson(res, 404, { error: 'Not found' });
      return;
    }
    res.writeHead(status, {
      'content-type': MIME[ext] || 'application/octet-stream',
      'cache-control': 'no-store',
      'content-length': data.length,
    });
    res.end(data);
  });
}

function isLoopbackOrigin(origin) {
  if (!origin) return true;
  try {
    const parsed = new URL(origin);
    return ['127.0.0.1', 'localhost', '[::1]'].includes(parsed.hostname);
  } catch {
    return false;
  }
}

const server = http.createServer(async (req, res) => {
  let parsed;
  try {
    parsed = new URL(req.url, `http://${HOST}`);
  } catch {
    sendJson(res, 400, { error: 'Bad request' });
    return;
  }

  const pathname = decodeURIComponent(parsed.pathname);

  if (pathname.startsWith('/api/')) {
    if (req.headers['x-cc-token'] !== TOKEN) {
      sendJson(res, 403, { error: 'Invalid session token. Reload OpenClean.' });
      return;
    }
    if (!isLoopbackOrigin(req.headers.origin)) {
      sendJson(res, 403, { error: 'Cross-origin requests are refused.' });
      return;
    }

    try {
      const body = req.method === 'POST' ? await readBody(req) : {};
      const result = await api.route(req.method, pathname, parsed.searchParams, body);
      sendJson(res, 200, result === undefined ? { ok: true } : result);
    } catch (err) {
      const status = err.status || (err.code === 'CONFIRM' ? 428 : 400);
      sendJson(res, status, { error: err.message || 'Request failed.' });
    }
    return;
  }

  if (pathname === '/' || pathname === '/index.html') {
    fs.readFile(path.join(PUBLIC_DIR, 'index.html'), 'utf8', (err, html) => {
      if (err) {
        res.writeHead(500, { 'content-type': 'text/plain' });
        res.end('UI files are missing. Reinstall OpenClean.');
        return;
      }
      const rendered = html.replace('__OPEN_CLEAN_TOKEN__', TOKEN);
      res.writeHead(200, {
        'content-type': 'text/html; charset=utf-8',
        'cache-control': 'no-store',
        'content-length': Buffer.byteLength(rendered),
      });
      res.end(rendered);
    });
    return;
  }

  // Static assets, with traversal protection.
  const requested = path.normalize(path.join(PUBLIC_DIR, pathname));
  if (!requested.startsWith(PUBLIC_DIR)) {
    sendJson(res, 403, { error: 'Forbidden' });
    return;
  }
  serveFile(res, requested);
});

function listen(port, attempt = 0) {
  server.once('error', (err) => {
    if (err.code === 'EADDRINUSE' && attempt < 12) {
      listen(port + 1, attempt + 1);
      return;
    }
    console.error('Could not start OpenClean:', err.message);
    process.exit(1);
  });
  server.listen(port, HOST, () => {
    const url = `http://${HOST}:${port}`;
    console.log('');
    console.log('  OpenClean is running');
    console.log(`  ${url}`);
    console.log('');
    console.log('  Press Ctrl+C to stop.');
    console.log('');
  });
}

process.on('uncaughtException', (err) => {
  console.error('Unexpected error:', err && err.message ? err.message : err);
});
process.on('unhandledRejection', (err) => {
  console.error('Unexpected rejection:', err && err.message ? err.message : err);
});

listen(BASE_PORT);
