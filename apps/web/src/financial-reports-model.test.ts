import { expect, test } from "bun:test";
import {
  reportBarWidth,
  reportRangeError,
  validReportDate,
  financialReportCsv,
  type FinancialReport,
} from "./financial-reports-model";

test("report periods validate calendar dates including centuries and year limits", () => {
  for (const d of ["0001-01-01", "2000-02-29", "2024-02-29", "9999-12-31"])
    expect(validReportDate(d)).toBe(true);
  for (const d of [
    "0000-01-01",
    "1900-02-29",
    "2025-02-29",
    "2026-04-31",
    "2026-1-01",
    "2026-01-01\0",
    "2026-13-01",
  ])
    expect(validReportDate(d)).toBe(false);
  expect(reportRangeError("2026-01-01", "2026-01-01")).toBe("");
  expect(reportRangeError("2026-02-01", "2026-01-01")).not.toBe("");
});
test("financial bar geometry handles very large exact amounts, negatives and zero", () => {
  expect(reportBarWidth("0.00", ["0.00"])).toBe(0);
  expect(
    reportBarWidth("-50000000000000000.01", ["100000000000000000.02"]),
  ).toBe(50);
  expect(reportBarWidth("0.01", ["0.02", "-0.04"])).toBe(25);
});
test("report CSV preserves report basis, exact amounts, reconciliation and classification audit", () => {
  const report: FinancialReport = {
    from: "2026-01-01",
    to: "2026-01-31",
    timezone: "Asia/Jakarta",
    as_of: "2026-02-01T00:00:00Z",
    company_name: "Toko",
    income_statement: {
      revenue_accounts: [
        {
          code: "4100",
          name: "Jasa",
          kind: "revenue",
          amount: "9007199254740993.01",
        },
      ],
      expense_accounts: [],
      revenue_total: "9007199254740993.01",
      expense_total: "0.00",
      net_profit: "9007199254740993.01",
    },
    balance_sheet: {
      asset_accounts: [],
      liability_accounts: [],
      equity_accounts: [],
      retained_earnings_accounts: [],
      assets_total: "9007199254740993.01",
      liabilities_total: "0.00",
      equity_total: "0.00",
      retained_earnings: "9007199254740993.01",
      liabilities_and_equity_total: "9007199254740993.01",
      difference: "0.00",
      balanced: true,
    },
    cash_flow: {
      account_code: "1100",
      beginning_balance: "0.00",
      ending_balance: "9.01",
      cash_in: "9.01",
      cash_out: "0.00",
      net_change: "9.01",
      difference: "0.00",
      reconciled: true,
      categories: [],
      entries: [
        {
          journal_id: "journal",
          description: "Setoran",
          created_at: "2026-01-01T00:00:00Z",
          source_type: "manual",
          source_id: "source",
          original_source_type: "manual",
          cash_in: "9.01",
          cash_out: "0.00",
          net_change: "9.01",
          category: "financing",
          classification_reason: "manual_override",
          counterparts: [],
          classification: {
            category: "financing",
            version: 1,
            reason: "Setoran pemilik",
            updated_at: "now",
            updated_by_name: "Admin",
            history: [
              {
                category: "financing",
                previous_category: null,
                version: 1,
                reason: "Setoran pemilik",
                created_at: "now",
                created_by_name: "Admin",
              },
            ],
          },
        },
      ],
    },
  };
  const rows = financialReportCsv(report);
  expect(rows.some((r) => r[3] === "9007199254740993.01")).toBe(true);
  expect(
    rows.some((r) => r[1] === "Zona waktu" && r[3] === "Asia/Jakarta"),
  ).toBe(true);
  expect(
    rows.some((r) => r[1] === "Selisih rekonsiliasi" && r[3] === "0.00"),
  ).toBe(true);
  expect(
    rows.some(
      (r) =>
        r[0] === "Riwayat klasifikasi" &&
        r[1] === "Setoran pemilik" &&
        r[7] === "Admin",
    ),
  ).toBe(true);
});
