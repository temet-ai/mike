import type { HttpInit, HttpResponse } from "claude-code";

// mike answers its polled reads with an ETag drawn from its change feed. Sending
// it back lets mike answer 304 without reading the database; a 304 means the
// answer we already hold is still current.

type Fetch = (url: string, init?: HttpInit) => Promise<HttpResponse>;

const KEPT = 64;
const kept = new Map<string, { etag: string; text: string }>();

export async function fetchFresh(fetch: Fetch, url: string): Promise<HttpResponse> {
  const known = kept.get(url);
  const res = await fetch(
    url,
    known === undefined ? undefined : { headers: { "if-none-match": known.etag } },
  );
  if (res.status === 304 && known !== undefined) {
    kept.delete(url);
    kept.set(url, known);
    return { ...res, status: 200, ok: true, text: known.text };
  }
  kept.delete(url);
  const etag = res.headers["etag"];
  if (res.ok && typeof etag === "string" && etag !== "") {
    kept.set(url, { etag, text: res.text });
    if (kept.size > KEPT) {
      const oldest = kept.keys().next().value;
      if (oldest !== undefined) kept.delete(oldest);
    }
  }
  return res;
}

export function forgetAnswers(): void {
  kept.clear();
}
