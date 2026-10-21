import { expect, test } from "bun:test";
import {
  allocationPreview,
  positivePartyAmount,
  partyFundActionInput,
  statementCsv,
  type PartyStatement,
  type PartyTarget,
} from "./party-balances-model";
test("financial corrections use the strict reversal contract while actual refunds carry confirmation", () => {
  for (const kind of [
    "reverse",
    "refund_reverse",
    "allocation_reverse",
    "request_cancel",
  ] as const)
    expect(
      partyFundActionInput(kind, 3, " Salah bukti ", "100", "REF-1"),
    ).toEqual({ version: 3, reason: "Salah bukti" });
  expect(
    partyFundActionInput("request", 3, "Dana disisihkan", "50", "unused"),
  ).toEqual({ version: 3, reason: "Dana disisihkan", amount: "50" });
  for (const kind of ["refund", "request_pay"] as const)
    expect(
      partyFundActionInput(kind, 3, "Pengembalian aktual", "50", " REF-1 "),
    ).toEqual({
      version: 3,
      reason: "Pengembalian aktual",
      amount: "50",
      confirmed: true,
      reference: "REF-1",
    });
});
const target = (
  id: string,
  outstanding: string,
  party_id = "customer",
): PartyTarget => ({
  target_type: "invoice",
  id,
  number: id,
  party_id,
  party_name: "Pelanggan",
  total: outstanding,
  credit_total: "0",
  paid: "0",
  outstanding,
  document_date: null,
  due_date: null,
  version: 4,
});
test("allocations preserve exact money and current target versions without counting reserved refunds", () => {
  const preview = allocationPreview(
    { party_id: "customer", available: "400000.00" },
    [target("one", "200000.25"), target("two", "200000.00")],
    { "invoice:one": "200000.25", "invoice:two": "199999.75" },
  );
  expect(preview.error).toBe("");
  expect(preview.total).toBe(40000000n);
  expect(preview.items.map((item) => item.version)).toEqual([4, 4]);
  expect(
    allocationPreview(
      { party_id: "customer", available: "399999.99" },
      [target("one", "400000")],
      { "invoice:one": "400000" },
    ).error,
  ).toContain("disisihkan");
});
test("allocations reject cross-party, over-target, malformed and zero selections", () => {
  const fund = { party_id: "customer", available: "1000" };
  expect(
    allocationPreview(fund, [target("one", "500", "supplier")], {
      "invoice:one": "100",
    }).error,
  ).toContain("pihak yang sama");
  for (const amount of ["500.01", "1e2", "0.001", "-1", "0"])
    expect(
      allocationPreview(fund, [target("one", "500")], { "invoice:one": amount })
        .error,
    ).not.toBe("");
  expect(positivePartyAmount("1000000000000.00")).toBe(true);
  expect(positivePartyAmount("1000000000000.01")).toBe(false);
});
test("statement CSV retains unknown due aging, separate credit/cash and global reconciliation", () => {
  const statement = {
    party: { name: "Pelanggan" },
    side: "customer",
    timezone: "Asia/Jakarta",
    from: null,
    as_of: "2026-10-20",
    summary: {
      outstanding: "100.01",
      credit_balance: "50.00",
      available_credit: "35.00",
      reserved_refunds: "15.00",
    },
    opening: { debt_balance: "100.01", credit_balance: "50.00" },
    events: [
      {
        created_at: "now",
        event_type: "allocation",
        number: "A",
        debt_delta: "-15.00",
        credit_delta: "-15.00",
        cash_delta: "0.00",
        debt_balance: "85.01",
        credit_balance: "35.00",
        journal_id: "j",
        source_id: "s",
      },
    ],
    aging: [{ key: "unknown", count: 1, total: "100.01" }],
    reconciliation: {
      ar_or_ap_ledger: "100.01",
      ar_or_ap_subledger: "100.01",
      ar_or_ap_difference: "0.00",
      credits_ledger: "50.00",
      credits_subledger: "50.00",
      credits_difference: "0.00",
    },
  } as PartyStatement;
  const rows = statementCsv(statement);
  expect(rows.some((row) => row.includes("Jatuh tempo belum diketahui"))).toBe(
    true,
  );
  expect(
    rows.some((row) => row.includes("0.00") && row.includes("-15.00")),
  ).toBe(true);
  expect(
    rows.some((row) => row.includes("Rekonsiliasi seluruh pihak sejenis")),
  ).toBe(true);
});
