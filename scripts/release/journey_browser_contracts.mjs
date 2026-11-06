/** Offline script contracts only: no Playwright import, socket or browser launch. */
import assert from 'node:assert/strict';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { allowedBrowserRequest, observeFrame, validateSpec } from './journey_browser.mjs';

const spec = {
  schema_version: 1, ports: [11001, 11002, 11003],
  session: '12345678-1234-1234-1234-123456789abc', request_id: 42,
  chromium: process.execPath, playwright: fileURLToPath(import.meta.url),
  request_body: '{"stream":true}', text: 'SYNTHETIC_TEXT',
  tool_name: 'synthetic_tool', tool_result: 'SYNTHETIC_TOOL_RESULT'
};
const state = () => ({ frameCount: 0, networkFailure: false, events: new Map() });
const frame = (value, row, fixture = spec) => observeFrame(row, JSON.stringify(value), fixture);

test('spec requires explicit existing files, valid UUID, safe request ID and three distinct ports', () => {
  assert.equal(validateSpec(spec), spec);
  for (const changed of [
    { ports: [11001, 11001, 11003] }, { ports: [true, 11002, 11003] }, { ports: [11001, 11002, 65536] },
    { session: 'https://remote.invalid/' }, { request_id: true }, { request_id: Number.MAX_SAFE_INTEGER + 1 },
    { chromium: 'chromium' }, { playwright: 'playwright' }, { request_body: '{"stream":false}' },
    { text: '' }, { schema_version: true }
  ]) assert.throws(() => validateSpec({ ...spec, ...changed }));
});

test('matching native frames retain only booleans and capture identity, not bodies', () => {
  const observed = state();
  frame({ id: 43, type: 'TextDelta', text: spec.text }, observed);
  frame({ id: 43, type: 'ToolUseStart', tool_name: spec.tool_name, tool_id: 'private-tool-id' }, observed);
  frame({ id: 43, type: 'ResponseComplete', stop_reason: 'tool_use', usage: { input_tokens: 25, output_tokens: 12 } }, observed);
  assert.deepEqual(observed.events.get(43), { text: true, tool: true, complete: true });
  assert.equal(observed.frameCount, 3);
  assert.equal(observed.networkFailure, false);
  assert.equal(JSON.stringify([...observed.events]).includes(spec.text), false);
  assert.equal(JSON.stringify([...observed.events]).includes('private-tool-id'), false);
});

test('unmatched text, usage, tool or identity cannot be event success', () => {
  const observed = state();
  frame({ id: 43, type: 'TextDelta', text: 'wrong text' }, observed);
  frame({ id: 43, type: 'ToolUseStart', tool_name: 'wrong tool' }, observed);
  frame({ id: 43, type: 'ResponseComplete', stop_reason: 'end_turn', usage: { input_tokens: 25, output_tokens: 12 } }, observed);
  frame({ id: 43, type: 'ResponseComplete', stop_reason: 'tool_use', usage: { input_tokens: 25, output_tokens: 99 } }, observed);
  frame({ id: true, type: 'TextDelta', text: spec.text }, observed);
  assert.deepEqual(observed.events.get(43), { text: false, tool: false, complete: false });
  assert.equal(observed.events.size, 1);
});

test('event output is bounded by frames, payload bytes, and request identities', () => {
  const tooManyFrames = state();
  tooManyFrames.frameCount = 4096;
  frame({ id: 43, type: 'TextDelta', text: spec.text }, tooManyFrames);
  assert.equal(tooManyFrames.networkFailure, true);
  const tooLarge = state();
  observeFrame(tooLarge, 'x'.repeat(4 * 1024 * 1024 + 1), spec);
  assert.equal(tooLarge.networkFailure, true);
  const tooManyIds = state();
  for (let id = 1; id <= 129; id++) frame({ id, type: 'TextDelta', text: spec.text }, tooManyIds);
  assert.equal(tooManyIds.networkFailure, true);
  assert.equal(tooManyIds.events.size, 128);
});

test('malformed JSON refuses success without retaining the payload', () => {
  const observed = state();
  observeFrame(observed, 'PRIVATE_INVALID_FRAME', spec);
  assert.equal(observed.networkFailure, true);
  assert.equal(observed.events.size, 0);
  assert.equal(JSON.stringify(observed).includes('PRIVATE_INVALID_FRAME'), false);
});


test('only the exact bounded same-origin SPA fallback diagnostic POST is allowed', () => {
  const base = 'http://127.0.0.1:11003';
  const request = (url, method = 'POST', body = '{"reason":"predecessor-not-loaded"}') => ({
    url: () => url, method: () => method, postData: () => body,
    headers: () => ({ "content-type": "application/json" })
  });
  assert.equal(allowedBrowserRequest(request(`${base}/api/requests`, 'GET'), base), true);
  const wrongType = request(`${base}/api/diagnostics/spa_delta_fallback`);
  wrongType.headers = () => ({ 'content-type': 'text/plain' });
  assert.equal(allowedBrowserRequest(wrongType, base), false);
  for (const reason of ['predecessor-not-loaded', 'compaction', 'lineage-broken', 'chrome-strip-failed', 'parse-error']) {
    assert.equal(allowedBrowserRequest(request(`${base}/api/diagnostics/spa_delta_fallback`, 'POST', JSON.stringify({ reason })), base), true);
  }
  for (const [url, method, body] of [
    [`${base}/api/requests`, 'POST', '{}'],
    ['http://127.0.0.1:11004/api/diagnostics/spa_delta_fallback', 'POST', '{"reason":"parse-error"}'],
    [`${base}/api/diagnostics/spa_delta_fallback?secret=private`, 'POST', '{"reason":"parse-error"}'],
    [`${base}/api/diagnostics/spa_delta_fallback`, 'PUT', '{"reason":"parse-error"}'],
    [`${base}/api/diagnostics/spa_delta_fallback`, 'POST', '{"reason":"unknown"}'],
    [`${base}/api/diagnostics/spa_delta_fallback`, 'POST', '{"reason":"parse-error","private":"secret"}'],
    [`${base}/api/diagnostics/spa_delta_fallback`, 'POST', 'x'.repeat(257)],
    [`${base}/api/diagnostics/spa_delta_fallback`, 'POST', 'null'],
    [`${base}/api/diagnostics/spa_delta_fallback`, 'POST', '[]'],
    [`${base}/@vite/client`, 'GET', '']
  ]) assert.equal(allowedBrowserRequest(request(url, method, body), base), false);
});
