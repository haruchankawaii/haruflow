import { describe, expect, test } from "bun:test";
import {
  exactCost,
  exactDecimal,
  purchaseDraft,
  purchaseLineCents,
  purchasePreview,
  purchaseProducts,
  purchaseSummary,
  purchaseStatusRows,
  requestIdentity,
  receiptPreview,
  type PurchaseOrder,
  type PurchasingSummary,
} from "./purchasing-model";
import type { Product } from "./types";

const draft = (quantity = "1", unit_cost = "1") => ({
  supplier_id: "supplier-1",
  expected_date: "",
  notes: "",
  items: [{ product_id: "stock-1", quantity, unit_cost }],
});

describe("purchase previews", () => {
  test("rounds every line before summing and preserves subcent unit costs", () => {
    expect(purchaseLineCents("1", "0.005")).toBe(1n);
    expect(purchaseLineCents("0.125", "0.12")).toBe(2n);
    expect(purchaseLineCents("1.2500", "0.1200")).toBe(15n);
    const document = draft("1", "0.005");
    document.items.push({
      product_id: "stock-2",
      quantity: "1",
      unit_cost: "0.005",
    });
    expect(purchasePreview(document)).toMatchObject({ total: 2n, valid: true });
  });
  test("enforces precision, positive rounded amounts, duplicates and document bounds", () => {
    for (const [quantity, cost] of [
      ["0", "1"],
      ["1.0001", "1"],
      ["1", "1.00001"],
      ["1e3", "1"],
      ["1", "-1"],
      ["0.001", "0.0001"],
      ["1000000001", "1"],
    ])
      expect(purchasePreview(draft(quantity, cost)).valid).toBe(false);
    expect(purchasePreview(draft("1000", "1000000000")).valid).toBe(true);
    expect(purchasePreview(draft("1001", "1000000000")).valid).toBe(false);
    const duplicate = draft();
    duplicate.items.push({ ...duplicate.items[0] });
    expect(purchasePreview(duplicate).issue).toContain("Gabungkan");
    expect(purchasePreview({ ...draft(), items: [] }).valid).toBe(false);
    expect(
      purchasePreview({
        ...draft(),
        items: Array.from({ length: 101 }, (_, index) => ({
          ...draft().items[0],
          product_id: `${index}`,
        })),
      }).valid,
    ).toBe(false);
  });
  test("formats large values without floating point and keeps stored costs", () => {
    expect(exactCost("999999999.1234")).toBe("Rp999.999.999,1234");
    expect(exactDecimal("12345678901234567890.1200")).toBe(
      "12.345.678.901.234.567.890,12",
    );
    const stored = {
      supplier_id: "supplier-1",
      expected_date: "2026-11-02",
      notes: "Stored",
      items: [
        { product_id: "stock-1", quantity: "1.125", unit_cost: "2500.1234" },
      ],
    } as PurchaseOrder;
    expect(purchaseDraft(stored).items[0].unit_cost).toBe("2500.1234");
    expect(purchaseDraft().items[0].unit_cost).toBe("");
    expect(
      purchaseProducts([
        { id: "stock", kind: "stock" },
        { id: "service", kind: "service" },
      ] as Product[]).map((product) => product.id),
    ).toEqual(["stock"]);
  });
  test("commitment uses confirmed order totals and keeps cancelled amounts separate", () => {
    const summary = purchaseSummary([
      { status: "draft", total: "0.01" },
      { status: "confirmed", total: "999999999999.99" },
      { status: "confirmed", total: "0.01" },
      { status: "received", total: "250000.12" },
      { status: "cancelled", total: "100.00" },
    ] as PurchaseOrder[]);
    expect(summary.map(({ count }) => count)).toEqual([1, 2, 0, 1, 1]);
    expect(summary[1].total).toBe(100000000000000n);
    expect(summary[3].total).toBe(25000012n);
  });
  test("retries reuse request keys until the submitted payload changes", () => {
    let generated = 0;
    const make = () => `key-${++generated}`;
    const first = requestIdentity(null, { version: 1, notes: "A" }, make);
    expect(requestIdentity(first, { version: 1, notes: "A" }, make)).toBe(
      first,
    );
    const changed = requestIdentity(first, { version: 1, notes: "B" }, make);
    expect(changed.key).toBe("key-2");
    expect(requestIdentity(changed, { version: 2, notes: "B" }, make).key).toBe(
      "key-3",
    );
  });
  test("chart keeps complete server totals and received orders distinct from pending PO", () => {
    const summary: PurchasingSummary = {
      counts: {
        draft: 1,
        confirmed: 250,
        partial_received: 0,
        received: 500,
        cancelled: 7,
      },
      values: {
        draft: "0.01",
        confirmed: "999999999999999999.12",
        partial_received: "0.00",
        received: "125000.34",
        cancelled: "4000.00",
      },
      pending_receipt_total: "999999999999999999.12",
      received_unbilled_total: "125000.34",
      supplier_payable_total: "0.00",
      as_of: "2026-10-18T17:30:00Z",
    };
    const rows = purchaseStatusRows(summary);
    expect(rows.reduce((total, item) => total + item.count, 0)).toBe(758);
    expect(rows.find((item) => item.status === "confirmed")).toMatchObject({
      count: 250,
      value: 99999999999999999912n,
      width: 50,
    });
    expect(rows.find((item) => item.status === "received")).toMatchObject({
      count: 500,
      value: 12500034n,
      width: 100,
    });
    expect(rows[0].value).toBe(1n);
  });
  test("empty purchase charts retain every status with finite zero bars", () => {
    const rows = purchaseStatusRows({
      counts: {
        draft: 0,
        confirmed: 0,
        partial_received: 0,
        received: 0,
        cancelled: 0,
      },
      values: {
        draft: "0.00",
        confirmed: "0.00",
        partial_received: "0.00",
        received: "0.00",
        cancelled: "0.00",
      },
      pending_receipt_total: "0.00",
      received_unbilled_total: "0.00",
      supplier_payable_total: "0.00",
      as_of: "2026-10-18T17:30:00Z",
    });
    expect(rows.map((item) => item.status)).toEqual([
      "draft",
      "confirmed",
      "partial_received",
      "received",
      "cancelled",
    ]);
    expect(
      rows.every(
        (item) => item.width === 0 && item.count === 0 && item.value === 0n,
      ),
    ).toBe(true);
  });
  test("receipt retries retain identity while another receipt or current PO version gets a fresh key", () => {
    let generated = 0;
    const make = () => `receipt-key-${++generated}`;
    const payload = {
      action: "reverse",
      order_id: "po-1",
      receipt_id: "gr-1",
      version: 3,
      reason: "Jumlah salah",
    };
    const first = requestIdentity(null, payload, make);
    expect(requestIdentity(first, { ...payload }, make)).toBe(first);
    expect(
      requestIdentity(first, { ...payload, receipt_id: "gr-2" }, make).key,
    ).toBe("receipt-key-2");
    expect(requestIdentity(first, { ...payload, version: 4 }, make).key).toBe(
      "receipt-key-3",
    );
  });
});

test("partial receipt preview allocates cumulative rounding and caps remaining quantities", () => {
  const order = {
    items: [
      {
        id: "line-1",
        quantity: "3",
        unit_cost: "3.3333",
        total: "10.00",
        received_quantity: "1",
        remaining_quantity: "2",
        received_value: "3.33",
      },
    ],
  } as PurchaseOrder;
  expect(receiptPreview(order, { "line-1": "1" })).toMatchObject({
    total: 334n,
    valid: true,
    items: [{ order_line_id: "line-1", quantity: "1" }],
  });
  expect(receiptPreview(order, { "line-1": "2" })).toMatchObject({
    total: 667n,
    valid: true,
  });
  for (const quantity of ["0", "2.001", "-1", "1.0001", ""])
    expect(receiptPreview(order, { "line-1": quantity }).valid).toBe(false);
  const finalOrder = {
    items: [
      {
        ...order.items[0],
        received_quantity: "2",
        remaining_quantity: "1",
        received_value: "6.67",
      },
    ],
  } as PurchaseOrder;
  expect(receiptPreview(finalOrder, { "line-1": "1" }).total).toBe(333n);
});

test("tiny partial receipt cannot consume the final cent before all goods arrive", () => {
  const order = {
    items: [
      {
        id: "tiny",
        quantity: "1",
        unit_cost: "0.01",
        total: "0.01",
        received_quantity: "0",
        remaining_quantity: "1",
        received_value: "0.00",
      },
    ],
  } as PurchaseOrder;
  expect(receiptPreview(order, { tiny: "0.999" }).valid).toBe(false);
  expect(receiptPreview(order, { tiny: "1" })).toMatchObject({
    total: 1n,
    valid: true,
  });
});
