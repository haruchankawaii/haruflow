import { moneyCents } from "./dashboard-charts";
export type PartySide = "customer" | "supplier";
export type PartyTargetType =
  "invoice" | "bill" | "opening_receivable" | "opening_payable";
export interface PartyTarget {
  target_type: PartyTargetType;
  id: string;
  number: string;
  party_id: string;
  party_name: string;
  total: string;
  credit_total: string;
  paid: string;
  outstanding: string;
  document_date: string | null;
  due_date: string | null;
  version: number;
  bucket?: string;
  days_overdue?: number | null;
}
export interface PartyReversal {
  id: string;
  reason: string;
  created_at: string;
  journal_id: string;
}
export interface PartyAllocation {
  id: string;
  target_type: PartyTargetType;
  target_id: string;
  number: string;
  amount: string;
  journal_id: string;
  created_at: string;
  reversed: boolean;
  reversal: PartyReversal | null;
}
export interface PartyRefundRequest {
  id: string;
  fund_id: string;
  amount: string;
  paid: string;
  remaining: string;
  cancelled: boolean;
  reason: string;
  created_at: string;
}
export interface PartyRefund {
  id: string;
  fund_id: string;
  request_id: string | null;
  amount: string;
  reference: string | null;
  reason: string;
  journal_id: string;
  created_at: string;
  reversed: boolean;
  reversal: PartyReversal | null;
}
export interface PartyFund {
  id: string;
  number: string;
  side: PartySide;
  party_id: string;
  party_name: string;
  kind: string;
  amount: string;
  allocated: string;
  refunded: string;
  reserved: string;
  available: string;
  version: number;
  reference: string | null;
  description: string | null;
  source_type: string;
  source_id: string;
  journal_id: string;
  created_at: string;
  reversed: boolean;
  allocations: PartyAllocation[];
  refund_requests: PartyRefundRequest[];
  refunds: PartyRefund[];
  source_url?: string;
}
export interface PartyEvent {
  id: string;
  event_type: string;
  number: string;
  source_type: string;
  source_id: string;
  target_type?: string;
  target_id?: string;
  journal_id: string | null;
  created_at: string;
  debt_delta: string;
  credit_delta: string;
  cash_delta: string;
  debt_balance: string;
  credit_balance: string;
  net_balance: string;
  url?: string;
}
export interface PartyStatement {
  side: PartySide;
  party: { id: string; name: string };
  timezone: string;
  as_of: string;
  from: string | null;
  summary: {
    outstanding: string;
    credit_balance: string;
    available_credit: string;
    reserved_refunds: string;
    net_balance: string;
  };
  opening: {
    debt_balance: string;
    credit_balance: string;
    net_balance: string;
  };
  events: PartyEvent[];
  documents: PartyTarget[];
  aging: { key: string; count: number; total: string }[];
  reconciliation: {
    ar_or_ap_ledger: string;
    ar_or_ap_subledger: string;
    ar_or_ap_difference: string;
    credits_ledger: string;
    credits_subledger: string;
    credits_difference: string;
    balanced: boolean;
  };
}
export const agingNames: Record<string, string> = {
  current: "Belum jatuh tempo",
  days_1_30: "Terlambat 1–30 hari",
  days_31_60: "Terlambat 31–60 hari",
  days_61_90: "Terlambat 61–90 hari",
  over_90: "Lebih dari 90 hari",
  unknown: "Jatuh tempo belum diketahui",
};
export const fundKindNames: Record<string, string> = {
  cash_advance: "Uang muka",
  advance: "Uang muka",
  return_credit: "Kredit retur",
  opening_credit: "Kredit pembukaan",
  customer_credit: "Kredit pelanggan",
  supplier_credit: "Kredit supplier",
};
export const eventNames: Record<string, string> = {
  invoice: "Invoice",
  bill: "Tagihan supplier",
  payment: "Pembayaran",
  supplier_payment: "Pembayaran supplier",
  advance: "Uang muka",
  allocation: "Pemakaian saldo",
  allocation_reversal: "Pembalikan pemakaian saldo",
  refund: "Refund aktual",
  refund_reversal: "Pembalikan refund",
  refund_request: "Permintaan refund",
  refund_request_cancelled: "Pembatalan permintaan refund",
  return_credit: "Kredit retur",
  opening: "Saldo pembukaan",
  reversal: "Pembalikan",
  document_reversal: "Pembalikan tagihan",
  payment_reversal: "Pembalikan pembayaran",
  return: "Retur",
  return_reversal: "Pembalikan retur",
  opening_credit: "Kredit pembukaan",
  opening_reversal: "Pembalikan saldo awal",
  fund_reversal: "Pembalikan sumber dana",
  opening_receivable: "Piutang pembukaan",
  opening_payable: "Utang pembukaan",
  purchase_bill: "Tagihan supplier",
};
export type PartyFundActionType =
  | "request"
  | "refund"
  | "request_pay"
  | "request_cancel"
  | "allocation_reverse"
  | "refund_reverse"
  | "reverse";
export function partyFundActionInput(
  type: PartyFundActionType,
  version: number,
  reason: string,
  amount: string,
  reference: string,
) {
  const base = { version, reason: reason.trim() };
  return type === "refund" || type === "request_pay"
    ? { ...base, amount, confirmed: true, reference: reference.trim() || null }
    : type === "request"
      ? { ...base, amount }
      : base;
}
export function positivePartyAmount(value: string, maximum?: string) {
  try {
    const amount = moneyCents(value);
    return (
      amount > 0n &&
      amount <= 100000000000000n &&
      (maximum === undefined || amount <= moneyCents(maximum))
    );
  } catch {
    return false;
  }
}
export function allocationPreview(
  fund: Pick<PartyFund, "party_id" | "available">,
  targets: PartyTarget[],
  amounts: Record<string, string>,
) {
  let total = 0n;
  const items: {
    target_type: PartyTargetType;
    target_id: string;
    amount: string;
    version: number;
  }[] = [];
  let error = "";
  for (const target of targets) {
    const raw = amounts[`${target.target_type}:${target.id}`]?.trim();
    if (!raw || raw === "0" || raw === "0.00") continue;
    if (target.party_id !== fund.party_id) {
      error ||= "Saldo hanya dapat dipakai pada tagihan milik pihak yang sama.";
      continue;
    }
    if (!positivePartyAmount(raw, target.outstanding)) {
      error ||= `${target.number}: isi nominal positif maksimal sisa tagihan dengan dua desimal.`;
      continue;
    }
    total += moneyCents(raw);
    items.push({
      target_type: target.target_type,
      target_id: target.id,
      amount: raw,
      version: target.version,
    });
  }
  if (!items.length) error ||= "Isi nominal pada minimal satu tagihan.";
  if (total > moneyCents(fund.available))
    error ||=
      "Pemakaian melebihi saldo tersedia. Dana yang disisihkan untuk refund tidak dapat dialokasikan.";
  return { total, items, error };
}
export function targetHref(target: Pick<PartyTarget, "target_type" | "id">) {
  return target.target_type === "invoice"
    ? `?invoice=${target.id}#sales`
    : target.target_type === "bill"
      ? `?bill=${target.id}#purchasing`
      : null;
}
export function eventHref(event: PartyEvent) {
  if (event.url) return event.url;
  if (["advance", "return_credit", "opening_credit"].includes(event.event_type))
    return `?fund=${event.id}#balances`;
  if (event.source_type === "opening_item")
    return `?opening_item=${event.source_id}#balances`;
  if (event.target_id && event.target_type === "invoice")
    return `?invoice=${event.target_id}#sales`;
  if (event.target_id && event.target_type === "bill")
    return `?bill=${event.target_id}#purchasing`;
  if (event.source_type === "invoice")
    return `?invoice=${event.source_id}#sales`;
  if (event.source_type === "purchase_bill" || event.source_type === "bill")
    return `?bill=${event.source_id}#purchasing`;
  return null;
}
export function statementCsv(statement: PartyStatement) {
  const rows: unknown[][] = [
    ["Pihak", statement.party.name],
    ["Jenis", statement.side],
    ["Zona waktu", statement.timezone],
    ["Dari", statement.from ?? "Awal pembukuan"],
    ["Posisi per", statement.as_of],
    ["Sisa tagihan", statement.summary.outstanding],
    ["Kredit total", statement.summary.credit_balance],
    ["Kredit tersedia", statement.summary.available_credit],
    ["Refund disisihkan", statement.summary.reserved_refunds],
    ["Saldo tagihan awal rentang", statement.opening.debt_balance],
    ["Saldo kredit awal rentang", statement.opening.credit_balance],
    [],
    [
      "Tanggal",
      "Jenis",
      "Nomor",
      "Perubahan tagihan",
      "Perubahan kredit",
      "Kas aktual (+ masuk / - keluar)",
      "Saldo tagihan",
      "Saldo kredit",
      "ID jurnal",
      "ID sumber",
    ],
  ];
  for (const event of statement.events)
    rows.push([
      event.created_at,
      eventNames[event.event_type] ?? event.event_type,
      event.number,
      event.debt_delta,
      event.credit_delta,
      event.cash_delta,
      event.debt_balance,
      event.credit_balance,
      event.journal_id,
      event.source_id,
    ]);
  rows.push(
    [],
    [
      "Dokumen",
      "Jenis",
      "Tanggal dokumen",
      "Jatuh tempo",
      "Nilai awal",
      "Kredit retur",
      "Sudah diselesaikan",
      "Sisa",
      "ID dokumen",
    ],
  );
  for (const document of statement.documents ?? [])
    rows.push([
      document.number,
      document.target_type,
      document.document_date,
      document.due_date,
      document.total,
      document.credit_total,
      document.paid,
      document.outstanding,
      document.id,
    ]);
  rows.push([], ["Aging", "Jumlah dokumen", "Sisa"]);
  for (const bucket of statement.aging)
    rows.push([
      agingNames[bucket.key] ?? bucket.key,
      bucket.count,
      bucket.total,
    ]);
  rows.push(
    [],
    [
      "Rekonsiliasi seluruh pihak sejenis",
      "Ledger",
      "Buku pembantu",
      "Selisih",
    ],
    [
      "Piutang/utang",
      statement.reconciliation.ar_or_ap_ledger,
      statement.reconciliation.ar_or_ap_subledger,
      statement.reconciliation.ar_or_ap_difference,
    ],
    [
      "Kredit pihak",
      statement.reconciliation.credits_ledger,
      statement.reconciliation.credits_subledger,
      statement.reconciliation.credits_difference,
    ],
  );
  return rows;
}
