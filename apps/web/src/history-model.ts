export interface HistoryFilters {
  q: string;
  status: string;
  from: string;
  to: string;
}
export const emptyHistoryFilters: HistoryFilters = {
  q: "",
  status: "",
  from: "",
  to: "",
};
export function historyQuery(
  filters: HistoryFilters,
  page = 1,
  exporting = false,
) {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(filters))
    if (value.trim()) query.set(key, value.trim());
  if (!exporting) {
    query.set("page", String(page));
    query.set("page_size", "50");
  }
  return query.toString();
}
export const historyKinds = [
  ["sales-orders", "Pesanan penjualan"],
  ["invoices", "Invoice pelanggan"],
  ["purchase-orders", "Pesanan pembelian"],
  ["purchase-bills", "Tagihan supplier"],
  ["landed-costs", "Biaya perolehan"],
  ["customers", "Pelanggan"],
  ["suppliers", "Supplier"],
  ["products", "Produk"],
  ["inventory-movements", "Mutasi persediaan"],
  ["journals", "Jurnal"],
  ["party-funds", "Uang muka dan kredit"],
  ["imports", "Impor data"],
  ["audit", "Aktivitas"],
] as const;

const statusLabels: Record<string, string> = {
  active: "Aktif", inactive: "Nonaktif", stock: "Barang stok", service: "Jasa",
  draft: "Draf", confirmed: "Dikonfirmasi", shipped: "Dikirim", invoiced: "Ditagihkan", cancelled: "Dibatalkan",
  unpaid: "Belum dibayar", partial: "Dibayar sebagian", paid: "Lunas", void: "Dibatalkan",
  received: "Diterima", partial_received: "Diterima sebagian",
  cash_advance: "Uang muka", return_credit: "Kredit retur", opening_credit: "Kredit awal",
  ready: "Siap disimpan", invalid: "Tidak valid", committed: "Tersimpan",
  invoice: "Invoice pelanggan", payment: "Pembayaran pelanggan", manual: "Jurnal manual", reversal: "Pembalikan",
  receipt: "Penerimaan stok", shipment: "Pengiriman stok", purchase_receipt: "Penerimaan PO",
  purchase_bill: "Tagihan supplier", supplier_payment: "Pembayaran supplier", opening: "Saldo awal",
  sales_return: "Retur pelanggan", supplier_return: "Retur supplier", party_fund: "Dana pihak",
  party_allocation: "Alokasi saldo", party_refund: "Refund pihak",
  landed_cost: "Biaya perolehan", posted: "Dibukukan", reversed: "Dibalik",
};
export const historyStatusLabel = (status: string) => statusLabels[status] ?? status;
export function historyStatuses(kind: string): string[] {
  if (["customers", "suppliers"].includes(kind)) return kind === "customers" ? ["active"] : ["active", "inactive"];
  if (kind === "products") return ["stock", "service"];
  if (kind === "sales-orders") return ["draft", "confirmed", "shipped", "invoiced", "cancelled"];
  if (kind === "purchase-orders") return ["draft", "confirmed", "partial_received", "received", "cancelled"];
  if (["invoices", "purchase-bills"].includes(kind)) return ["unpaid", "partial", "paid", "void"];
  if (kind === "landed-costs") return ["posted", "reversed"];
  if (kind === "party-funds") return ["cash_advance", "return_credit", "opening_credit"];
  if (kind === "imports") return ["ready", "invalid", "committed"];
  if (kind === "journals") return ["opening", "manual", "invoice", "payment", "purchase_receipt", "purchase_bill", "landed_cost", "supplier_payment", "sales_return", "supplier_return", "party_fund", "party_allocation", "party_refund", "reversal"];
  return [];
}
