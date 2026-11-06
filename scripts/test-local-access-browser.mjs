#!/usr/bin/env node
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { openChromium } from './local-access/chromium.mjs';
import { browserProbe, probeExpression } from './local-access/probe.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const manualBrowser = process.argv.includes('--manual-browser');
const binary = resolve(process.argv.slice(2).find(value => !value.startsWith('--')) || join(root, 'target/debug/mike'));
const pages = new Map();
let browserAgent;
const marker = 'synthetic-browser-local-access-secret';
const request = JSON.stringify({ model: 'claude-test', max_tokens: 32, stream: true, messages: [{ role: 'user', content: marker }] });
const directory = await mkdtemp(join(tmpdir(), 'mike-browser-access-'));
const servers = [];
let browser;
let mike;
let startupError;
process.on('SIGINT', cancel);
process.on('SIGTERM', cancel);
function cancel() {
  for (const page of pages.values()) page.reject(new Error('Browser regression cancelled'));
  if (mike?.pid) mike.kill('SIGTERM');
}
let diagnostics = '';
const response = [
  ['message_start', { type: 'message_start', message: { id: 'msg_security', type: 'message', role: 'assistant', content: [], model: 'claude-test', usage: { input_tokens: 1, output_tokens: 0 } } }],
  ['content_block_start', { type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } }],
  ['content_block_delta', { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: marker } }],
  ['content_block_stop', { type: 'content_block_stop', index: 0 }],
  ['message_delta', { type: 'message_delta', delta: { stop_reason: 'end_turn' }, usage: { output_tokens: 1 } }],
  ['message_stop', { type: 'message_stop' }],
].map(([event, data]) => `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`).join('');

async function server(handler) {
  const instance = createServer((req, res) => Promise.resolve(handler(req, res)).catch(error => {
    res.writeHead(500).end(String(error));
  }));
  instance.listen(0, '127.0.0.1');
  await once(instance, 'listening');
  servers.push(instance);
  return `http://127.0.0.1:${instance.address().port}`;
}

async function unusedPort() {
  const reservation = createServer();
  reservation.listen(0, '127.0.0.1');
  await once(reservation, 'listening');
  const port = reservation.address().port;
  await new Promise(resolve => reservation.close(resolve));
  return port;
}

async function waitForMike(url) {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (startupError) throw startupError;
    if (mike.exitCode !== null || mike.signalCode !== null) throw new Error(`mike exited: ${diagnostics}`);
    try {
      if ((await fetch(url + '/api/health', { signal: AbortSignal.timeout(500) })).ok) return;
    } catch {}
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error(`mike startup timed out: ${diagnostics}`);
}

try {
  const captured = [];
  const upstream = await server(async (req, res) => {
    let body = '';
    for await (const chunk of req) body += chunk;
    captured.push(body);
    res.writeHead(200, { 'content-type': 'text/event-stream' }).end(response);
  });
  const [proxyPort, inspectPort, spaPort] = await Promise.all([unusedPort(), unusedPort(), unusedPort()]);
  const proxy = `http://127.0.0.1:${proxyPort}`;
  const addresses = [`http://127.0.0.1:${inspectPort}`, `http://127.0.0.1:${spaPort}`];
  const fixture = async (req, res) => {
    res.setHeader('Access-Control-Allow-Origin', '*');
    if (req.url.startsWith('/_probe/')) {
      const page = pages.get(req.url);
      if (!page) { res.writeHead(404).end(); return; }
      const script = `(${page.expression}).then(result => { document.body.textContent = JSON.stringify(result, null, 2); return fetch('/_report/${page.id}', { method: 'POST', body: JSON.stringify({ result, agent: navigator.userAgent }) }); }).catch(error => { document.body.textContent = String(error); fetch('/_report/${page.id}', { method: 'POST', body: JSON.stringify({ error: String(error) }) }); });`;
      res.writeHead(200, { 'content-type': 'text/html' }).end(`<!doctype html><title>Local access browser regression</title><body>Running synthetic tests<script>${script.replaceAll('</script>', '<\\/script>')}</script></body>`);
      return;
    }
    if (req.url.startsWith('/_report/')) {
      const page = pages.get('/_probe/' + req.url.split('/').pop());
      let body = '';
      for await (const chunk of req) body += chunk;
      const report = JSON.parse(body);
      browserAgent = report.agent;
      if (report.error) page.reject(new Error(report.error)); else page.resolve(report.result);
      res.writeHead(200).end('recorded');
      return;
    }
    if (req.url === '/capture') {
      const result = await fetch(proxy + '/v1/messages', {
        method: 'POST', headers: { 'content-type': 'application/json', 'x-claude-code-session-id': '2e04fa85-134e-4d37-a6f7-786c587ea70a' }, body: request,
      });
      const received = await result.text();
      assert.equal(result.status, 200);
      assert.equal(received, response);
      res.writeHead(200).end('captured');
    } else {
      res.writeHead(200, { 'content-type': 'text/html' }).end('<!doctype html><title>Isolated local access regression</title><body>Only synthetic capture data.</body>');
    }
  };
  const approved = await server(fixture);
  const unrelated = await server(fixture);
  const config = join(directory, 'config.toml');
  await writeFile(config, [
    `proxy_listen_addr = "127.0.0.1:${proxyPort}"`, `inspect_listen_addr = "127.0.0.1:${inspectPort}"`,
    `spa_bind = "127.0.0.1:${spaPort}"`, `upstream_url = "${upstream}"`,
    `spa_origins = ["${approved}"]`, 'persist_enabled = false', 'smoke_run = true',
  ].join('\n'));
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('MIKE_') && key !== 'DATABASE_URL'));
  Object.assign(env, { HOME: directory, XDG_CONFIG_HOME: directory, XDG_STATE_HOME: directory });
  mike = spawn(binary, ['--config', config, 'serve'], { cwd: directory, env, stdio: ['ignore', 'pipe', 'pipe'] });
  mike.once('error', error => { startupError = error; });
  mike.stdout.on('data', chunk => diagnostics += chunk);
  mike.stderr.on('data', chunk => diagnostics += chunk);
  await waitForMike(addresses[0]);
  browser = manualBrowser ? {
    page: (origin, expression) => new Promise((resolve, reject) => {
      const id = String(pages.size + 1);
      const deadline = setTimeout(() => reject(new Error('Timed out waiting for browser page')), 120000);
      pages.set('/_probe/' + id, { id, expression,
        resolve: result => { clearTimeout(deadline); resolve(result); },
        reject: error => { clearTimeout(deadline); reject(error); },
      });
      console.error('BROWSER_PAGE ' + origin + '/_probe/' + id);
    }),
    version: async () => ({ userAgent: browserAgent, driver: 'supported browser tool' }),
    close: async () => {},
  } : await openChromium(join(directory, 'chromium'), process.env.CHROMIUM || 'chromium');
  const observations = [];
  for (const [name, origin, opaque, accepted] of [
    ['approved', approved, false, true], ['unrelated', unrelated, false, false], ['null', unrelated, true, false],
  ]) {
    const values = await browser.page(origin, probeExpression(browserProbe, [addresses, origin, marker], opaque));
    for (const value of values) {
      assert.equal(value.opened, accepted, `${name}: WebSocket open`);
      assert.equal(value.receivedCapture, accepted, `${name}: WebSocket capture`);
      assert.equal(value.readCapture, accepted, `${name}: REST capture`);
      assert.equal(value.readStatus, accepted ? 200 : 'blocked', `${name}: REST access`);
      assert.equal(value.mutationStatus, accepted ? 503 : 'blocked', `${name}: mutation guard`);
      if (opaque) assert.equal(value.origin, 'null');
    }
    observations.push({ name, values });
  }
  assert.equal(captured.length, 3);
  assert.ok(captured.every(body => body === request), 'Capture proxy changed client bytes');
  console.log(JSON.stringify({
    verdict: 'PASS', binary, binary_version: execFileSync(binary, ['--version'], { encoding: 'utf8' }).trim(), binary_sha256: createHash('sha256').update(await readFile(binary)).digest('hex'),
    candidate_source_sha: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
    source_dirty: Boolean(execFileSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' }).trim()),
    browser: await browser.version(), node: process.version, observations,
    note: 'Mutation handler control returns 503 because this isolated test intentionally has no database. The Rust middleware test proves rejected handlers never execute.',
  }, null, 2));
} finally {
  if (browser) await browser.close();
  if (mike?.pid && mike.exitCode === null && mike.signalCode === null) {
    mike.kill('SIGTERM');
    const timer = setTimeout(() => mike.kill('SIGKILL'), 5000);
    await once(mike, 'exit').catch(() => {});
    clearTimeout(timer);
  }
  for (const instance of servers) {
    instance.closeAllConnections();
    await new Promise(resolve => instance.close(resolve));
  }
  await rm(directory, { recursive: true, force: true });
}
