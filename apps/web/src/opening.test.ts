import { describe, expect, test } from "bun:test";
import { openingAccounts, openingDraft, openingPreview } from "./opening";
import type { OpeningDocument, OpeningDraft } from "./opening";

const draft = (overrides: Partial<OpeningDraft> = {}): OpeningDraft => ({
  description: "Saldo awal usaha",
  items: [],
  lines: [],
  ...overrides,
});
describe("opening balance preparation", () => {
  test("inventory is rounded per item and added to debit exactly once", () => {
    const preview = openingPreview(
      draft({
        items: [
          { product_id: "stock", quantity: "0.333", unit_cost: "1.0000" },
        ],
        lines: [
          { account_code: "1100", debit: "0.10", credit: "0" },
          { account_code: "3000", debit: "0", credit: "0.43" },
        ],
      }),
    );
    expect(preview).toEqual({
      inventory: "0.33",
      debit: "0.43",
      credit: "0.43",
      difference: "0.00",
      balanced: true,
      valid: true,
    });
  });
  test("empty and unbalanced drafts can be prepared but are not postable", () => {
    expect(openingPreview(draft()).balanced).toBe(false);
    const unbalanced = openingPreview(
      draft({ lines: [{ account_code: "1100", debit: "100", credit: "" }] }),
    );
    expect(unbalanced.valid).toBe(true);
    expect(unbalanced.balanced).toBe(false);
    expect(unbalanced.difference).toBe("100.00");
  });
  test("insignificant trailing zeros match server decimal validation", () => {
    const preview = openingPreview(
      draft({
        items: [
          {
            product_id: "stock",
            quantity: "1.000000",
            unit_cost: "100.000000",
          },
        ],
        lines: [
          { account_code: "3000", debit: "0.000000", credit: "100.000000" },
        ],
      }),
    );
    expect(preview.valid).toBe(true);
    expect(preview.balanced).toBe(true);
    expect(preview.inventory).toBe("100.00");
  });
  test("invalid scales, zero-value inventory and double-sided lines do not appear postable", () => {
    for (const item of [
      { product_id: "a", quantity: "0.0001", unit_cost: "10000" },
      { product_id: "a", quantity: "1", unit_cost: "1.00001" },
      { product_id: "a", quantity: "0.001", unit_cost: "0.0001" },
      { product_id: "a", quantity: "1e3", unit_cost: "1" },
    ])
      expect(openingPreview(draft({ items: [item] })).valid).toBe(false);
    expect(
      openingPreview(
        draft({ lines: [{ account_code: "1100", debit: "1", credit: "1" }] }),
      ).valid,
    ).toBe(false);
    expect(
      openingPreview(
        draft({
          lines: [{ account_code: "1100", debit: "1.001", credit: "0" }],
        }),
      ).valid,
    ).toBe(false);
  });
  test("generated inventory lines never return to manual draft entries", () => {
    const document = {
      description: "Opening",
      items: [],
      lines: [
        {
          account_code: "1300",
          account_name: "Inventory",
          debit: "500",
          credit: "0",
        },
        {
          account_code: "3000",
          account_name: "Equity",
          debit: "0",
          credit: "500",
        },
      ],
    } as unknown as OpeningDocument;
    expect(openingDraft(document).lines).toEqual([
      { account_code: "3000", debit: "0", credit: "500" },
    ]);
  });
  test("only balance sheet accounts outside controlled ledgers are selectable", () => {
    const accounts = [
      { code: "1100", name: "Cash", kind: "asset" },
      { code: "1200", name: "Receivables", kind: "asset" },
      { code: "1300", name: "Inventory", kind: "asset" },
      { code: "2000", name: "Liability", kind: "liability" },
      { code: "3000", name: "Equity", kind: "equity" },
      { code: "4000", name: "Revenue", kind: "revenue" },
      { code: "5000", name: "Expense", kind: "expense" },
    ];
    expect(openingAccounts(accounts).map((a) => a.code)).toEqual([
      "1100",
      "2000",
      "3000",
    ]);
  });
  test("reserved purchasing control uses metadata rather than occupying a user's existing code", () => {
    const accounts = [
      { code: "2110", name: "Utang lain-lain lama", kind: "liability" },
      { code: "2200", name: "Utang khusus lama", kind: "liability" },
      {
        code: "2111",
        name: "Barang diterima belum ditagih",
        kind: "liability",
        system_role: "purchase_receipt_accrual",
        is_control: true,
      },
      {
        code: "2201",
        name: "Utang supplier",
        kind: "liability",
        system_role: "purchase_payable",
        is_control: true,
      },
    ];
    expect(openingAccounts(accounts).map((account) => account.code)).toEqual([
      "2110",
      "2200",
    ]);
    const controlDraft = draft({
      lines: [
        { account_code: "1100", debit: "1", credit: "0" },
        { account_code: "2111", debit: "0", credit: "1" },
      ],
    });
    expect(openingPreview(controlDraft, accounts).valid).toBe(false);
    controlDraft.lines[1].account_code = "2201";
    expect(openingPreview(controlDraft, accounts).valid).toBe(false);
    controlDraft.lines[1].account_code = "2110";
    expect(openingPreview(controlDraft, accounts).valid).toBe(true);
  });
});
