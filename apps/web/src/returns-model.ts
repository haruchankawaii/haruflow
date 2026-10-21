import { moneyCents } from "./dashboard-charts";

export type ReturnKind = "sales" | "supplier";
export type ReturnRefundMode = "immediate" | "credit" | "deferred";
export const returnRefundNames: Record<ReturnRefundMode, string> = {
  immediate: "Dana sudah dikembalikan",
  credit: "Simpan kredit pihak",
  deferred: "Menunggu refund",
};
export function returnSettlementCash(refund: bigint, mode: ReturnRefundMode) {
  return mode === "immediate" ? refund : 0n;
}
export interface ReturnableItem {
  id: string;
  quantity: string;
  total?: string;
  value?: string;
  unit?: string;
  product_sku?: string;
  sku?: string;
  product_name?: string;
  name?: string;
  product_kind?: "stock" | "service";
  returned_quantity: string;
  remaining_return_quantity: string;
  returned_credit: string;
}
export interface ReturnRecord {
  id: string;
  number: string;
  credit_total: string;
  inventory_value?: string;
  cost_total?: string;
  refund: string;
  refund_mode?: ReturnRefundMode;
  settlement_mode?: ReturnRefundMode;
  settlement_credit?: string;
  credit_fund_id?: string | null;
  refund_request_id?: string | null;
  reason: string;
  journal_id: string;
  created_at: string;
  items: {
    id: string;
    product_sku?: string;
    sku?: string;
    product_name?: string;
    name?: string;
    unit?: string;
    product_kind?: string;
    quantity: string;
    credit: string;
    value?: string;
    cost?: string;
  }[];
  reversal: null | {
    id: string;
    reason: string;
    created_at: string;
    journal_id: string;
  };
  can_reverse: boolean;
  reversal_blocked_reason: string | null;
}
export interface ReturnableDocument {
  version: number;
  credit_total: string;
  net_total: string;
  can_return: boolean;
  returns: ReturnRecord[];
  items: ReturnableItem[];
}
export function quantityMillis(value: string): bigint {
  const match = /^(\d+)(?:\.(\d{1,3}))?$/.exec(value);
  if (!match)
    throw new Error(
      "Jumlah harus berupa angka positif dengan maksimal 3 desimal.",
    );
  const amount =
    BigInt(match[1]) * 1000n + BigInt((match[2] ?? "").padEnd(3, "0"));
  if (amount > 1000000000000n)
    throw new Error("Jumlah melebihi batas transaksi.");
  return amount;
}
export function returnQuantityLabel(value: string): string {
  return value.replace(/(\.\d*?)0+$/, "$1").replace(/\.$/, "");
}
export function returnItemName(item: {
  product_name?: string;
  name?: string;
  product_sku?: string;
  sku?: string;
}) {
  return (
    item.product_name ?? item.name ?? item.product_sku ?? item.sku ?? "Produk"
  );
}
export function centsAmount(value: bigint): string {
  const absolute = value < 0n ? -value : value;
  return `${value < 0n ? "-" : ""}${absolute / 100n}.${(absolute % 100n).toString().padStart(2, "0")}`;
}
export interface ReturnPreview {
  items: { id: string; quantity: string; credit: bigint }[];
  credit: bigint;
  netAfter: bigint;
  refund: bigint;
  paidAfter: bigint;
  outstandingAfter: bigint;
  error: string;
}
export function previewReturn(
  items: ReturnableItem[],
  quantities: Record<string, string>,
  total: string,
  paid: string,
  creditTotal: string,
): ReturnPreview {
  const result: ReturnPreview = {
    items: [],
    credit: 0n,
    netAfter: moneyCents(total) - moneyCents(creditTotal),
    refund: 0n,
    paidAfter: moneyCents(paid),
    outstandingAfter: 0n,
    error: "",
  };
  try {
    for (const item of items) {
      const raw = quantities[item.id]?.trim();
      if (!raw) continue;
      const selected = quantityMillis(raw);
      if (selected === 0n) continue;
      const quantity = quantityMillis(item.quantity),
        returned = quantityMillis(item.returned_quantity);
      if (
        quantity === 0n ||
        selected > quantityMillis(item.remaining_return_quantity) ||
        returned + selected > quantity
      )
        throw new Error(
          `${returnItemName(item)}: jumlah retur melebihi sisa yang dapat dikembalikan.`,
        );
      const totalCredit = moneyCents(item.total ?? item.value ?? "0.00"),
        credited = moneyCents(item.returned_credit);
      const remainingCredit = totalCredit - credited;
      const cumulativeQuantity = returned + selected;
      // Positive half-away rounding, matching the server's cumulative allocation.
      let credit =
        (2n * totalCredit * cumulativeQuantity + quantity) / (2n * quantity) -
        credited;
      if (cumulativeQuantity === quantity) credit = remainingCredit;
      else if (credit > remainingCredit - 1n) credit = remainingCredit - 1n;
      if (credit <= 0n)
        throw new Error(
          `${returnItemName(item)}: jumlah terlalu kecil untuk kredit minimal Rp0,01. Gabungkan jumlah retur.`,
        );
      result.items.push({ id: item.id, quantity: raw, credit });
      result.credit += credit;
    }
    result.netAfter -= result.credit;
    if (result.netAfter < 0n)
      throw new Error(
        "Kredit retur melebihi nilai tagihan tersisa. Muat ulang dokumen.",
      );
    result.refund =
      result.paidAfter > result.netAfter
        ? result.paidAfter - result.netAfter
        : 0n;
    result.paidAfter -= result.refund;
    result.outstandingAfter = result.netAfter - result.paidAfter;
    if (!result.items.length)
      result.error = "Isi jumlah pada minimal satu baris yang akan diretur.";
  } catch (error) {
    result.error =
      error instanceof Error ? error.message : "Jumlah retur tidak valid.";
  }
  return result;
}
export function returnEndpoint(kind: ReturnKind, documentId: string): string {
  return kind === "sales"
    ? `/invoices/${documentId}/returns`
    : `/purchase-bills/${documentId}/returns`;
}
export function returnReversalEndpoint(kind: ReturnKind, id: string): string {
  return kind === "sales"
    ? `/sales-returns/${id}/reverse`
    : `/supplier-returns/${id}/reverse`;
}
