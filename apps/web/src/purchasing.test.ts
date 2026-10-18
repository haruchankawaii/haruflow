import { describe, expect, test } from "bun:test";
import {
  exactCost,
  exactDecimal,
  purchaseDraft,
  purchaseLineCents,
  purchasePreview,
  purchaseProducts,
  purchaseSummary,
  requestIdentity,
  type PurchaseOrder,
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
      { status: "cancelled", total: "100.00" },
    ] as PurchaseOrder[]);
    expect(summary.map(({ count }) => count)).toEqual([1, 2, 1]);
    expect(summary[1].total).toBe(100000000000000n);
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
});
