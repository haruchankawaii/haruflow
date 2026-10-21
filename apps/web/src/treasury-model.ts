import { moneyCents } from "./dashboard-charts";
export interface TreasuryAccount { account_code: string; name: string; kind: "cash" | "bank"; currency: "IDR"; bank_name: string | null; account_number: string | null; balance: string }
export interface TreasuryTransfer { id: string; number: string; from_account: string; to_account: string; amount: string; reference: string | null; reason: string; journal_id: string; created_at: string; version: number; reversal: { reason: string; created_at: string } | null }
export interface BankStatement { id: string; reference: string; from_date: string; to_date: string; opening_balance: string; closing_balance: string; ledger_opening: string; ledger_closing: string; unmatched_bank: number; unmatched_ledger: number }
export interface BankLine { id: string; statement_id: string; transaction_date: string; reference: string; description: string; amount: string; version: number; statement_reference: string; journal_line_id: string | null; journal_id: string | null; history: { action: string; reason: string; created_at: string; version: number }[] }
export interface TreasuryLedgerLine { id: string; journal_id: string; date: string; description: string; amount: string; matched: boolean }
export interface BankReconciliation { account_code: string; statements: BankStatement[]; lines: BankLine[]; ledger: TreasuryLedgerLine[] }
export interface StatementPreview { rows: { row_number: number; transaction_date: string; reference: string; description: string; amount: string }[]; count: number; sha256: string; opening_balance: string; closing_balance: string; valid: boolean }
export const statementTemplate = "date,reference,description,amount\n2026-10-01,BANK-001,Setoran awal,1000000.00\n2026-10-01,BANK-002,Pembayaran supplier,-250000.00\n";
export function statementReconciled(statement: BankStatement): boolean {
  return statement.unmatched_bank === 0 && statement.unmatched_ledger === 0 && moneyCents(statement.opening_balance) === moneyCents(statement.ledger_opening) && moneyCents(statement.closing_balance) === moneyCents(statement.ledger_closing);
}
export function matchingCandidates(line: BankLine, ledger: TreasuryLedgerLine[]): TreasuryLedgerLine[] {
  return ledger.filter(candidate => !candidate.matched && moneyCents(candidate.amount) === moneyCents(line.amount));
}
export function transferError(from: string, to: string, amount: string, balance: string): string {
  if (!from || !to || from === to) return "Pilih rekening sumber dan tujuan yang berbeda.";
  if (!/^\d+(?:\.\d{1,2})?$/.test(amount) || moneyCents(amount) <= 0n || moneyCents(amount) > 100000000000000n) return "Isi nominal positif, maksimal dua angka desimal.";
  return moneyCents(amount) > moneyCents(balance) ? "Saldo rekening sumber tidak cukup." : "";
}
