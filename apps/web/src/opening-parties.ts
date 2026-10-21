import { companyToday } from "./format";
import { moneyCents } from "./dashboard-charts";
export type OpeningPartyKind =
  "receivable" | "payable" | "customer_credit" | "supplier_credit";
export interface OpeningParty {
  kind: OpeningPartyKind;
  party_id: string;
  party_name?: string;
  reference: string;
  document_date: string;
  due_date?: string | null;
  amount: string;
  id?: string;
}
export interface OpeningPartyCsvPreview {
  parties: OpeningParty[];
  errors: { row: number; column: string; message: string }[];
  valid_count: number;
}
export function openingPartyInput(row: OpeningParty) {
  return {
    kind: row.kind,
    party_id: row.party_id,
    reference: row.reference.trim(),
    document_date: row.document_date,
    due_date: row.due_date || null,
    amount: row.amount,
  };
}
export const openingPartyNames: Record<OpeningPartyKind, string> = {
  receivable: "Piutang pelanggan",
  payable: "Utang supplier",
  customer_credit: "Kredit/uang muka pelanggan",
  supplier_credit: "Kredit/uang muka supplier",
};
export function openingPartySide(kind: OpeningPartyKind) {
  return kind === "receivable" || kind === "supplier_credit"
    ? "debit"
    : "credit";
}
export function openingPartyCustomer(kind: OpeningPartyKind) {
  return kind === "receivable" || kind === "customer_credit";
}
export function validOpeningDate(value: string) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || value.slice(0, 4) === "0000")
    return false;
  const date = new Date(`${value}T12:00:00Z`);
  return (
    Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value
  );
}
function duplicateKey(row: OpeningParty) {
  return `${row.kind}:${row.party_id}:${row.reference.trim().toLocaleLowerCase("id-ID")}`;
}
export function openingPartyPreview(
  rows: OpeningParty[],
  today = companyToday(),
) {
  let debit = 0n,
    credit = 0n;
  const errors: string[] = [],
    keys = new Set<string>();
  if (rows.length > 1000)
    errors.push("Maksimal 1.000 rincian saldo pihak per pembukaan.");
  rows.forEach((row, index) => {
    const prefix = `Baris pihak ${index + 1}: `;
    if (!(row.kind in openingPartyNames)) {
      errors.push(prefix + "pilih jenis saldo.");
      return;
    }
    if (!row.party_id) errors.push(prefix + "pilih pelanggan atau supplier.");
    if (!row.reference.trim() || row.reference.length > 200)
      errors.push(prefix + "isi referensi 1–200 karakter.");
    if (!validOpeningDate(row.document_date) || row.document_date > today)
      errors.push(
        prefix + "tanggal dokumen harus valid dan tidak melewati hari ini.",
      );
    if (
      row.due_date &&
      (!validOpeningDate(row.due_date) || row.due_date < row.document_date)
    )
      errors.push(
        prefix +
          "jatuh tempo harus valid dan tidak mendahului tanggal dokumen.",
      );
    const key = duplicateKey(row);
    if (keys.has(key))
      errors.push(prefix + "pihak, jenis, dan referensi sudah digunakan.");
    keys.add(key);
    try {
      const amount = moneyCents(row.amount);
      if (amount <= 0n || amount > 100000000000000n) throw new Error();
      if (openingPartySide(row.kind) === "debit") debit += amount;
      else credit += amount;
    } catch {
      errors.push(
        prefix + "nominal positif maksimal Rp1 triliun dengan dua desimal.",
      );
    }
  });
  return { debit, credit, errors };
}
export function mergeOpeningParties(
  existing: OpeningParty[],
  preview: OpeningPartyCsvPreview,
) {
  if (preview.errors.length || !preview.parties.length)
    throw new Error(
      "Perbaiki semua kesalahan CSV sebelum menambahkan rincian.",
    );
  const next = [...existing, ...preview.parties];
  const validation = openingPartyPreview(next);
  if (validation.errors.length) throw new Error(validation.errors.join(" "));
  return next;
}
