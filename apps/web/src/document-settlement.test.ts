import { expect, test } from "bun:test";
import { documentSettlement } from "./document-settlement";
test("retained credit does not pretend that cash was refunded", () => {
  expect(
    documentSettlement(
      "100",
      [{ amount: "120" }],
      [{ refund: "0", settlement_credit: "20" }],
    ),
  ).toEqual({
    cash: 12000n,
    transferred: 2000n,
    allocated: 0n,
    settled: 10000n,
  });
  expect(
    documentSettlement(
      "100",
      [{ amount: "120" }],
      [{ refund: "20", settlement_credit: "0" }],
    ),
  ).toEqual({ cash: 10000n, transferred: 0n, allocated: 0n, settled: 10000n });
});
test("credit allocation is separate from both cash and reversed history", () => {
  expect(
    documentSettlement("600", [{ amount: "100", reversed: true }], []),
  ).toEqual({ cash: 0n, transferred: 0n, allocated: 60000n, settled: 60000n });
  expect(
    documentSettlement("100", [], [{ refund: "20", settlement_credit: "0" }]),
  ).toEqual({
    cash: -2000n,
    transferred: 0n,
    allocated: 12000n,
    settled: 10000n,
  });
  expect(
    documentSettlement(
      "120",
      [{ amount: "120" }],
      [{ refund: "20", settlement_credit: "0", reversal: { id: "reversed" } }],
    ).cash,
  ).toBe(12000n);
});
