import type { Role } from "./types";
export type DocumentKind =
  | "sales_order"
  | "invoice"
  | "payment"
  | "purchase_order"
  | "purchase_receipt"
  | "purchase_bill"
  | "supplier_payment"
  | "journal"
  | "sales_return"
  | "supplier_return"
  | "party_fund"
  | "party_refund"
  | "opening_item";
export const attachmentLimits = {
  max_file_bytes: 5242880,
  max_files: 20,
  max_document_bytes: 52428800,
};
export interface Attachment {
  id: string;
  document_type: DocumentKind;
  document_id: string;
  filename: string;
  media_type: string;
  byte_size: number;
  sha256: string;
  version: number;
  created_at: string;
  created_by_name: string;
  archived_at: string | null;
  archived_by_name: string | null;
  archive_reason: string | null;
  history: {
    action: string;
    reason: string | null;
    version: number;
    created_at: string;
    created_by_name: string;
  }[];
}
export interface AttachmentList {
  items: Attachment[];
  limits: typeof attachmentLimits;
  active_count: number;
  active_bytes: number;
}
export function canManageAttachments(role: Role, kind: DocumentKind) {
  return (
    role === "admin" ||
    role ===
      (kind === "sales_order"
        ? "sales"
        : kind === "purchase_order" || kind === "purchase_receipt"
          ? "warehouse"
          : "finance")
  );
}
export function attachmentMime(filename: string) {
  return /\.pdf$/i.test(filename)
    ? "application/pdf"
    : /\.png$/i.test(filename)
      ? "image/png"
      : /\.jpe?g$/i.test(filename)
        ? "image/jpeg"
        : "";
}
export function attachmentFileError(
  file: { name: string; size: number; type: string },
  activeCount: number,
  activeBytes: number,
  limits = attachmentLimits,
) {
  const mime = attachmentMime(file.name);
  if (!mime || (file.type && file.type !== mime))
    return "Pilih file PDF, JPEG, atau PNG dengan ekstensi dan jenis file yang sesuai.";
  if (!file.size) return "File masih kosong.";
  if (file.size > limits.max_file_bytes)
    return "Ukuran setiap lampiran maksimal 5 MiB.";
  if (activeCount >= limits.max_files)
    return "Maksimal 20 lampiran aktif per dokumen. Arsipkan lampiran yang tidak diperlukan.";
  if (activeBytes + file.size > limits.max_document_bytes)
    return "Total lampiran aktif dokumen maksimal 50 MiB.";
  return "";
}
export function fileSize(bytes: number) {
  return bytes >= 1048576
    ? `${(bytes / 1048576).toLocaleString("id-ID", { maximumFractionDigits: 2 })} MiB`
    : bytes >= 1024
      ? `${(bytes / 1024).toLocaleString("id-ID", { maximumFractionDigits: 1 })} KiB`
      : `${bytes} byte`;
}

export function attachmentActionLabel(action: string) {
  return action === "uploaded"
    ? "Diunggah"
    : action === "archived"
      ? "Diarsipkan"
      : action === "restored"
        ? "Dipulihkan"
        : action;
}
