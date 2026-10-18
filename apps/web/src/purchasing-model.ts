import { moneyCents } from "./dashboard-charts";
import type { Product } from "./types";

export interface Supplier {
  id: string;
  code: string;
  name: string;
  email: string | null;
  phone: string | null;
  address: string | null;
  active: boolean;
  version: number;
  created_at: string;
  updated_at: string;
}
export interface PurchaseItem {
  id: string;
  product_id: string;
  product_sku: string;
  product_name: string;
  unit: string;
  quantity: string;
  unit_cost: string;
  total: string;
}
export interface PurchaseOrder {
  id: string;
  number: string;
  supplier_id: string;
  supplier_code: string;
  supplier_name: string;
  status: "draft" | "confirmed" | "cancelled";
  expected_date: string | null;
  notes: string | null;
  total: string;
  version: number;
  created_at: string;
  updated_at: string;
  confirmed_at: string | null;
  cancelled_at: string | null;
  cancellation_reason: string | null;
  items: PurchaseItem[];
}
export interface PurchaseDraft {
  supplier_id: string;
  expected_date: string;
  notes: string;
  items: { product_id: string; quantity: string; unit_cost: string }[];
}

export const purchaseProducts = (products: Product[]) =>
  products.filter((product) => product.kind !== "service");

export function purchaseDraft(order?: PurchaseOrder): PurchaseDraft {
  return {
    supplier_id: order?.supplier_id ?? "",
    expected_date: order?.expected_date ?? "",
    notes: order?.notes ?? "",
    items: order?.items.map(({ product_id, quantity, unit_cost }) => ({
      product_id,
      quantity,
      unit_cost,
    })) ?? [{ product_id: "", quantity: "1", unit_cost: "" }],
  };
}

// Integer arithmetic matches the server: round each line to cents, midpoint away
// from zero, before summing. Haruflow accepts positive purchase quantities/costs.
function scaled(value: string, places: number): bigint | null {
  const normalized = value.includes(".")
    ? value.replace(/0+$/, "").replace(/\.$/, "")
    : value;
  if (!new RegExp(`^\\d+(\\.\\d{1,${places}})?$`).test(normalized)) return null;
  const [whole, fraction = ""] = normalized.split(".");
  return (
    BigInt(whole) * 10n ** BigInt(places) + BigInt(fraction.padEnd(places, "0"))
  );
}
export function purchaseLineCents(
  quantity: string,
  cost: string,
): bigint | null {
  const qty = scaled(quantity, 3);
  const unitCost = scaled(cost, 4);
  if (qty === null || unitCost === null) return null;
  return (qty * unitCost + 50_000n) / 100_000n;
}
export function purchasePreview(draft: PurchaseDraft) {
  let total = 0n;
  let issue = "";
  const ids = new Set<string>();
  if (!draft.supplier_id) issue = "Pilih supplier aktif untuk pesanan ini.";
  if (!draft.items.length || draft.items.length > 100)
    issue ||= "Pesanan harus memiliki 1–100 baris barang.";
  for (const item of draft.items) {
    const qty = scaled(item.quantity, 3);
    const cost = scaled(item.unit_cost, 4);
    const value = purchaseLineCents(item.quantity, item.unit_cost);
    if (!item.product_id) issue ||= "Pilih barang pada setiap baris.";
    if (ids.has(item.product_id))
      issue ||= "Gabungkan barang yang sama dalam satu baris.";
    ids.add(item.product_id);
    if (qty === null || qty <= 0n || qty > 1_000_000_000_000n)
      issue ||=
        "Jumlah harus lebih dari nol, maksimal 1 miliar dengan 3 desimal.";
    if (cost === null || cost <= 0n || cost > 10_000_000_000_000n)
      issue ||=
        "Biaya satuan harus lebih dari nol, maksimal 1 miliar dengan 4 desimal.";
    if (value === null || value <= 0n || value > 100_000_000_000_000n)
      issue ||=
        "Nilai setiap baris setelah pembulatan harus lebih dari nol dan maksimal Rp1 triliun.";
    total += value ?? 0n;
  }
  if (total > 100_000_000_000_000n)
    issue ||= "Total pesanan maksimal Rp1 triliun.";
  return { total, valid: !issue, issue };
}

export function exactDecimal(value: string): string {
  const [whole, fraction = ""] = value.split(".");
  const trimmed = fraction.replace(/0+$/, "");
  return `${BigInt(whole).toLocaleString("id-ID")}${trimmed ? `,${trimmed}` : ""}`;
}
export function exactCost(value: string): string {
  return `Rp${exactDecimal(value)}`;
}
export function purchaseSummary(orders: PurchaseOrder[]) {
  const statuses = ["draft", "confirmed", "cancelled"] as const;
  return statuses.map((status) => {
    const items = orders.filter((order) => order.status === status);
    return {
      status,
      count: items.length,
      total: items.reduce((sum, order) => sum + moneyCents(order.total), 0n),
    };
  });
}

export interface RequestIdentity {
  payload: string;
  key: string;
}
export function requestIdentity(
  previous: RequestIdentity | null,
  payload: unknown,
  generate: () => string = () => crypto.randomUUID(),
): RequestIdentity {
  const serialized = JSON.stringify(payload);
  return previous?.payload === serialized
    ? previous
    : { payload: serialized, key: generate() };
}
