import { lineTotal } from "./format";
import { isControlAccount } from "./account-controls";
import type { Account, Product } from "./types";

export interface OpeningItem {
  product_id: string;
  sku: string;
  name: string;
  unit: string;
  quantity: string;
  unit_cost: string;
  value: string;
}
export interface OpeningLine {
  account_code: string;
  account_name: string;
  debit: string;
  credit: string;
}
export interface OpeningSnapshot {
  description: string;
  items: OpeningItem[];
  lines: OpeningLine[];
  inventory_total: string;
  total_debit: string;
  total_credit: string;
}
export interface OpeningHistory extends OpeningSnapshot {
  id: string;
  journal_id: string;
  posted_at: string;
  reversed_at: string | null;
  reason: string | null;
}
export interface OpeningDocument extends OpeningSnapshot {
  id: string;
  status: "draft" | "posted";
  version: number;
  created_at: string;
  posted_at: string | null;
  balanced: boolean;
  history: OpeningHistory[];
}
export interface OpeningState {
  eligible: boolean;
  blocked_reason: string | null;
  can_reverse: boolean;
  document: OpeningDocument | null;
}
export interface OpeningDraft {
  description: string;
  items: { product_id: string; quantity: string; unit_cost: string }[];
  lines: { account_code: string; debit: string; credit: string }[];
}

export const openingAccounts = (accounts: Account[]) =>
  accounts.filter(
    (account) =>
      ["asset", "liability", "equity"].includes(account.kind) &&
      !isControlAccount(account),
  );
export const openingProducts = (products: Product[]) =>
  products.filter((product) => product.kind !== "service");

export function openingDraft(document: OpeningDocument | null): OpeningDraft {
  return {
    description: document?.description ?? "Saldo awal usaha",
    items: (document?.items ?? []).map(
      ({ product_id, quantity, unit_cost }) => ({
        product_id,
        quantity,
        unit_cost,
      }),
    ),
    // The server returns the generated inventory control line as well.
    lines: (document?.lines ?? [])
      .filter((line) => line.account_code !== "1300")
      .map(({ account_code, debit, credit }) => ({
        account_code,
        debit,
        credit,
      })),
  };
}

function decimal(value: string, places: number): bigint | null {
  value = normalizedDecimal(value);
  if (!new RegExp(`^\\d+(\\.\\d{1,${places}})?$`).test(value)) return null;
  const [whole, fraction = ""] = value.split(".");
  return (
    BigInt(whole) * 10n ** BigInt(places) + BigInt(fraction.padEnd(places, "0"))
  );
}
function normalizedDecimal(value: string): string {
  return value.includes(".")
    ? value.replace(/0+$/, "").replace(/\.$/, "")
    : value;
}
function cents(value: string) {
  return decimal(value || "0", 2);
}
export function openingItemValue(quantity: string, cost: string): string {
  return lineTotal(normalizedDecimal(quantity), normalizedDecimal(cost));
}
export function openingDifference(debit: string, credit: string): string {
  return openingMoney((cents(debit) ?? 0n) - (cents(credit) ?? 0n));
}
export function openingIsBalanced(debit: string, credit: string): boolean {
  const dr = cents(debit);
  const cr = cents(credit);
  return dr !== null && dr > 0n && dr === cr;
}
export function openingMoney(value: bigint): string {
  const absolute = value < 0n ? -value : value;
  return `${value < 0n ? "-" : ""}${absolute / 100n}.${(absolute % 100n).toString().padStart(2, "0")}`;
}

export function openingPreview(draft: OpeningDraft, accounts: Account[] = []) {
  let inventory = 0n;
  let debit = 0n;
  let credit = 0n;
  let valid = true;
  const productIds = new Set<string>();
  const accountCodes = new Set<string>();
  const controlled = new Set([
    "1200",
    "1300",
    ...accounts.filter(isControlAccount).map((account) => account.code),
  ]);
  for (const item of draft.items) {
    const quantity = decimal(item.quantity, 3);
    const cost = decimal(item.unit_cost, 4);
    const value = cents(openingItemValue(item.quantity, item.unit_cost)) ?? 0n;
    if (
      !item.product_id ||
      productIds.has(item.product_id) ||
      quantity === null ||
      quantity <= 0n ||
      quantity > 1_000_000_000_000n ||
      cost === null ||
      cost <= 0n ||
      cost > 10_000_000_000_000n ||
      value <= 0n ||
      value > 100_000_000_000_000n
    )
      valid = false;
    productIds.add(item.product_id);
    inventory += value;
  }
  debit += inventory;
  for (const line of draft.lines) {
    const dr = cents(line.debit);
    const cr = cents(line.credit);
    if (
      !line.account_code ||
      controlled.has(line.account_code) ||
      accountCodes.has(line.account_code) ||
      dr === null ||
      cr === null ||
      (dr === 0n) === (cr === 0n) ||
      (dr ?? 0n) > 100_000_000_000_000n ||
      (cr ?? 0n) > 100_000_000_000_000n
    )
      valid = false;
    accountCodes.add(line.account_code);
    debit += dr ?? 0n;
    credit += cr ?? 0n;
  }
  if (
    draft.items.length > 100 ||
    draft.lines.length > 100 ||
    debit > 100_000_000_000_000n ||
    credit > 100_000_000_000_000n
  )
    valid = false;
  return {
    inventory: openingMoney(inventory),
    debit: openingMoney(debit),
    credit: openingMoney(credit),
    difference: openingMoney(debit - credit),
    balanced: valid && debit > 0n && debit === credit,
    valid,
  };
}
