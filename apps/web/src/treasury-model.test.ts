import { describe, expect, test } from "bun:test";
import { matchingCandidates, statementReconciled, transferError, type BankLine, type BankStatement } from "./treasury-model";
import { requestIdentity } from "./purchasing-model";
describe("cash and bank reconciliation", () => {
 test("both bank lines and book balances must agree, not only total", () => {
  const row: BankStatement = { id: "s", reference: "BANK", from_date: "2026-10-01", to_date: "2026-10-31", opening_balance: "100.00", closing_balance: "150.00", ledger_opening: "100", ledger_closing: "150", unmatched_bank: 0, unmatched_ledger: 0 };
  expect(statementReconciled(row)).toBe(true);
  expect(statementReconciled({ ...row, unmatched_bank: 1 })).toBe(false);
  expect(statementReconciled({ ...row, unmatched_ledger: 1 })).toBe(false);
  expect(statementReconciled({ ...row, ledger_opening: "99.99" })).toBe(false);
 });
 test("matching preserves sign, cents, and prevents reused book lines", () => {
  const line = { amount: "-100.01" } as BankLine;
  const common = { journal_id: "j", date: "2026-10-01", description: "Refund" };
  expect(matchingCandidates(line, [{ ...common, id: "a", amount: "100.01", matched: false }, { ...common, id: "b", amount: "-100.01", matched: false }, { ...common, id: "c", amount: "-100.01", matched: true }]).map(row => row.id)).toEqual(["b"]);
 });
 test("transfers validate each source balance exactly", () => {
  expect(transferError("1100", "1110", "0.01", "0.01")).toBe("");
  expect(transferError("1100", "1110", "0.02", "0.01")).toContain("cukup");
  expect(transferError("1100", "1100", "1", "2")).toContain("berbeda");
  expect(transferError("1100", "1110", "1.001", "2")).toContain("desimal");
 });
 test("retry stays idempotent while changing cash destination creates a new request", () => {
  let sequence = 0;
  const key = () => `bank-${++sequence}`;
  const payment = { path: "/invoices/invoice/payments", amount: "100.00", reference: "BANK-1", cash_account_code: "1100" };
  const original = requestIdentity(null, payment, key);
  expect(requestIdentity(original, { ...payment }, key)).toBe(original);
  const bank = requestIdentity(original, { ...payment, cash_account_code: "1110" }, key);
  expect(bank.key).not.toBe(original.key);
  expect(requestIdentity(bank, { ...payment, cash_account_code: "1110" }, key)).toBe(bank);
 });
});
