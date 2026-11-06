/** Native Chromium hostile-origin probes against only explicitly owned listeners.
 * Probe fetches and socket handshakes remain native, including Origin and CORS.
 * Only owned document/favicon loads use a redirect-refusing HTTP passthrough.
 */
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { validateSpec } from './journey_browser.mjs';

export const SECURITY_CHECKS = ['native_hostile_reads_rejected', 'native_null_reads_rejected',
  'native_hostile_mutations_rejected', 'native_null_mutations_rejected',
  'session_pin_unchanged', 'capture_bytes_unchanged', 'rejected_sockets_receive_no_events',
  'same_origin_reads_and_mutations', 'same_origin_live_capture_events', 'native_origin_headers'];
export function requireValue(value) { if (!value) throw new Error('boundary check failed'); }
export function origin(host, port) { return `http://${host === '::1' ? '[::1]' : host}:${port}`; }
export function validateSecuritySpec(spec) {
  validateSpec(spec);
  requireValue(['127.0.0.1', '::1'].includes(spec.target_host));
  requireValue(Number.isInteger(spec.hostile_port) && spec.hostile_port > 0 && spec.hostile_port < 65536 &&
    !spec.ports.includes(spec.hostile_port));
  return spec;
}
export function targetPaths(spec) {
  return { body: `/api/requests/${spec.request_id}/body/request`,
    session: `/api/sessions/${spec.session}`, pin: `/api/sessions/${spec.session}/pin` };
}
export function allowedSecurityRequest(request, spec) {
  const url = new URL(request.url());
  if (url.username || url.password || url.search || url.hash) return false;
  if (url.origin === origin('127.0.0.1', spec.hostile_port)) {
    return request.method() === 'GET' && ['/hostile', '/opaque'].includes(url.pathname);
  }
  const paths = targetPaths(spec);
  if (!spec.ports.slice(1).some(port => url.origin === origin(spec.target_host, port))) return false;
  if (request.method() === 'GET') return ['/api/health', '/favicon.ico', paths.body, paths.session].includes(url.pathname);
  if (url.pathname !== paths.pin) return false;
  if (request.method() === 'OPTIONS') return true;
  if (request.method() !== 'PATCH' || request.headers()['content-type'] !== 'application/json') return false;
  return ['{"pinned":true}', '{"pinned":false}'].includes(request.postData());
}
export async function guardedRoute(route, spec, state) {
  const request = route.request();
  if (!allowedSecurityRequest(request, spec)) {
    state.failure = true;
    return route.abort('blockedbyclient');
  }
  if (!request.isNavigationRequest() && new URL(request.url()).pathname !== '/favicon.ico') {
    // Keep the native browser request and response. Every probe fetch explicitly
    // uses redirect:error, so it cannot follow a redirect beyond its owned target.
    return route.continue();
  }
  let response;
  try {
    // Playwright routes only a redirect's first hop. Guard navigations separately;
    // this does not service or replace any read/mutation security probe response.
    response = await route.fetch({ maxRedirects: 0, timeout: 10000 });
    if (response.status() >= 300 && response.status() < 400) {
      state.failure = true;
      await route.abort('blockedbyclient');
    } else await route.fulfill({ response });
  } catch {
    state.failure = true;
    await route.abort('failed').catch(() => {});
  } finally { if (response) await response.dispose(); }
}
export function captureObserved(row, id) {
  const value = row.events?.[id];
  return row.opened === true && row.closed === false && row.error === false &&
    Number.isSafeInteger(id) && id > 0 && value?.text === true && value.tool === true && value.complete === true;
}
export function nativeRequest(spec, port, pathname, body, session) {
  return new Promise((resolve, reject) => {
    const headers = body ? { 'content-type': 'application/json', 'content-length': Buffer.byteLength(body),
      'anthropic-version': '2023-06-01', 'x-api-key': 'synthetic-unused-provider-key',
      'x-claude-code-session-id': session } : {};
    const request = http.request({ hostname: spec.target_host, port, path: pathname,
      method: body ? 'POST' : 'GET', headers, agent: false }, response => {
      const chunks = [];
      let bytes = 0;
      response.on('data', chunk => {
        bytes += chunk.length;
        if (bytes > 4 * 1024 * 1024) response.destroy(new Error('bounded response exceeded'));
        else chunks.push(chunk);
      });
      response.on('error', reject);
      response.on('end', () => response.statusCode === 200 ? resolve(Buffer.concat(chunks).toString()) :
        reject(new Error('owned response status')));
    });
    request.setTimeout(10000, () => request.destroy(new Error('owned request timeout')));
    request.on('error', reject);
    request.end(body);
  });
}
export async function waitFor(predicate, timeout = 10000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (await predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 25));
  }
  throw new Error('bounded boundary observation timeout');
}
export function observeNetwork(state, kind, value) {
  if (!state.rows.has(value.requestId)) {
    if (state.rows.size >= 128) { state.failure = true; return; }
    state.rows.set(value.requestId, {});
  }
  const row = state.rows.get(value.requestId);
  if (kind === 'request') { row.url = value.request.url; row.method = value.request.method; }
  if (kind === 'response') row.status = value.statusCode;
  if (kind === 'headers') {
    const headers = Object.fromEntries(Object.entries(value.headers).map(([key, val]) => [key.toLowerCase(), val]));
    row.origin = headers.origin;
    row.site = headers['sec-fetch-site'];
    row.preflight = headers['access-control-request-method'];
    if (headers.authorization || headers.cookie) state.failure = true;
  }
}
async function auditPage(context, page) {
  const state = { rows: new Map(), failure: false, socketOrigins: [] };
  const client = await context.newCDPSession(page);
  await client.send('Network.enable');
  client.on('Network.requestWillBeSent', value => observeNetwork(state, 'request', value));
  client.on('Network.requestWillBeSentExtraInfo', value => observeNetwork(state, 'headers', value));
  client.on('Network.responseReceivedExtraInfo', value => observeNetwork(state, 'response', value));
  client.on('Network.webSocketWillSendHandshakeRequest', value => {
    const headers = Object.fromEntries(Object.entries(value.request.headers).map(([key, val]) => [key.toLowerCase(), val]));
    if (state.socketOrigins.length >= 8 || headers.authorization || headers.cookie) state.failure = true;
    else state.socketOrigins.push(headers.origin);
  });
  return state;
}
export function rejectedObservation(rows, expected, method) {
  return rows.some(row => row.url === expected.url && row.method === method && row.status === 403 &&
    (expected.mode === 'no-cors' ? row.origin === undefined || row.origin === expected.origin : row.origin === expected.origin) &&
    ['same-site', 'cross-site'].includes(row.site) && (method !== 'OPTIONS' || row.preflight === 'PATCH'));
}
async function rejectedFetch(frame, audit, url, expectedOrigin, mode, mutation = false) {
  const previous = new Set(audit.rows.keys());
  const result = await frame.evaluate(async ({ url, mode, mutation }) => {
    try {
      const response = await fetch(url, { mode, redirect: 'error', credentials: 'omit',
        method: mutation ? 'PATCH' : 'GET', signal: AbortSignal.timeout(5000),
        ...(mutation ? { headers: { 'content-type': 'application/json' }, body: '{"pinned":true}' } : {}) });
      const text = await response.text();
      return { rejected: false, opaque: response.type === 'opaque' && response.status === 0 && text === '' };
    } catch { return { rejected: true, opaque: false }; }
  }, { url, mode, mutation });
  requireValue(mode === 'no-cors' ? result.opaque || result.rejected : result.rejected);
  const expected = { url, origin: expectedOrigin, mode };
  await waitFor(() => rejectedObservation([...audit.rows].filter(([id]) => !previous.has(id)).map(([, row]) => row),
    expected, mutation ? 'OPTIONS' : 'GET'));
  if (mutation) requireValue(![...audit.rows].some(([id, row]) => !previous.has(id) && row.method === 'PATCH'));
  requireValue(!audit.failure);
}
async function startSocket(frame, url, spec) {
  await frame.evaluate(({ url, text, tool }) => {
    const row = { opened: false, closed: false, error: false, frames: 0, events: {} };
    window.__boundary = row;
    const socket = new WebSocket(url);
    window.__boundarySocket = socket;
    socket.onopen = () => { row.opened = true; };
    socket.onerror = () => { row.error = true; };
    socket.onclose = () => { row.closed = true; };
    socket.onmessage = event => {
      row.frames++;
      if (row.frames > 4096 || typeof event.data !== 'string' || event.data.length > 4 * 1024 * 1024) {
        row.error = true; socket.close(); return;
      }
      try {
        const value = JSON.parse(event.data);
        if (!Number.isSafeInteger(value.id) || value.id <= 0) return;
        if (!row.events[value.id]) {
          if (Object.keys(row.events).length >= 128) { row.error = true; socket.close(); return; }
          row.events[value.id] = { text: false, tool: false, complete: false };
        }
        const capture = row.events[value.id];
        if (value.type === 'TextDelta' && value.text === text) capture.text = true;
        if (value.type === 'ToolUseStart' && value.tool_name === tool) capture.tool = true;
        if (value.type === 'ResponseComplete' && value.stop_reason === 'tool_use' &&
            value.usage?.input_tokens === 25 && value.usage?.output_tokens === 12) capture.complete = true;
      } catch { row.error = true; }
    };
  }, { url, text: spec.text, tool: spec.tool_name });
}
async function snapshot(spec) {
  const paths = targetPaths(spec);
  const [body, detail] = await Promise.all([nativeRequest(spec, spec.ports[1], paths.body),
    nativeRequest(spec, spec.ports[1], paths.session)]);
  const value = JSON.parse(detail);
  requireValue(value.summary?.id === spec.session && value.summary.pinned === false &&
    Number.isInteger(value.summary.request_count) && Array.isArray(value.requests) &&
    value.requests.every(row => Number.isSafeInteger(row.id) && row.id > 0) &&
    new Set(value.requests.map(row => row.id)).size === value.requests.length);
  return { body, count: value.summary.request_count, ids: value.requests.map(row => row.id).sort(),
    pinned: value.summary.pinned, ttl: value.summary.ttl_override };
}
async function sameOriginControl(page, base, spec) {
  const paths = targetPaths(spec);
  const response = await page.goto(`${base}/api/health`, { waitUntil: 'domcontentloaded' });
  requireValue(response.status() === 200);
  const ok = await page.evaluate(async ({ paths, body }) => {
    const read = await fetch(paths.body, { redirect: 'error', credentials: 'omit' });
    if (!read.ok || (await read.json()).text !== body) return false;
    for (const pinned of [true, false]) {
      const result = await fetch(paths.pin, { method: 'PATCH', redirect: 'error', credentials: 'omit',
        headers: { 'content-type': 'application/json' }, body: JSON.stringify({ pinned }) });
      if (!result.ok || (await result.json()).pinned !== pinned) return false;
      const detail = await fetch(paths.session, { redirect: 'error', credentials: 'omit' });
      if (!detail.ok || (await detail.json()).summary?.pinned !== pinned) return false;
    }
    return true;
  }, { paths, body: spec.request_body });
  requireValue(ok);
}
async function runBoundary(browser, spec, stage) {
  const context = await browser.newContext({ serviceWorkers: 'block', acceptDownloads: false });
  context.setDefaultNavigationTimeout(10000);
  const network = { failure: false };
  await context.route('**/*', route => guardedRoute(route, spec, network));
  const expectedSockets = spec.ports.slice(1).map(port => origin(spec.target_host, port).replace('http:', 'ws:') + '/api/ws');
  await context.addInitScript(urls => {
    const Native = window.WebSocket;
    window.WebSocket = class extends Native {
      constructor(url, protocols) {
        if (!urls.includes(new URL(String(url), location.href).href)) throw new Error('off-target socket');
        super(url, protocols);
      }
    };
  }, expectedSockets);
  try {
    const before = await snapshot(spec);
    requireValue(before.ids.length === before.count);
    const rejected = [], controls = [];
    for (const port of spec.ports.slice(1)) {
      const base = origin(spec.target_host, port);
      stage('BROWSER_BOUNDARY_SAME_ORIGIN');
      const control = await context.newPage();
      await sameOriginControl(control, base, spec);
      await startSocket(control, base.replace('http:', 'ws:') + '/api/ws', spec);
      await waitFor(() => control.evaluate(() => window.__boundary.opened));
      controls.push(control);
      for (const kind of ['hostile', 'opaque']) {
        stage('BROWSER_BOUNDARY_' + kind.toUpperCase());
        const page = await context.newPage();
        const audit = await auditPage(context, page);
        const response = await page.goto(`${origin('127.0.0.1', spec.hostile_port)}/${kind}`);
        requireValue(response.status() === 200);
        let frame = page.mainFrame();
        if (kind === 'opaque') {
          await waitFor(() => page.frames().length === 2);
          frame = page.frames().find(item => item !== page.mainFrame());
          await frame.waitForLoadState('domcontentloaded');
          requireValue(await frame.evaluate(() => window.origin) === 'null');
        }
        const expectedOrigin = kind === 'opaque' ? 'null' : origin('127.0.0.1', spec.hostile_port);
        for (const mode of ['cors', 'no-cors']) await rejectedFetch(frame, audit, base + targetPaths(spec).body, expectedOrigin, mode);
        await rejectedFetch(frame, audit, base + targetPaths(spec).pin, expectedOrigin, 'cors', true);
        await startSocket(frame, base.replace('http:', 'ws:') + '/api/ws', spec);
        await waitFor(() => frame.evaluate(() => window.__boundary.closed && window.__boundary.error));
        await waitFor(() => audit.socketOrigins.length === 1);
        requireValue(audit.socketOrigins[0] === expectedOrigin && !audit.failure);
        rejected.push(frame);
      }
    }
    stage('BROWSER_BOUNDARY_SIDE_EFFECTS');
    requireValue(JSON.stringify(await snapshot(spec)) === JSON.stringify(before));
    stage('BROWSER_BOUNDARY_LIVE_EVENT');
    await nativeRequest(spec, spec.ports[0], '/v1/messages', spec.request_body, spec.session);
    let after;
    await waitFor(async () => { after = await snapshot(spec); return after.count === before.count + 1 &&
      after.ids.length === before.ids.length + 1; });
    requireValue(after.body === before.body && after.count === before.count + 1 &&
      before.ids.every(id => after.ids.includes(id)) && after.ids.length === before.ids.length + 1 &&
      after.pinned === before.pinned && after.ttl === before.ttl);
    const added = after.ids.filter(id => !before.ids.includes(id));
    requireValue(added.length === 1 && Number.isSafeInteger(added[0]) && added[0] > 0);
    for (const page of controls) await waitFor(async () =>
      captureObserved(await page.evaluate(() => window.__boundary), added[0]));
    for (const frame of rejected) requireValue(await frame.evaluate(() => {
      const row = window.__boundary;
      return !row.opened && row.error && row.closed && row.frames === 0;
    }));
    requireValue(!network.failure);
    return { schema_version: 1, state: 'PASSED', qualified: false,
      checks: Object.fromEntries(SECURITY_CHECKS.map(key => [key, true])),
      listener_family: spec.target_host === '::1' ? 'ipv6' : 'ipv4', listener_count: 2,
      rejected_socket_count: 4, rejected_read_count: 8, rejected_mutation_count: 4,
      browser_version: browser.version() };
  } finally { await context.close(); }
}
export async function main() {
  let stage = 'BROWSER_BOUNDARY_SPEC_INVALID';
  const timer = setTimeout(() => process.stdout.write(JSON.stringify({ schema_version: 1, state: 'FAILED',
    qualified: false, code: 'BROWSER_BOUNDARY_TIMEOUT' }) + '\n', () => process.exit(0)), 90000);
  try {
    requireValue(process.argv.length === 3 && fs.statSync(process.argv[2]).size <= 1024 * 1024);
    const spec = validateSecuritySpec(JSON.parse(fs.readFileSync(process.argv[2], 'utf8')));
    stage = 'BROWSER_TOOLING_UNAVAILABLE';
    const { chromium } = await import(pathToFileURL(spec.playwright).href);
    stage = 'BROWSER_LAUNCH_UNAVAILABLE';
    const browser = await chromium.launch({ executablePath: spec.chromium, headless: true,
      chromiumSandbox: true, timeout: 15000 });
    try { process.stdout.write(JSON.stringify(await runBoundary(browser, spec, value => { stage = value; })) + '\n'); }
    finally { await browser.close(); }
  } catch {
    process.stdout.write(JSON.stringify({ schema_version: 1, state: 'FAILED', qualified: false, code: stage }) + '\n');
  } finally { clearTimeout(timer); }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
