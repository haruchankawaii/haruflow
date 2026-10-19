import { expect, test } from "bun:test";
import {
  previewReturn,
  quantityMillis,
  returnEndpoint,
  returnReversalEndpoint,
  type ReturnableItem,
} from "./returns-model";
const line = (overrides: Partial<ReturnableItem> = {}): ReturnableItem => ({
  id: "line",
  name: "Produk",
  quantity: "3.000",
  total: "1.00",
  returned_quantity: "0.000",
  remaining_return_quantity: "3.000",
  returned_credit: "0.00",
  ...overrides,
});
test("return credits allocate cumulative cents and preserve final cent across partial returns", () => {
  const first = previewReturn([line()], { line: "1" }, "1.00", "0.00", "0.00");
  expect(first.error).toBe("");
  expect(first.credit).toBe(33n);
  const second = previewReturn(
    [
      line({
        returned_quantity: "1",
        remaining_return_quantity: "2",
        returned_credit: "0.33",
      }),
    ],
    { line: "1" },
    "1.00",
    "0.00",
    "0.33",
  );
  expect(second.credit).toBe(34n);
  const third = previewReturn(
    [
      line({
        returned_quantity: "2",
        remaining_return_quantity: "1",
        returned_credit: "0.67",
      }),
    ],
    { line: "1" },
    "1.00",
    "0.00",
    "0.67",
  );
  expect(third.credit).toBe(33n);
  expect(third.netAfter).toBe(0n);
  expect(first.credit + second.credit + third.credit).toBe(100n);
});
test("return previews use historical purchase value and reserve final cent until final quantity", () => {
  const purchase = line({ total: undefined, value: "0.02" });
  expect(
    previewReturn([purchase], { line: "2" }, "0.02", "0", "0").credit,
  ).toBe(1n);
  expect(
    previewReturn([line({ total: "0.01" })], { line: "2" }, "0.01", "0", "0")
      .error,
  ).not.toBe("");
  expect(
    previewReturn([line({ total: "0.01" })], { line: "3" }, "0.01", "0", "0")
      .credit,
  ).toBe(1n);
});
test("refund and outstanding previews distinguish unpaid, partly paid, and fully paid documents", () => {
  const item = line({
    quantity: "1",
    remaining_return_quantity: "1",
    total: "80.00",
  });
  const unpaid = previewReturn([item], { line: "1" }, "180.00", "0.00", "0.00");
  expect(unpaid.refund).toBe(0n);
  expect(unpaid.outstandingAfter).toBe(10000n);
  const partial = previewReturn(
    [item],
    { line: "1" },
    "180.00",
    "120.00",
    "0.00",
  );
  expect(partial.refund).toBe(2000n);
  expect(partial.paidAfter).toBe(10000n);
  expect(partial.outstandingAfter).toBe(0n);
  const full = previewReturn([item], { line: "1" }, "180.00", "180.00", "0.00");
  expect(full.refund).toBe(8000n);
  expect(full.netAfter).toBe(10000n);
});
test("return quantity bounds and large exact amounts reject malformed and stale selections", () => {
  for (const qty of ["-1", "1e2", "0.0001", "1,5", "1000000001"])
    expect(() => quantityMillis(qty)).toThrow();
  expect(
    previewReturn([line()], { line: "3.001" }, "1.00", "0", "0").error,
  ).not.toBe("");
  expect(previewReturn([line()], {}, "1.00", "0", "0").error).not.toBe("");
  const huge = previewReturn(
    [
      line({
        quantity: "1",
        remaining_return_quantity: "1",
        total: "9007199254740993.01",
      }),
    ],
    { line: "1" },
    "9007199254740993.01",
    "9007199254740993.01",
    "0",
  );
  expect(huge.refund).toBe(900719925474099301n);
  expect(huge.netAfter).toBe(0n);
});
test("return actions use the document-specific forward and reversal routes", () => {
  expect(returnEndpoint("sales", "invoice")).toBe("/invoices/invoice/returns");
  expect(returnEndpoint("supplier", "bill")).toBe(
    "/purchase-bills/bill/returns",
  );
  expect(returnReversalEndpoint("sales", "return")).toBe(
    "/sales-returns/return/reverse",
  );
  expect(returnReversalEndpoint("supplier", "return")).toBe(
    "/supplier-returns/return/reverse",
  );
});
