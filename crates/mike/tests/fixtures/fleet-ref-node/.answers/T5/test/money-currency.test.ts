import { test } from "node:test";
import assert from "node:assert/strict";
import { formatMoney } from "../src/money.ts";

test("formatMoney prefixes the currency", () => {
  assert.equal(formatMoney(1999, "EUR"), "EUR 19.99");
  assert.equal(formatMoney(-5, "USD"), "USD -0.05");
});
