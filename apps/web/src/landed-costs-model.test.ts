import { expect, test } from "bun:test";
import { landedCostInput, retainedPercent, type LandedCostDraft, type LandedCostOptions } from "./landed-costs-model";
const options: LandedCostOptions = { sources: [{ line_id: "cost", journal_id: "journal", source_type: "purchase_bill", source_id: "bill", number: "PB-1", account_code: "6000", account_name: "Ongkir", amount: "1000000000000.00", available: "1000000000000.00", eligible: true }], targets: ["a", "b"].map((id) => ({ receipt_line_id: id, receipt_id: `receipt-${id}`, receipt_number: `PR-${id}`, product_id: `product-${id}`, product_sku: id, product_name: id, unit: "kg", quantity: "1", receipt_value: "10", eligible: true, blocked_reason: null })) };
const draft = (patch: Partial<LandedCostDraft> = {}): LandedCostDraft => ({ source_line_id: "cost", amount: "1000000000000.00", allocation_method: "value", receipt_line_ids: ["b", "a"], reference: " proof ", reason: " Based on freight proof ", ...patch });
test("cost request keeps trillion-scale cents exact and normalizes proof without creating a financial split", () => {
  expect(landedCostInput(draft(), options)).toEqual({ source_line_id: "cost", amount: "1000000000000.00", allocation_method: "value", receipt_line_ids: ["a", "b"], reference: "proof", reason: "Based on freight proof" });
  for (const amount of ["0", "-1", "1.001", "NaN", "1000000000000.01"]) expect(() => landedCostInput(draft({ amount }), options)).toThrow();
  expect(landedCostInput(draft({ reference: "  " }), options)).not.toHaveProperty("reference");
});
test("inactive source, duplicate product, mixed units and unavailable receipt fail before preview", () => {
  expect(() => landedCostInput(draft(), { ...options, sources: [{ ...options.sources[0], eligible: false, blocked_reason: "Sumber sudah dipakai" }] })).toThrow("Sumber sudah dipakai");
  expect(() => landedCostInput(draft(), { ...options, targets: options.targets.map((t) => ({ ...t, product_id: "same" })) })).toThrow("satu penerimaan");
  expect(() => landedCostInput(draft({ allocation_method: "quantity" }), { ...options, targets: [options.targets[0], { ...options.targets[1], unit: "pcs" }] })).toThrow("satuan dasar");
  expect(() => landedCostInput(draft(), { ...options, targets: [options.targets[0], { ...options.targets[1], eligible: false, blocked_reason: "Riwayat retur" }] })).toThrow("Riwayat retur");
  expect(() => landedCostInput(draft({ receipt_line_ids: ["a", "a"] }), options)).toThrow();
});
test("retained fractions display percentages without floating-point money conversions", () => {
  expect(retainedPercent("0.6")).toBe("60%");expect(retainedPercent("0.3333333333333333333333333333")).toBe("33,33%");expect(retainedPercent("0")).toBe("0%");expect(retainedPercent("1")).toBe("100%");
});
