/** Installed-archive browser helper. No build, server, download or fallback launch.
 * Invoked only inside the owned Python persistent browser journey; never a standalone AC pass.
 * Candidate executables are trusted. Routing is scope control, not an OS sandbox.
 */
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { onboardingChecks, ONBOARDING_CHECKS } from './journey_browser_onboarding.mjs';

const DOCUMENTS = ['installed-setup', 'configuration-and-import', 'connections-and-privacy',
  'recovery-and-maintenance'];
const CHECKS = ['capture_request_dom', 'capture_response_dom', 'direct_link_reload',
  'live_websocket_capture', 'live_request_count_dom', 'help_navigation', 'help_selected_endpoints',
  'packaged_docs_reload', 'same_origin_assets', 'no_console_or_page_errors', ...ONBOARDING_CHECKS];
const MAX_BYTES = 4 * 1024 * 1024;

function requireValue(condition) {
  if (!condition) throw new Error('check failed');
}

export function validateSpec(spec) {
  requireValue(spec?.schema_version === 1 && Array.isArray(spec.ports) && spec.ports.length === 3);
  requireValue(new Set(spec.ports).size === 3 && spec.ports.every(p => Number.isInteger(p) && p > 0 && p < 65536));
  requireValue(typeof spec.session === 'string' && /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(spec.session));
  requireValue(Number.isSafeInteger(spec.request_id) && spec.request_id > 0);
  for (const name of ['chromium', 'playwright']) {
    requireValue(typeof spec[name] === 'string' && path.isAbsolute(spec[name]) && fs.statSync(spec[name]).isFile());
  }
  requireValue(typeof spec.request_body === 'string' && spec.request_body.length < 1024 * 1024);
  requireValue(JSON.parse(spec.request_body).stream === true);
  for (const name of ['text', 'tool_name', 'tool_result']) {
    requireValue(typeof spec[name] === 'string' && spec[name].length > 0 && spec[name].length < 1024);
  }
  return spec;
}

function origin(port) { return `http://127.0.0.1:${port}`; }

function ownedHttp(port, pathname, body, session, method = body ? 'POST' : 'GET') {
  return new Promise((resolve, reject) => {
    const headers = body ? { 'content-type': 'application/json', 'content-length': Buffer.byteLength(body),
      'anthropic-version': '2023-06-01', 'x-api-key': 'synthetic-unused-provider-key',
      'x-claude-code-session-id': session } : {};
    const req = http.request({ hostname: '127.0.0.1', port, path: pathname,
      method, headers, agent: false }, response => {
      const chunks = [];
      let count = 0;
      response.on('data', chunk => {
        count += chunk.length;
        if (count > MAX_BYTES) response.destroy(new Error('bounded response exceeded'));
        else chunks.push(chunk);
      });
      response.on('error', reject);
      response.on('end', () => {
        if (response.statusCode !== 200) reject(new Error('owned HTTP status'));
        else resolve(Buffer.concat(chunks).toString('utf8'));
      });
    });
    req.setTimeout(10000, () => req.destroy(new Error('owned HTTP timeout')));
    req.on('error', reject);
    req.end(body);
  });
}

async function waitFor(predicate, timeout = 10000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (await predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 40));
  }
  throw new Error('bounded observation timeout');
}

export function observeFrame(state, payload, spec) {
  if (++state.frameCount > 4096 || Buffer.byteLength(payload) > MAX_BYTES) {
    state.networkFailure = true;
    return;
  }
  let event;
  try { event = JSON.parse(payload.toString()); } catch { state.networkFailure = true; return; }
  if (!Number.isSafeInteger(event?.id) || event.id <= 0) return;
  if (!state.events.has(event.id)) {
    if (state.events.size >= 128) { state.networkFailure = true; return; }
    state.events.set(event.id, { text: false, tool: false, complete: false });
  }
  const row = state.events.get(event.id);
  if (event.type === 'TextDelta' && event.text === spec.text) row.text = true;
  if (event.type === 'ToolUseStart' && event.tool_name === spec.tool_name) row.tool = true;
  if (event.type === 'ResponseComplete' && event.stop_reason === 'tool_use' &&
      event.usage?.input_tokens === 25 && event.usage?.output_tokens === 12) row.complete = true;
}

export function allowedBrowserRequest(request, base) {
  const url = new URL(request.url());
  if (url.origin !== base || url.username || url.password || /\/@vite|\/@id|\/@fs/.test(url.pathname)) return false;
  if (['GET', 'HEAD'].includes(request.method())) return true;
  if (request.method() !== 'POST' || url.pathname !== '/api/diagnostics/spa_delta_fallback' || url.search) return false;
  if (request.headers()['content-type']?.split(';')[0].trim() !== 'application/json') return false;
  const body = request.postData();
  if (typeof body !== 'string' || Buffer.byteLength(body) > 256) return false;
  try {
    const value = JSON.parse(body);
    return value !== null && !Array.isArray(value) && Object.keys(value).length === 1 &&
      ['predecessor-not-loaded', 'compaction', 'lineage-broken', 'chrome-strip-failed', 'parse-error'].includes(value.reason);
  } catch { return false; }
}

async function restrictBrowser(context, spec, state) {
  const base = origin(spec.ports[2]);
  const wsUrl = `ws://127.0.0.1:${spec.ports[2]}/api/ws`;
  await context.route('**/*', async route => {
    const request = route.request();
    if (!allowedBrowserRequest(request, base)) {
      state.networkFailure = true;
      return route.abort('blockedbyclient');
    }
    // Playwright only routes the first hop of a redirected browser request.
    // Fetch without redirects, then forward the actual owned-service response.
    // This is a transport guard, not a fixture/mock response or a CORS verdict.
    try {
      const response = await route.fetch({ maxRedirects: 0, timeout: 10000 });
      if (response.status() >= 300 && response.status() < 400) {
        state.networkFailure = true;
        await route.abort('blockedbyclient');
      } else {
        await route.fulfill({ response });
      }
      await response.dispose();
    } catch {
      state.networkFailure = true;
      await route.abort('failed').catch(() => {});
    }
  });
  // Keep the native socket/frames. Guard the constructor only to refuse an
  // off-target socket before it connects; no messages or API replies are mocked.
  await context.addInitScript(expected => {
    const NativeWebSocket = window.WebSocket;
    window.WebSocket = class extends NativeWebSocket {
      constructor(url, protocols) {
        if (new URL(String(url), location.href).href !== expected) {
          window.__mikeReleaseUnexpectedSocket = true;
          throw new Error('release browser refuses off-target socket');
        }
        super(url, protocols);
      }
    };
  }, wsUrl);
  context.on('page', page => {
    page.on('pageerror', () => { state.pageErrors += 1; });
    page.on('console', message => { if (message.type() === 'error') state.consoleErrors += 1; });
    page.on('response', response => {
      if (response.request().resourceType() === 'script') {
        const url = new URL(response.url());
        if (url.origin !== base || !url.pathname.startsWith('/_app/') || response.status() !== 200) {
          state.networkFailure = true;
        } else if (++state.scriptCount > 4096) state.networkFailure = true;
      }
    });
    page.on('websocket', socket => {
      if (socket.url() !== wsUrl) { state.networkFailure = true; return; }
      socket.on('framereceived', event => observeFrame(state, event.payload, spec));
      socket.on('socketerror', () => { state.networkFailure = true; });
    });
  });
}

async function gotoOwned(page, url) {
  const response = await page.goto(url, { waitUntil: 'domcontentloaded' });
  requireValue(response?.status() === 200 && page.url() === url);
}

async function bodyDrawer(page, url, kind, expected) {
  await gotoOwned(page, `${url}?drawer=${kind}`);
  const dialog = page.getByRole('dialog', { name: kind === 'request' ? 'Request body' : 'Response body' });
  await dialog.waitFor({ state: 'visible' });
  const raw = dialog.getByRole('tab', { name: 'Raw JSON', exact: true });
  await waitFor(async () => {
    if (await raw.count() && await raw.getAttribute('aria-selected') !== 'true') await raw.click();
    return (await dialog.innerText()).includes(expected);
  });
}

async function liveCapture(page, spec, state, stage) {
  const base = origin(spec.ports[2]);
  const deep = `${base}/sessions/${spec.session}/requests/${spec.request_id}`;
  await gotoOwned(page, deep);
  await page.locator('[data-slot="connection-status"][data-state="connected"]').waitFor({ state: 'visible' });
  const detail = page.getByRole('region', { name: 'Request detail', exact: true });
  // The section has an accessible name and maps to region in Chromium.
  await waitFor(async () => /\b1\s+requests\b/.test(await detail.locator('.detail-head').innerText()));
  state.events.clear();
  await ownedHttp(spec.ports[0], '/v1/messages', spec.request_body, spec.session);
  let next;
  await waitFor(async () => {
    const rows = JSON.parse(await ownedHttp(spec.ports[1], '/api/requests'));
    requireValue(Array.isArray(rows));
    const matching = rows.filter(row => row.session_id === spec.session && row.id !== spec.request_id);
    requireValue(matching.length <= 1);
    next = matching[0];
    return next?.status === 'Complete';
  });
  requireValue(Number.isSafeInteger(next.id) && next.id > 0);
  await waitFor(() => {
    const event = state.events.get(next.id);
    return event?.text && event.tool && event.complete;
  });
  stage('BROWSER_LIVE_DOM');
  // No refresh or navigation between the second capture and this DOM assertion.
  await waitFor(async () => /\b2\s+requests\b/.test(await detail.locator('.detail-head').innerText()));
}

async function helpChecks(page, spec, stage) {
  const base = origin(spec.ports[2]);
  await gotoOwned(page, `${base}/help`);
  const help = page.locator('[data-slot="help-index"]');
  await help.waitFor({ state: 'visible' });
  await help.locator('a[href="/help/guides/getting-started"]').click();
  await page.waitForURL(`${base}/help/guides/getting-started`);
  stage('BROWSER_HELP_ENDPOINTS');
  for (const port of spec.ports) {
    await waitFor(async () => (await page.locator('body').innerText()).includes(origin(port)));
  }
  requireValue((await page.locator('body').innerText()).includes(`ANTHROPIC_BASE_URL='${origin(spec.ports[0])}' claude`));
  stage('BROWSER_PACKAGED_DOCS');
  for (const slug of DOCUMENTS) {
    await gotoOwned(page, `${base}/help`);
    await page.locator(`[data-slot="help-index"] a[href="/help/docs/${slug}"]`).click();
    await page.waitForURL(`${base}/help/docs/${slug}`);
    const document = page.locator('[data-slot="packaged-document"]');
    await document.waitFor({ state: 'visible' });
    const text = await document.innerText();
    requireValue(text.length > 200 && text.length < MAX_BYTES);
    const response = await page.reload({ waitUntil: 'domcontentloaded' });
    requireValue(response?.status() === 200);
    await waitFor(async () => (await document.innerText()) === text);
  }
}

async function browserChecks(spec, setStage) {
  setStage('BROWSER_TOOLING_UNAVAILABLE');
  const { chromium } = await import(pathToFileURL(spec.playwright).href);
  setStage('BROWSER_LAUNCH_UNAVAILABLE');
  // Explicit executable only. Playwright cannot install a browser from launch().
  // Keep Chromium's sandbox enabled; never retry with --no-sandbox or a remote browser.
  const browser = await chromium.launch({ executablePath: spec.chromium, headless: true,
    chromiumSandbox: true, timeout: 15000 });
  try {
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 },
      serviceWorkers: 'block', acceptDownloads: false });
    context.setDefaultTimeout(10000);
    context.setDefaultNavigationTimeout(15000);
    const state = { frameCount: 0, scriptCount: 0, pageErrors: 0, consoleErrors: 0,
      networkFailure: false, events: new Map() };
    await restrictBrowser(context, spec, state);
    const page = await context.newPage();
    const deep = `${origin(spec.ports[2])}/sessions/${spec.session}/requests/${spec.request_id}`;
    setStage('BROWSER_CAPTURE_DOM');
    await bodyDrawer(page, deep, 'request', spec.tool_result);
    await bodyDrawer(page, deep, 'response', spec.text);
    setStage('BROWSER_DIRECT_LINK_RELOAD');
    const response = await page.reload({ waitUntil: 'domcontentloaded' });
    requireValue(response?.status() === 200);
    await waitFor(async () => (await page.getByRole('dialog', { name: 'Response body' }).innerText()).includes(spec.text));
    setStage('BROWSER_LIVE_EVENT');
    await liveCapture(page, spec, state, setStage);
    setStage('BROWSER_HELP_NAVIGATION');
    await helpChecks(page, spec, setStage);
    await onboardingChecks(page, spec, { ownedHttp, gotoOwned, waitFor, stage: setStage });
    setStage('BROWSER_NETWORK_SCOPE');
    requireValue(!state.networkFailure && state.scriptCount > 0 &&
      !(await page.evaluate(() => window.__mikeReleaseUnexpectedSocket === true)));
    setStage('BROWSER_CONSOLE_OR_PAGE_ERROR');
    requireValue(state.consoleErrors === 0 && state.pageErrors === 0);
    return { schema_version: 1, state: 'PASSED', qualified: false,
      checks: Object.fromEntries(CHECKS.map(name => [name, true])),
      packaged_document_count: DOCUMENTS.length, browser_version: browser.version(),
      websocket_frame_count: state.frameCount, script_response_count: state.scriptCount };
  } finally {
    await browser.close();
  }
}

export async function main() {
  let stage = 'BROWSER_SPEC_INVALID';
  const timer = setTimeout(() => {
    process.stdout.write(JSON.stringify({ schema_version: 1, state: 'FAILED', qualified: false,
      code: 'BROWSER_TIMEOUT' }) + '\n', () => process.exit(0));
  }, 100000);
  try {
    requireValue(process.argv.length === 3 && fs.statSync(process.argv[2]).size <= 1024 * 1024);
    const spec = validateSpec(JSON.parse(fs.readFileSync(process.argv[2], 'utf8')));
    const result = await browserChecks(spec, value => { stage = value; });
    process.stdout.write(JSON.stringify(result) + '\n');
  } catch {
    // Browser/site/tool errors may contain fixture bodies, URLs, paths or secrets.
    process.stdout.write(JSON.stringify({ schema_version: 1, state: 'FAILED', qualified: false, code: stage }) + '\n');
  } finally {
    clearTimeout(timer);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
