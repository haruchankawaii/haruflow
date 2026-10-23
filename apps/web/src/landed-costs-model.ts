import { moneyCents } from "./dashboard-charts";

export interface LandedCostSource {
  line_id: string;
  journal_id: string;
  source_type: "purchase_bill" | "purchase_receipt";
  source_id: string;
  number: string;
  account_code: string;
  account_name: string;
  system_role?: string;
  amount?: string;
  available?: string;
  eligible?: boolean;
  blocked_reason?: string | null;
}
export interface LandedCostTarget {
  receipt_line_id: string;
  receipt_id: string;
  receipt_number: string;
  product_id: string;
  product_sku: string;
  product_name: string;
  unit: string;
  quantity: string;
  receipt_value: string;
  eligible: boolean;
  blocked_reason: string | null;
}
export interface LandedCostAllocation extends Omit<LandedCostTarget, "eligible" | "blocked_reason"> {
  weight: string;
  remaining_ratio: string;
  allocated: string;
  inventory_amount: string;
  expense_amount: string;
  before_quantity: string;
  before_value: string;
  before_average: string;
  after_value: string;
  after_average: string;
}
export interface LandedCostPreview {
  source: LandedCostSource;
  amount: string;
  inventory_amount: string;
  expense_amount: string;
  allocation_method: "value" | "quantity";
  reference: string | null;
  reason: string;
  items: LandedCostAllocation[];
  warnings: string[];
  preview_token: string;
}
export interface LandedCostDocument extends Omit<LandedCostPreview, "warnings" | "preview_token"> {
  id: string;
  number: string;
  version: number;
  status: "posted" | "reversed";
  journal_id: string;
  source_line_id: string;
  created_at: string;
  can_reverse: boolean;
  reversal_blocked_reason: string | null;
  reversal: { id: string; reason: string; created_at: string; journal_id: string } | null;
  location_events: { id: string; product_id: string; warehouse_id: string; lot_id: string | null; value: string; phase: "post" | "reverse"; created_at: string }[];
}
export interface LandedCostOptions { sources: LandedCostSource[]; targets: LandedCostTarget[] }
export interface LandedCostDraft {
  source_line_id: string;
  amount: string;
  allocation_method: "value" | "quantity";
  receipt_line_ids: string[];
  reference: string;
  reason: string;
}
export function landedCostInput(draft: LandedCostDraft, options: LandedCostOptions) {
  const source = options.sources.find((s) => s.line_id === draft.source_line_id);
  if (!source?.eligible || !source.available) throw new Error(source?.blocked_reason || "Pilih sumber biaya yang masih tersedia.");
  let amount: bigint;
  try { amount = moneyCents(draft.amount); } catch { throw new Error("Nominal harus berupa angka dengan maksimal dua desimal."); }
  if (amount <= 0n || amount > 100_000_000_000_000n || amount > moneyCents(source.available)) throw new Error("Nominal harus positif dan tidak melebihi sisa biaya sumber atau Rp1 triliun.");
  if (!draft.receipt_line_ids.length || draft.receipt_line_ids.length > 100 || new Set(draft.receipt_line_ids).size !== draft.receipt_line_ids.length) throw new Error("Pilih 1–100 baris penerimaan yang berbeda.");
  const products = new Set<string>();
  const units = new Set<string>();
  for (const id of draft.receipt_line_ids) {
    const target = options.targets.find((t) => t.receipt_line_id === id);
    if (!target?.eligible) throw new Error(target?.blocked_reason || "Penerimaan tujuan tidak tersedia.");
    if (products.has(target.product_id)) throw new Error("Pilih satu penerimaan untuk setiap produk dalam dokumen ini.");
    products.add(target.product_id); units.add(target.unit);
  }
  if (draft.allocation_method === "quantity" && units.size > 1) throw new Error("Metode kuantitas memerlukan satuan dasar yang sama. Gunakan metode nilai untuk satuan berbeda.");
  const reason = draft.reason.trim();
  if (reason.length < 3 || reason.length > 1000 || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(reason)) throw new Error("Alasan harus berisi 3–1.000 karakter tanpa karakter kontrol.");
  const reference = draft.reference.trim();
  if (reference.length > 200 || /[\u0000-\u001f\u007f]/.test(reference)) throw new Error("Referensi maksimal 200 karakter tanpa karakter kontrol.");
  return { source_line_id: draft.source_line_id, amount: draft.amount, allocation_method: draft.allocation_method, receipt_line_ids: [...draft.receipt_line_ids].sort(), reason, ...(reference ? { reference } : {}) };
}
export function retainedPercent(value: string): string {
  const match = /^(\d+)(?:\.(\d+))?$/.exec(value);
  if (!match) return "—";
  const scale = 10n ** BigInt(match[2]?.length ?? 0);
  const numerator = BigInt(match[1]) * scale + BigInt(match[2] || "0");
  const hundredths = (numerator * 10_000n + scale / 2n) / scale;
  return `${hundredths / 100n}${hundredths % 100n ? `,${(hundredths % 100n).toString().padStart(2, "0")}` : ""}%`;
}
