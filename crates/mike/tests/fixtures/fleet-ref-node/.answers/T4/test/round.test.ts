import { test } from "node:test";
import assert from "node:assert/strict";
import { roundTo } from "../src/round.ts";

test("roundTo rounds halves away from zero", () => {
  assert.equal(roundTo(2.5, 0), 3);
  assert.equal(roundTo(-2.5, 0), -3);
  assert.equal(roundTo(1.25, 1), 1.3);
});

test("roundTo refuses digits out of range", () => {
  assert.throws(() => roundTo(1, 11), RangeError);
  assert.throws(() => roundTo(1, 1.5), RangeError);
});
