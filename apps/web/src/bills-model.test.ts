import { describe, expect, test } from "bun:test";
import {
  billChartRows,
  billDraftIssue,
  eligibleBillReceipts,
  validCalendarDate,
  type BillDraft,
  type SupplierBillsSummary,
} from "./bills-model";
import { exactMoney } from "./dashboard-charts";
import type { PurchaseOrder, PurchaseReceipt } from "./purchasing-model";

const draft: BillDraft = {
  supplier_invoice_number: "INV-1",
  invoice_date: "2026-10-19",
  due_date: "2026-11-19",
  notes: "",
};
const summary: SupplierBillsSummary = {
  today: "2026-10-19",
  timezone: "Asia/Jakarta",
  as_of: "2026-10-18T17:30:00Z",
  counts: {
    unpaid: 700,
    void: 10,
    upcoming: 400,
    due_today: 100,
    overdue: 200,
  },
  outstanding_total: "2000000000000000000.01",
  overdue_total: "1000000000000000000.00",
  due_today_total: "0.01",
  upcoming_total: "1000000000000000000.00",
  suppliers: [],
};

describe("supplier bills", () => {
  test("accepts actual calendar dates across leap years and the supported year range", () => {
    for (const date of [
      "0001-01-01",
      "0099-12-31",
      "2000-02-29",
      "2024-02-29",
      "9999-12-31",
    ])
      expect(validCalendarDate(date)).toBe(true);
    for (const date of [
      "0000-01-01",
      "10000-01-01",
      "1900-02-29",
      "2100-02-29",
      "2026-02-29",
      "2026-04-31",
      "2026-13-01",
      "2026-00-01",
      "2026-10-00",
      "2026-1-1",
      "2026-10-19T00:00:00Z",
      "",
    ])
      expect(validCalendarDate(date)).toBe(false);
  });
  test("uses company today for future invoice validation and permits old invoices with ordered due dates", () => {
    expect(billDraftIssue(draft, "2026-10-19")).toBe("");
    expect(billDraftIssue(draft, "2026-10-18")).toContain(
      "zona waktu perusahaan",
    );
    expect(
      billDraftIssue(
        { ...draft, invoice_date: "2000-02-29", due_date: "2000-02-29" },
        "2026-10-19",
      ),
    ).toBe("");
    expect(
      billDraftIssue({ ...draft, due_date: "2026-10-18" }, "2026-10-19"),
    ).toContain("sebelum tanggal invoice");
    expect(
      billDraftIssue({ ...draft, invoice_date: "2026-02-30" }, "2026-10-19"),
    ).toContain("valid");
    expect(
      billDraftIssue(
        { ...draft, supplier_invoice_number: "   " },
        "2026-10-19",
      ),
    ).toContain("wajib");
    expect(
      billDraftIssue(
        { ...draft, supplier_invoice_number: "X".repeat(101) },
        "2026-10-19",
      ),
    ).toContain("100");
    expect(
      billDraftIssue({ ...draft, notes: "X".repeat(2001) }, "2026-10-19"),
    ).toContain("2.000");
  });
  test("chart preserves complete server money and count buckets beyond JavaScript safe integers", () => {
    const rows = billChartRows(summary);
    expect(rows.map((row) => row.status)).toEqual([
      "upcoming",
      "due_today",
      "overdue",
    ]);
    expect(rows.map((row) => row.count)).toEqual([400, 100, 200]);
    expect(rows[0].value).toBe(100000000000000000000n);
    expect(rows[1].value).toBe(1n);
    expect(rows.reduce((sum, row) => sum + row.value, 0n)).toBe(
      200000000000000000001n,
    );
    expect(rows[0].width).toBe(100);
    expect(rows[2].width).toBe(100);
    expect(rows.every((row) => Number.isFinite(row.width))).toBe(true);
    expect(exactMoney(rows[1].value)).toBe("Rp0,01");
    expect(exactMoney(rows[0].value)).toBe("Rp1.000.000.000.000.000.000");
  });
  test("empty chart retains every due category with finite zero bars", () => {
    const rows = billChartRows({
      ...summary,
      upcoming_total: "0.00",
      due_today_total: "0.00",
      overdue_total: "0.00",
      counts: { unpaid: 0, void: 2, upcoming: 0, due_today: 0, overdue: 0 },
    });
    expect(rows).toHaveLength(3);
    expect(
      rows.every(
        (row) => row.width === 0 && row.value === 0n && row.count === 0,
      ),
    ).toBe(true);
  });
  test("picker accepts only current active server-eligible receipts, retaining the latest PO version", () => {
    const receipt = (id: string, props: Partial<PurchaseReceipt> = {}) =>
      ({ id, reversal: null, can_bill: true, ...props }) as PurchaseReceipt;
    const orders = [
      {
        id: "po-1",
        version: 9,
        active_receipt_id: "gr-current",
        receipts: [receipt("gr-old"), receipt("gr-current")],
      },
      {
        id: "po-2",
        version: 4,
        active_receipt_id: "gr-billed",
        receipts: [
          receipt("gr-billed", { can_bill: false, bill_id: "bill-1" }),
        ],
      },
      {
        id: "po-3",
        version: 6,
        active_receipt_id: "gr-reversed",
        receipts: [
          receipt("gr-reversed", {
            reversal: {
              id: "r-1",
              reason: "wrong",
              created_at: "2026-10-19",
              journal_id: "j-1",
            },
          }),
        ],
      },
      {
        id: "po-4",
        version: 3,
        active_receipt_id: null,
        receipts: [receipt("gr-not-current")],
      },
    ] as PurchaseOrder[];
    const eligible = eligibleBillReceipts(orders);
    expect(eligible.map((source) => source.receipt.id)).toEqual(["gr-current"]);
    expect(eligible[0].order.version).toBe(9);
    expect(eligibleBillReceipts([])).toEqual([]);
  });
});
