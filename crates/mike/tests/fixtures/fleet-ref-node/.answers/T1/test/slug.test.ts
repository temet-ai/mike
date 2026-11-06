import { test } from "node:test";
import assert from "node:assert/strict";
import { slugify } from "../src/slug.ts";

test("slugify lowercases and joins words with dashes", () => {
  assert.equal(slugify("Hello, World!"), "hello-world");
  assert.equal(slugify("  --a  b--  "), "a-b");
});
