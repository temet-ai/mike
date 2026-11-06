import { test } from "node:test";
import assert from "node:assert/strict";
import { titleSlug } from "../src/title.ts";

test("titleSlug puts the id before the slug", () => {
  assert.equal(titleSlug("Fleet Reference Run", 7), "7-fleet-reference-run");
});
