/** Offline contracts only: no browser import, networking, or listener creation. */
import assert from 'node:assert/strict';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { allowedSecurityRequest, captureObserved, guardedRoute, observeNetwork, origin, rejectedObservation,
  targetPaths, validateSecuritySpec } from './journey_browser_security.mjs';
import { fleetLocationMatches, PROJECTS, validateProjects } from './journey_browser_onboarding.mjs';
const spec = { schema_version: 1, ports: [11001, 11002, 11003], hostile_port: 11004,
  target_host: '127.0.0.1', session: '12345678-1234-1234-1234-123456789abc', request_id: 42,
  chromium: process.execPath, playwright: fileURLToPath(import.meta.url), request_body: '{"stream":true}',
  text: 'SYNTHETIC_TEXT', tool_name: 'synthetic_tool', tool_result: 'SYNTHETIC_RESULT' };
const request = (url, method = 'GET', body = '{"pinned":true}', type = 'application/json') => ({
  url: () => url, method: () => method, postData: () => body, headers: () => ({ 'content-type': type }) });
const paths = targetPaths(spec);

test('security spec selects only exact loopback addresses and a distinct owned hostile port', () => {
  assert.equal(validateSecuritySpec(spec), spec);
  assert.equal(validateSecuritySpec({ ...spec, target_host: '::1' }).target_host, '::1');
  assert.equal(origin('::1', 11002), 'http://[::1]:11002');
  for (const change of [{ hostile_port: true }, { hostile_port: 65536 }, { hostile_port: 11001 },
    { target_host: '0.0.0.0' }, { target_host: '::' }, { target_host: 'localhost' }, { target_host: 'foreign.test' }]) {
    assert.throws(() => validateSecuritySpec({ ...spec, ...change }));
  }
});

test('native route scope permits only exact fixture, reads, pin values and preflights on both listener families', () => {
  for (const host of ['127.0.0.1', '::1']) {
    const current = { ...spec, target_host: host };
    for (const port of spec.ports.slice(1)) {
      const base = origin(host, port);
      for (const pathname of ['/api/health', paths.body, paths.session]) {
        assert.equal(allowedSecurityRequest(request(base + pathname), current), true);
      }
      assert.equal(allowedSecurityRequest(request(base + paths.pin, 'OPTIONS'), current), true);
      for (const body of ['{"pinned":true}', '{"pinned":false}']) {
        assert.equal(allowedSecurityRequest(request(base + paths.pin, 'PATCH', body), current), true);
      }
    }
  }
  assert.equal(allowedSecurityRequest(request('http://127.0.0.1:11004/hostile'), spec), true);
  assert.equal(allowedSecurityRequest(request('http://127.0.0.1:11004/opaque'), spec), true);
  for (const [url, method, body, type] of [
    ['http://127.0.0.1:11001/v1/messages'], ['http://127.0.0.1:11005' + paths.body],
    ['http://foreign.test:11002' + paths.body], ['http://127.0.0.1:11002/api/settings'],
    ['http://127.0.0.1:11004/hostile?next=http://foreign.test'],
    ['http://127.0.0.1:11002' + paths.pin, 'PATCH', '{"pinned":true,"extra":"private"}'],
    ['http://127.0.0.1:11002' + paths.pin, 'PATCH', '{"pinned":true}', 'text/plain'],
    ['http://127.0.0.1:11002' + paths.pin, 'POST'],
    ['http://user:private@127.0.0.1:11002' + paths.body],
    ['http://127.0.0.1:11002' + paths.body + '#fragment']
  ]) assert.equal(allowedSecurityRequest(request(url, method, body, type), spec), false);
});

test('native CDP evidence requires actual 403 plus browser Origin and fetch metadata, not only a fetch exception', () => {
  const expected = { url: 'http://127.0.0.1:11002' + paths.body, origin: 'null', mode: 'cors' };
  const row = { url: expected.url, method: 'GET', status: 403, origin: 'null', site: 'cross-site' };
  assert.equal(rejectedObservation([row], expected, 'GET'), true);
  for (const edit of [{ status: 200 }, { status: 0 }, { origin: undefined }, { origin: 'http://foreign.test' },
    { site: 'same-origin' }, { url: expected.url + '?other=1' }, { method: 'OPTIONS' }]) {
    assert.equal(rejectedObservation([{ ...row, ...edit }], expected, 'GET'), false);
  }
  assert.equal(rejectedObservation([{ ...row, origin: undefined }], { ...expected, mode: 'no-cors' }, 'GET'), true);
  assert.equal(rejectedObservation([{ ...row, method: 'OPTIONS', preflight: 'PATCH' }], expected, 'OPTIONS'), true);
  assert.equal(rejectedObservation([{ ...row, method: 'OPTIONS', preflight: 'DELETE' }], expected, 'OPTIONS'), false);
});

test('CDP events are bounded and retain no body, private header, cookie or error text', () => {
  const state = { rows: new Map(), failure: false };
  observeNetwork(state, 'response', { requestId: '1', statusCode: 403 });
  observeNetwork(state, 'headers', { requestId: '1', headers: { Origin: 'null', 'Sec-Fetch-Site': 'cross-site',
    'X-Private': 'PRIVATE_MARKER' } });
  observeNetwork(state, 'request', { requestId: '1', request: { url: 'http://127.0.0.1:11002/api/health',
    method: 'GET', postData: 'PRIVATE_MARKER' } });
  assert.equal(rejectedObservation([...state.rows.values()], { url: 'http://127.0.0.1:11002/api/health',
    origin: 'null', mode: 'cors' }, 'GET'), true);
  assert.equal(JSON.stringify([...state.rows]).includes('PRIVATE_MARKER'), false);
  observeNetwork(state, 'headers', { requestId: '2', headers: { Cookie: 'PRIVATE_MARKER' } });
  assert.equal(state.failure, true);
  for (let id = 3; id <= 129; id++) observeNetwork(state, 'response', { requestId: String(id), statusCode: 403 });
  assert.equal(state.rows.size, 128);
});

test('onboarding fixture validates empty, single and multiple project sets without inventing rows', () => {
  assert.deepEqual(validateProjects([], []), []);
  assert.deepEqual(validateProjects([{ project_path: PROJECTS[1] }], [PROJECTS[1]]), [PROJECTS[1]]);
  assert.deepEqual(validateProjects(PROJECTS.map(project_path => ({ project_path })).reverse(), PROJECTS), [...PROJECTS].reverse());
  for (const rows of [null, 'private', [{ project_path: 'foreign' }],
    [{ project_path: PROJECTS[0] }, { project_path: PROJECTS[0] }]]) {
    assert.throws(() => validateProjects(rows, PROJECTS));
  }
});


test('security probes use native continue, while document redirects are rejected before any second hop', async () => {
  const calls = [];
  const response = { status: () => 200, dispose: async () => calls.push('dispose') };
  const route = { request: () => ({ ...request('http://127.0.0.1:11002' + paths.body), isNavigationRequest: () => false }),
    continue: async () => calls.push('native'), fetch: async options => { calls.push(options); return response; },
    fulfill: async () => calls.push('fulfill'), abort: async code => calls.push(code) };
  const state = { failure: false };
  await guardedRoute(route, spec, state);
  assert.deepEqual(calls, ['native']);
  route.request = () => ({ ...request('http://127.0.0.1:11004/hostile'), isNavigationRequest: () => true });
  calls.length = 0;
  await guardedRoute(route, spec, state);
  assert.deepEqual(calls, [{ maxRedirects: 0, timeout: 10000 }, 'fulfill', 'dispose']);
  response.status = () => 302;
  calls.length = 0;
  await guardedRoute(route, spec, state);
  assert.equal(state.failure, true);
  assert.deepEqual(calls, [{ maxRedirects: 0, timeout: 10000 }, 'blockedbyclient', 'dispose']);
});

test('positive socket proof requires the newly captured request identity, never matching historical frames', () => {
  const row = { opened: true, closed: false, error: false, events: { 42: { text: true, tool: true, complete: true } } };
  assert.equal(captureObserved(row, 42), true);
  assert.equal(captureObserved(row, 43), false);
  for (const change of [{ opened: false }, { closed: true }, { error: true },
    { events: { 42: { text: true, tool: true, complete: false } } }]) {
    assert.equal(captureObserved({ ...row, ...change }, 42), false);
  }
});


test('fleet reload accepts hydrated window=24 canonicalization while preserving exact project and owned origin', () => {
  const base = 'http://127.0.0.1:11003';
  const requested = `${base}/fleet?project=${encodeURIComponent(PROJECTS[1])}`;
  const canonical = new URL(requested);
  canonical.searchParams.set('window', '24');
  assert.notEqual(canonical.href, requested);
  assert.equal(fleetLocationMatches(canonical.href, base, PROJECTS[1]), true);
  assert.equal(fleetLocationMatches(`${base}/fleet?window=24&project=${encodeURIComponent(PROJECTS[1])}`,
    base, PROJECTS[1]), true);
  assert.equal(fleetLocationMatches(`${base}/fleet?window=24`, base, ''), true);
  assert.equal(fleetLocationMatches(requested, base, PROJECTS[1]), false);
  for (const [key, value] of [['project', PROJECTS[0]], ['window', '48']]) {
    const changed = new URL(canonical);
    changed.searchParams.set(key, value);
    assert.equal(fleetLocationMatches(changed.href, base, PROJECTS[1]), false);
  }
  for (const changed of [canonical.href.replace('11003', '11004'),
    canonical.href.replace('/fleet?', '/sessions?'), `${canonical.href}&window=24`,
    `${canonical.href}&project=${encodeURIComponent(PROJECTS[1])}`, `${canonical.href}#other`, 'invalid']) {
    assert.equal(fleetLocationMatches(changed, base, PROJECTS[1]), false);
  }
});
