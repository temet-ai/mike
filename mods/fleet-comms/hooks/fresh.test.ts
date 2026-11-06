import { expect, test } from "claude-code/testing";
import type { HttpInit, HttpResponse } from "claude-code";

import { fetchFresh, forgetAnswers } from "./fresh";

const URL = "http://localhost:9989/api/threads?participant=s1";

type Call = { url: string; init?: HttpInit };

function server(answers: HttpResponse[]) {
  const calls: Call[] = [];
  return {
    calls,
    fetch: async (url: string, init?: HttpInit): Promise<HttpResponse> => {
      calls.push({ url, init });
      const next = answers.shift();
      if (next === undefined) throw new Error(`no answer left for ${url}`);
      return next;
    },
  };
}

const fresh = (text: string, etag: string): HttpResponse => ({
  status: 200,
  ok: true,
  headers: { etag },
  text,
});
const unchanged: HttpResponse = { status: 304, ok: false, headers: {}, text: "" };

test("a 304 answers with the text already held, after sending its etag", async () => {
  forgetAnswers();
  const http = server([fresh('{"threads":[1]}', '"e1"'), unchanged]);
  const first = await fetchFresh(http.fetch, URL);
  const second = await fetchFresh(http.fetch, URL);
  expect(http.calls[0].init).toBe(undefined);
  expect(http.calls[1].init?.headers?.["if-none-match"]).toBe('"e1"');
  expect(second.ok).toBe(true);
  expect(second.status).toBe(200);
  expect(second.text).toBe(first.text);
});

test("a changed answer replaces the text and the etag sent next", async () => {
  forgetAnswers();
  const http = server([
    fresh('{"threads":[1]}', '"e1"'),
    fresh('{"threads":[1,2]}', '"e2"'),
    unchanged,
  ]);
  await fetchFresh(http.fetch, URL);
  const changed = await fetchFresh(http.fetch, URL);
  const held = await fetchFresh(http.fetch, URL);
  expect(changed.text).toBe('{"threads":[1,2]}');
  expect(http.calls[2].init?.headers?.["if-none-match"]).toBe('"e2"');
  expect(held.text).toBe('{"threads":[1,2]}');
});

test("an answer without an etag, or a failure, is not held", async () => {
  forgetAnswers();
  const failed: HttpResponse = { status: 500, ok: false, headers: {}, text: "" };
  const http = server([
    fresh('{"threads":[1]}', '"e1"'),
    failed,
    { status: 200, ok: true, headers: {}, text: '{"threads":[]}' },
    { status: 200, ok: true, headers: {}, text: '{"threads":[]}' },
  ]);
  await fetchFresh(http.fetch, URL);
  const broken = await fetchFresh(http.fetch, URL);
  await fetchFresh(http.fetch, URL);
  await fetchFresh(http.fetch, URL);
  expect(broken.ok).toBe(false);
  expect(http.calls[2].init).toBe(undefined);
  expect(http.calls[3].init).toBe(undefined);
});
