import { moneyCents } from "./dashboard-charts";
import type { PurchaseOrder, PurchaseReceipt } from "./purchasing-model";
import type { ReturnableItem, ReturnRecord } from "./returns-model";
import type { DocumentAllocation } from "./DocumentAllocations";

export type BillDueStatus =
  "upcoming" | "due_today" | "overdue" | "void" | "paid";
export interface SupplierPayment {
  id: string;
  number: string;
  amount: string;
  reference: string | null;
  journal_id: string;
  created_at: string;
  can_reverse: boolean;
  reversal: PurchaseReceipt["reversal"];
}
export interface SupplierBill {
  fund_allocations?: DocumentAllocation[];
  credit_total: string;
  net_total: string;
  can_return: boolean;
  returns: ReturnRecord[];
  id: string;
  number: string;
  supplier_id: string;
  supplier_code: string;
  supplier_name: string;
  supplier_invoice_number: string;
  receipt_id: string;
  receipt_number: string;
  order_id: string;
  order_number: string;
  invoice_date: string;
  due_date: string;
  total: string;
  paid: string;
  outstanding: string;
  status: "unpaid" | "partial" | "paid" | "void";
  can_pay: boolean;
  payments: SupplierPayment[];
  due_status: BillDueStatus;
  version: number;
  notes: string | null;
  journal_id: string;
  created_at: string;
  items: (PurchaseReceipt["items"][number] & ReturnableItem)[];
  reversal: PurchaseReceipt["reversal"];
  can_reverse: boolean;
  reversal_blocked_reason: string | null;
}
export interface SupplierBillsSummary {
  today: string;
  timezone: string;
  as_of: string;
  counts: Record<"unpaid" | Exclude<BillDueStatus, "paid">, number> & {
    paid?: number;
    partial?: number;
  };
  outstanding_total: string;
  overdue_total: string;
  due_today_total: string;
  upcoming_total: string;
  suppliers: {
    supplier_id: string;
    supplier_code: string;
    supplier_name: string;
    bill_count: number;
    outstanding_total: string;
    overdue_total: string;
  }[];
}
export const billDueNames: Record<BillDueStatus, string> = {
  upcoming: "Belum jatuh tempo",
  due_today: "Jatuh tempo hari ini",
  overdue: "Lewat jatuh tempo",
  void: "Dibatalkan",
  paid: "Lunas",
};
export function billChartRows(summary: SupplierBillsSummary) {
  const statuses = ["upcoming", "due_today", "overdue"] as const;
  const values = statuses.map((status) =>
    moneyCents(summary[`${status}_total`]),
  );
  const maximum = values.reduce((a, b) => (a > b ? a : b), 0n);
  return statuses.map((status, index) => ({
    status,
    count: summary.counts[status],
    value: values[index],
    // Number is limited to SVG geometry; labels and totals stay exact.
    width: maximum > 0n ? Number((values[index] * 10_000n) / maximum) / 100 : 0,
  }));
}
export function validCalendarDate(value: string): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return false;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  if (year < 1 || month < 1 || month > 12 || day < 1) return false;
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  return (
    day <=
    [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1]
  );
}
export interface BillDraft {
  supplier_invoice_number: string;
  invoice_date: string;
  due_date: string;
  notes: string;
}
export function billDraftIssue(draft: BillDraft, today: string): string {
  if (
    !draft.supplier_invoice_number.trim() ||
    draft.supplier_invoice_number.trim().length > 100
  )
    return "Nomor invoice supplier wajib diisi, maksimal 100 karakter.";
  if (
    !validCalendarDate(draft.invoice_date) ||
    !validCalendarDate(draft.due_date)
  )
    return "Isi tanggal invoice dan jatuh tempo yang valid (tahun 0001–9999).";
  if (draft.invoice_date > today)
    return "Tanggal invoice tidak boleh melewati hari ini menurut zona waktu perusahaan.";
  if (draft.due_date < draft.invoice_date)
    return "Jatuh tempo tidak boleh sebelum tanggal invoice.";
  if (draft.notes.trim().length > 2000)
    return "Catatan maksimal 2.000 karakter.";
  return "";
}
export function eligibleBillReceipts(orders: PurchaseOrder[]) {
  return orders.flatMap((order) =>
    order.receipts
      .filter((receipt) => !receipt.reversal && receipt.can_bill)
      .map((receipt) => ({ order, receipt })),
  );
}

export function billAllocationPreview(
  receipt: PurchaseReceipt | undefined,
  quantities: Record<string, string>,
) {
  const items: { receipt_line_id: string; quantity: string }[] = [];
  let total = 0n;
  let issue = "";
  const qty = (v: string) => {
    const [w, f = ""] = v.split(".");
    return BigInt(w) * 1000n + BigInt(f.padEnd(3, "0"));
  };
  for (const line of receipt?.items ?? []) {
    const value =
      quantities[line.id] ?? line.remaining_bill_quantity ?? line.quantity;
    if (!/^\d+(\.\d{1,3})?$/.test(value)) {
      issue = "Jumlah tagihan maksimal tiga desimal.";
      continue;
    }
    const requested = qty(value),
      remaining = qty(line.remaining_bill_quantity ?? line.quantity);
    if (requested > remaining) {
      issue = "Jumlah tagihan melampaui sisa penerimaan.";
      continue;
    }
    if (requested === 0n) continue;
    const original = qty(line.quantity),
      already = qty(line.billed_quantity ?? "0"),
      originalValue = moneyCents(line.value),
      billedValue = moneyCents(line.billed_value ?? "0");
    let allocated =
      requested === remaining
        ? originalValue - billedValue
        : ((already + requested) * originalValue * 2n + original) /
            (2n * original) -
          billedValue;
    if (requested < remaining) {
      const maximum = originalValue - billedValue - 1n;
      allocated = allocated > maximum ? maximum : allocated;
    }
    if (allocated <= 0n) {
      issue = "Perbesar jumlah barang agar nilai baris minimal satu sen.";
      continue;
    }
    if (allocated < 0n) {
      issue = "Nilai tagihan tidak cocok. Muat data terbaru.";
      continue;
    }
    total += allocated;
    items.push({ receipt_line_id: line.id, quantity: value });
  }
  if (!items.length || total <= 0n)
    issue =
      issue || "Pilih jumlah barang dengan total tagihan lebih besar dari nol.";
  return { items, total, issue };
}
