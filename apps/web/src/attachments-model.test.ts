import { describe, expect, test } from "bun:test";
import {
  attachmentFileError,
  attachmentActionLabel,
  attachmentLimits,
  attachmentMime,
  canManageAttachments,
} from "./attachments-model";
import { requestIdentity } from "./purchasing-model";
describe("document attachments", () => {
  test("renders actual backend audit action names in Indonesian", () => {
    expect(attachmentActionLabel("uploaded")).toBe("Diunggah");
    expect(attachmentActionLabel("archived")).toBe("Diarsipkan");
    expect(attachmentActionLabel("restored")).toBe("Dipulihkan");
    expect(attachmentActionLabel("future_action")).toBe("future_action");
  });
  test("enforces file, count, and cumulative byte limits at boundaries", () => {
    const file = {
      name: "nota.PDF",
      type: "application/pdf",
      size: attachmentLimits.max_file_bytes,
    };
    expect(
      attachmentFileError(
        file,
        19,
        attachmentLimits.max_document_bytes - file.size,
      ),
    ).toBe("");
    expect(
      attachmentFileError({ ...file, size: file.size + 1 }, 0, 0),
    ).toContain("5 MiB");
    expect(attachmentFileError(file, 20, 0)).toContain("20");
    expect(
      attachmentFileError(
        file,
        19,
        attachmentLimits.max_document_bytes - file.size + 1,
      ),
    ).toContain("50 MiB");
    expect(attachmentFileError({ ...file, size: 0 }, 0, 0)).toContain("kosong");
  });
  test("rejects misleading extensions and MIME mismatch", () => {
    expect(
      attachmentFileError(
        { name: "nota.html", type: "text/html", size: 30 },
        0,
        0,
      ),
    ).not.toBe("");
    expect(
      attachmentFileError(
        { name: "nota.pdf", type: "text/html", size: 30 },
        0,
        0,
      ),
    ).not.toBe("");
    expect(
      attachmentFileError({ name: "foto.JPEG", type: "", size: 30 }, 0, 0),
    ).toBe("");
    expect(attachmentMime("foto.JPG")).toBe("image/jpeg");
  });
  test("uses document ownership role rather than financial edit permissions", () => {
    expect(canManageAttachments("sales", "sales_order")).toBe(true);
    expect(canManageAttachments("sales", "invoice")).toBe(false);
    expect(canManageAttachments("warehouse", "purchase_receipt")).toBe(true);
    expect(canManageAttachments("warehouse", "purchase_bill")).toBe(false);
    expect(canManageAttachments("finance", "supplier_payment")).toBe(true);
    expect(canManageAttachments("viewer", "journal")).toBe(false);
    expect(canManageAttachments("admin", "sales_order")).toBe(true);
  });
  test("retries preserve keys and changed file selections or revisions get new keys", () => {
    const first = requestIdentity(null, {
      document: "a",
      file_selection: "first",
    });
    expect(
      requestIdentity(first, { document: "a", file_selection: "first" }).key,
    ).toBe(first.key);
    expect(
      requestIdentity(first, { document: "a", file_selection: "second" }).key,
    ).not.toBe(first.key);
    expect(
      requestIdentity(first, { document: "a", version: 2, reason: "arsip" })
        .key,
    ).not.toBe(first.key);
  });
});
