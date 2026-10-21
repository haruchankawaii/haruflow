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
  receipt_value?: string;
  subtotal?: string;
  shipping?: string;
  discount?: string;
  tax_value?: string;
  tax_snapshot?: {code:string;name:string;rate:string;basis:string}|null;
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
  prices: Record<string, string> = {},
) {
  const items: { receipt_line_id: string; quantity: string; unit_cost?:string }[] = [];
  let total = 0n;
  let receiptTotal = 0n;
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
    receiptTotal += allocated;
    const cost=prices[line.id]?.trim();
    let invoiceValue=allocated;
    if (cost) {
      if (!/^\d+(\.\d{1,4})?$/.test(cost)) {issue="Harga invoice maksimal empat desimal.";continue;}
      const [w,f=""]=cost.split("."); const units=BigInt(w)*10000n+BigInt(f.padEnd(4,"0"));
      invoiceValue=(requested*units+50000n)/100000n;
      if(units<=0n||units>10000000000000n||invoiceValue<=0n){issue="Harga invoice harus positif dan nilai baris minimal satu sen.";continue;}
    }
    total += invoiceValue;
    items.push({ receipt_line_id: line.id, quantity: value,...(cost?{unit_cost:cost}:{}) });
  }
  if (!items.length || total <= 0n)
    issue =
      issue || "Pilih jumlah barang dengan total tagihan lebih besar dari nol.";
  return { items, total, receiptTotal, issue };
}

export interface BillAdjustments {shipping:string;discount:string;tax_code:string;tax_name:string;tax_rate:string;tax_basis:"exclusive"|"inclusive";}
export const emptyBillAdjustments:BillAdjustments={shipping:"0",discount:"0",tax_code:"",tax_name:"",tax_rate:"0",tax_basis:"exclusive"};
export function billFinancialPreview(subtotal:bigint, draft:BillAdjustments) {
 const parse=(v:string)=>{if(!/^\d+(\.\d{1,2})?$/.test(v))return null;const[w,f=""]=v.split(".");return BigInt(w)*100n+BigInt(f.padEnd(2,"0"));};
 const shipping=parse(draft.shipping),discount=parse(draft.discount),rate=parse(draft.tax_rate);
 let issue="";
 if(shipping===null||discount===null||rate===null)issue="Ongkir, diskon, dan tarif pajak maksimal dua desimal.";
 const base=subtotal+(shipping??0n)-(discount??0n);
 if(base<=0n)issue||="Diskon harus lebih kecil dari subtotal ditambah ongkir.";
 if((rate??0n)>10000n)issue||="Tarif pajak maksimal 100%.";
 const hasTax=!!draft.tax_code.trim()||!!draft.tax_name.trim()||(rate??0n)>0n;
 if(hasTax&&(!draft.tax_code.trim()||!draft.tax_name.trim()))issue||="Isi kode dan nama pajak yang dipilih.";
 if(draft.tax_code.trim().length>64||draft.tax_name.trim().length>150)issue||="Kode pajak maksimal 64 dan nama maksimal 150 karakter.";
 const divisor=draft.tax_basis==="inclusive"?10000n+(rate??0n):10000n;
 const tax=base>0n?(base*(rate??0n)*2n+divisor)/(2n*divisor):0n;
 const total=base+(draft.tax_basis==="exclusive"?tax:0n);
 if(total>100000000000000n)issue||="Total tagihan maksimal Rp1 triliun.";
 return {shipping:shipping??0n,discount:discount??0n,tax,total,issue,payload:{shipping:draft.shipping,discount:draft.discount,...(hasTax?{tax:{code:draft.tax_code.trim(),name:draft.tax_name.trim(),rate:draft.tax_rate,basis:draft.tax_basis}}:{})}};
}
