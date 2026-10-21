import { describe, expect, test } from "bun:test";
import {
  mergeOpeningParties,
  openingPartyPreview,
  openingPartyInput,
  validOpeningDate,
  type OpeningParty,
} from "./opening-parties";
import { openingDraft, openingPreview, type OpeningDocument } from "./opening";
const row = (kind: OpeningParty["kind"], amount = "100.00"): OpeningParty => ({
  kind,
  amount,
  party_id: "party",
  reference: kind,
  document_date: "2025-01-01",
  due_date: null,
});
describe("opening party subledger", () => {
  test("a returned draft and CSV preview can be saved without read-only metadata", () => {
    const saved = {
      ...row("receivable"),
      reference: " OLD-001 ",
      party_name: "Pelanggan lama",
      id: "display-only",
    };
    expect(openingPartyInput(saved)).toEqual({
      kind: "receivable",
      party_id: "party",
      reference: "OLD-001",
      document_date: "2025-01-01",
      due_date: null,
      amount: "100.00",
    });
    expect(saved.party_name).toBe("Pelanggan lama");
  });
  test("generated party control lines are not loaded as manual balances or counted twice", () => {
    const saved = {
      description: "Pembukaan",
      items: [],
      parties: [row("receivable", "2000"), row("payable", "3000")],
      lines: [
        { account_code: "1200", debit: "2000", credit: "0", generated: true },
        { account_code: "2200", debit: "0", credit: "3000", generated: true },
        { account_code: "1100", debit: "1000", credit: "0", generated: false },
      ],
    } as unknown as OpeningDocument;
    const draft = openingDraft(saved);
    expect(draft.lines).toHaveLength(1);
    const preview = openingPreview(draft);
    expect(preview.balanced).toBe(true);
    expect(preview.debit).toBe("3000.00");
    expect(preview.credit).toBe("3000.00");
  });
  test("keeps debt and customer/supplier credit on the correct control side", () => {
    const result = openingPartyPreview(
      [
        row("receivable", "200.01"),
        row("payable", "300.01"),
        row("customer_credit", "20.02"),
        row("supplier_credit", "30.02"),
      ],
      "2026-10-20",
    );
    expect(result.errors).toEqual([]);
    expect(result.debit).toBe(23003n);
    expect(result.credit).toBe(32003n);
  });
  test("CSV merges are atomic and reject duplicates against unsaved manual rows", () => {
    const existing = [row("receivable")];
    expect(() =>
      mergeOpeningParties(existing, {
        parties: [row("receivable")],
        errors: [],
        valid_count: 1,
      }),
    ).toThrow("referensi");
    expect(() =>
      mergeOpeningParties(existing, {
        parties: [row("payable")],
        errors: [{ row: 2, column: "amount", message: "invalid" }],
        valid_count: 1,
      }),
    ).toThrow("CSV");
    expect(existing).toHaveLength(1);
    expect(
      mergeOpeningParties(existing, {
        parties: [row("payable")],
        errors: [],
        valid_count: 1,
      }),
    ).toHaveLength(2);
  });
  test("validates real historical dates, unknown due dates, and strict monetary scale", () => {
    expect(validOpeningDate("2024-02-29")).toBe(true);
    expect(validOpeningDate("2025-02-29")).toBe(false);
    expect(
      openingPartyPreview(
        [{ ...row("receivable"), due_date: "2024-12-31" }],
        "2026-10-20",
      ).errors.length,
    ).toBeGreaterThan(0);
    expect(
      openingPartyPreview(
        [{ ...row("receivable"), document_date: "2026-10-21" }],
        "2026-10-20",
      ).errors.length,
    ).toBeGreaterThan(0);
    expect(
      openingPartyPreview([row("receivable", "0.001")], "2026-10-20").errors
        .length,
    ).toBeGreaterThan(0);
    expect(
      openingPartyPreview([row("receivable", "0")], "2026-10-20").errors.length,
    ).toBeGreaterThan(0);
  });
});
