import type { Role } from "./types";

export type ImportKind = "customers" | "products" | "suppliers";
export const importNames: Record<ImportKind, string> = {
  customers: "Pelanggan",
  products: "Produk",
  suppliers: "Supplier",
};
export const importColumns: Record<ImportKind, string[]> = {
  customers: ["name", "email", "phone", "address"],
  products: ["sku", "name", "price", "kind", "unit"],
  suppliers: ["code", "name", "email", "phone", "address"],
};
export const CSV_MAX_BYTES = 262144;
export function importKinds(role: Role): ImportKind[] {
  return role === "admin"
    ? ["customers", "products", "suppliers"]
    : role === "sales"
      ? ["customers"]
      : role === "warehouse"
        ? ["products", "suppliers"]
        : [];
}
export function csvFileError(file: { name: string; size: number }) {
  if (!/\.csv$/i.test(file.name)) return "Pilih file dengan ekstensi .csv.";
  if (!file.size) return "File CSV masih kosong.";
  if (file.size > CSV_MAX_BYTES) return "Ukuran CSV maksimal 256 KiB.";
  return "";
}
export function decodeCsv(bytes: ArrayBuffer) {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error(
      "File harus memakai UTF-8. Simpan sebagai CSV UTF-8 dari aplikasi spreadsheet Anda.",
    );
  }
}
export interface ImportIssue {
  row: number;
  column: string;
  message: string;
}
export interface ImportBatch {
  id: string;
  kind: ImportKind;
  filename: string;
  created_at: string;
  status: "ready" | "invalid" | "committed";
  row_count: number;
  valid_count: number;
  errors: ImportIssue[];
  rows: {
    row: number;
    data: Record<string, string | null>;
    errors: ImportIssue[];
  }[];
  can_commit: boolean;
  committed_at?: string | null;
  created_count?: number | null;
  creator?: { id: string; name: string; email: string };
}
export function importStatus(status: ImportBatch["status"]) {
  return status === "committed"
    ? "Tersimpan"
    : status === "ready"
      ? "Siap ditinjau"
      : "Perlu diperbaiki";
}
