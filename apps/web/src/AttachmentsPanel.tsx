import { useContext, useRef, useState, type FormEvent } from "react";
import { Archive, Download, Paperclip, RotateCcw, Upload } from "lucide-react";
import { api, ApiError, errorMessage } from "./api";
import { ErrorState, FormError, FormField, Loading, Modal } from "./components";
import { SessionRoleContext, useResource } from "./hooks";
import { dateTime } from "./format";
import { downloadFile, fileRequest } from "./file-api";
import { requestIdentity, type RequestIdentity } from "./purchasing-model";
import {
  attachmentFileError,
  attachmentActionLabel,
  attachmentLimits,
  attachmentMime,
  canManageAttachments,
  fileSize,
  type Attachment,
  type AttachmentList,
  type DocumentKind,
} from "./attachments-model";
import "./data-files.css";

export function AttachmentsPanel({
  documentType,
  documentId,
  title = "Lampiran bukti",
}: {
  documentType: DocumentKind;
  documentId: string;
  title?: string;
}) {
  const [opened, setOpened] = useState(false);
  return (
    <details
      className="attachments-panel"
      onToggle={(event) => {
        if (event.currentTarget.open) setOpened(true);
      }}
    >
      <summary>
        <Paperclip size={16} /> {title}
      </summary>
      {opened && (
        <AttachmentContent
          key={`${documentType}:${documentId}`}
          documentType={documentType}
          documentId={documentId}
        />
      )}
    </details>
  );
}

function AttachmentContent({
  documentType,
  documentId,
}: {
  documentType: DocumentKind;
  documentId: string;
}) {
  const role = useContext(SessionRoleContext);
  const canManage = canManageAttachments(role, documentType);
  const path = `/documents/${documentType}/${documentId}/attachments`;
  const resource = useResource<AttachmentList>(path);
  const [file, setFile] = useState<File | null>(null);
  const [fileSelection, setFileSelection] = useState("");
  const [inputVersion, setInputVersion] = useState(0);
  const [selection, setSelection] = useState<{
    item: Attachment;
    action: "archive" | "restore";
  } | null>(null);
  const [reason, setReason] = useState("");
  const [review, setReview] = useState(false);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [showArchived, setShowArchived] = useState(false);
  const limits = resource.data?.limits ?? attachmentLimits;
  const issue = file
    ? attachmentFileError(
        file,
        resource.data?.active_count ?? 0,
        resource.data?.active_bytes ?? 0,
        limits,
      )
    : "";
  async function work(action: () => Promise<void>) {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await action();
    } catch (e) {
      setError(errorMessage(e));
      if (e instanceof ApiError && e.status === 409) resource.reload();
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  async function upload(event: FormEvent) {
    event.preventDefault();
    if (!file || issue || !resource.data) return;
    await work(async () => {
      identity.current = requestIdentity(identity.current, {
        path,
        file_selection: fileSelection,
      });
      await fileRequest(
        `${path}?filename=${encodeURIComponent(file.name)}&idempotency_key=${identity.current.key}`,
        {
          method: "POST",
          headers: { "Content-Type": attachmentMime(file.name) },
          body: file,
        },
      );
      setFile(null);
      setFileSelection("");
      setInputVersion((value) => value + 1);
      resource.reload();
      setNotice("Lampiran tersimpan.");
    });
  }
  async function change(event: FormEvent) {
    event.preventDefault();
    if (!selection) return;
    if (!review) {
      setReview(true);
      return;
    }
    await work(async () => {
      const request = {
        path: `/attachments/${selection.item.id}/${selection.action}`,
        version: selection.item.version,
        reason: reason.trim(),
      };
      identity.current = requestIdentity(identity.current, request);
      await api(request.path, {
        method: "POST",
        body: {
          version: request.version,
          reason: request.reason,
          idempotency_key: identity.current.key,
        },
      });
      setSelection(null);
      resource.reload();
      setNotice(
        selection.action === "archive"
          ? "Lampiran diarsipkan. File dan riwayat tetap tersimpan."
          : "Lampiran dipulihkan.",
      );
    });
  }
  const visible =
    resource.data?.items.filter((item) => showArchived || !item.archived_at) ??
    [];
  return (
    <div className="attachments-content">
      <p className="muted">
        PDF, JPEG, atau PNG · maksimal 5 MiB per file · 20 lampiran aktif / 50
        MiB per dokumen. Unduhan menyimpan file asli.
      </p>
      {resource.loading ? (
        <Loading />
      ) : resource.error ? (
        <ErrorState message={resource.error} onRetry={resource.reload} />
      ) : (
        resource.data && (
          <>
            <div className="data-file-section-head">
              <span>
                {resource.data.active_count} aktif ·{" "}
                {fileSize(resource.data.active_bytes)}
              </span>
              {resource.data.items.some((item) => item.archived_at) && (
                <label className="checkbox-field">
                  <input
                    type="checkbox"
                    checked={showArchived}
                    onChange={(e) => setShowArchived(e.target.checked)}
                  />{" "}
                  Tampilkan arsip
                </label>
              )}
            </div>
            {visible.length ? (
              <div className="attachment-list">
                {visible.map((item) => (
                  <article
                    className={`attachment-item${item.archived_at ? " archived" : ""}`}
                    key={item.id}
                  >
                    <div>
                      <strong className="attachment-filename">
                        {item.filename}
                      </strong>
                      <small>
                        {fileSize(item.byte_size)} · {dateTime(item.created_at)}{" "}
                        · {item.created_by_name}
                      </small>
                      {item.archived_at && (
                        <p>
                          Diarsipkan {dateTime(item.archived_at)} oleh{" "}
                          {item.archived_by_name}: {item.archive_reason}
                        </p>
                      )}
                    </div>
                    <div className="data-file-actions">
                      <button
                        type="button"
                        className="text-button"
                        disabled={busy}
                        onClick={() =>
                          void work(async () => {
                            await downloadFile(
                              `/attachments/${item.id}/download`,
                              item.filename,
                            );
                          })
                        }
                      >
                        <Download size={14} /> Unduh
                      </button>
                      {canManage && (
                        <button
                          type="button"
                          className="text-button"
                          disabled={busy}
                          onClick={() => {
                            setSelection({
                              item,
                              action: item.archived_at ? "restore" : "archive",
                            });
                            setReason("");
                            setReview(false);
                            setError("");
                          }}
                        >
                          {item.archived_at ? (
                            <RotateCcw size={14} />
                          ) : (
                            <Archive size={14} />
                          )}
                          {item.archived_at ? "Pulihkan" : "Arsipkan"}
                        </button>
                      )}
                    </div>
                    <details className="attachment-history">
                      <summary>Riwayat dan pemeriksaan file</summary>
                      <p className="attachment-hash">SHA-256: {item.sha256}</p>
                      <ul>
                        {item.history.map((entry, i) => (
                          <li key={i}>
                            {dateTime(entry.created_at)} ·{" "}
                            {entry.created_by_name} ·{" "}
                            {attachmentActionLabel(entry.action)}
                            {entry.reason && `: ${entry.reason}`}
                          </li>
                        ))}
                      </ul>
                    </details>
                  </article>
                ))}
              </div>
            ) : (
              <p className="muted">Belum ada lampiran aktif.</p>
            )}
            {canManage && (
              <form onSubmit={upload} className="attachment-upload">
                <FormField label="Tambah bukti transaksi">
                  <input
                    key={inputVersion}
                    type="file"
                    accept=".pdf,.jpg,.jpeg,.png,application/pdf,image/jpeg,image/png"
                    disabled={busy}
                    onChange={(e) => {
                      setFile(e.target.files?.[0] ?? null);
                      setFileSelection(crypto.randomUUID());
                      setError("");
                    }}
                  />
                </FormField>
                {file && (
                  <>
                    <p className="attachment-filename">
                      {file.name} · {fileSize(file.size)} ·{" "}
                      {attachmentMime(file.name) || "Jenis tidak didukung"}
                    </p>
                    <FormError error={issue} />
                    <button
                      className="button secondary"
                      disabled={busy || !!issue}
                    >
                      <Upload size={15} />{" "}
                      {busy ? "Mengunggah…" : "Unggah lampiran"}
                    </button>
                  </>
                )}
              </form>
            )}
          </>
        )
      )}
      <FormError error={error} />
      {notice && (
        <p role="status" className="data-file-success">
          {notice}
        </p>
      )}
      {selection && (
        <Modal
          title={
            selection.action === "archive"
              ? "Arsipkan lampiran"
              : "Pulihkan lampiran"
          }
          description={selection.item.filename}
          onClose={() => {
            if (!busy) setSelection(null);
          }}
        >
          <form onSubmit={change}>
            <div className="form-body">
              <p>
                {selection.action === "archive"
                  ? "Lampiran disembunyikan dari daftar aktif. File dan riwayatnya tetap dapat diunduh dan dipulihkan."
                  : "Lampiran kembali muncul pada daftar aktif, mengikuti batas jumlah dan ukuran dokumen."}
              </p>
              {review ? (
                <div className="info-box">
                  <Archive size={18} />
                  <p>
                    Periksa {selection.item.filename} ·{" "}
                    {fileSize(selection.item.byte_size)}.<br />
                    Alasan: {reason}
                  </p>
                </div>
              ) : (
                <FormField label="Alasan *">
                  <textarea
                    required
                    minLength={3}
                    maxLength={1000}
                    rows={3}
                    value={reason}
                    disabled={busy}
                    onChange={(e) => setReason(e.target.value)}
                    autoFocus
                  />
                </FormField>
              )}
              <FormError error={error} />
            </div>
            <div className="form-actions">
              <button
                type="button"
                className="button secondary"
                disabled={busy}
                onClick={() => (review ? setReview(false) : setSelection(null))}
              >
                {review ? "Ubah alasan" : "Batal"}
              </button>
              <button
                className="button primary"
                disabled={busy || reason.trim().length < 3}
              >
                {busy
                  ? "Memproses…"
                  : !review
                    ? "Tinjau perubahan"
                    : selection.action === "archive"
                      ? "Arsipkan lampiran"
                      : "Pulihkan lampiran"}
              </button>
            </div>
          </form>
        </Modal>
      )}
    </div>
  );
}
