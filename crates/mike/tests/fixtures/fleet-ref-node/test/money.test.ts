import { test } from "node:test";
import assert from "node:assert/strict";
import { formatCents } from "../src/money.ts";

test("formats whole and fractional amounts", () => {
  assert.equal(formatCents(1999), "19.99");
  assert.equal(formatCents(5), "0.05");
  assert.equal(formatCents(-250), "-2.50");
});
