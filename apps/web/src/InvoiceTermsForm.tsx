import { useRef, useState, type FormEvent } from "react";
import { api, errorMessage } from "./api";
import { FormError, FormField, Modal } from "./components";
import { companyToday } from "./format";
import { validOpeningDate } from "./opening-parties";
import { requestIdentity, type RequestIdentity } from "./purchasing-model";
import type { Customer } from "./types";

export function InvoiceTermsForm({
  invoice,
  onClose,
  onSaved,
}: {
  invoice: {
    id: string;
    number: string;
    version: number;
    invoice_date: string | null;
    due_date: string | null;
  };
  onClose: () => void;
  onSaved: () => void;
}) {
  const [invoiceDate, setInvoiceDate] = useState(invoice.invoice_date ?? "");
  const [dueDate, setDueDate] = useState(invoice.due_date ?? "");
  const [reason, setReason] = useState("");
  const [review, setReview] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busyRef.current) return;
    if (
      (invoiceDate &&
        (!validOpeningDate(invoiceDate) || invoiceDate > companyToday())) ||
      (dueDate &&
        (!validOpeningDate(dueDate) ||
          (invoiceDate && dueDate < invoiceDate))) ||
      reason.trim().length < 3
    ) {
      setError(
        "Periksa tanggal dokumen, jatuh tempo yang tidak mendahului dokumen, dan alasan minimal tiga karakter.",
      );
      return;
    }
    if (!review) {
      setReview(true);
      setError("");
      return;
    }
    const payload = {
      version: invoice.version,
      invoice_date: invoiceDate || null,
      due_date: dueDate || null,
      reason: reason.trim(),
    };
    identity.current = requestIdentity(identity.current, {
      invoice_id: invoice.id,
      ...payload,
    });
    busyRef.current = true;
    setBusy(true);
    setError("");
    try {
      await api(`/invoices/${invoice.id}/terms`, {
        method: "PATCH",
        body: { ...payload, idempotency_key: identity.current.key },
      });
      onSaved();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  return (
    <Modal
      title="Tanggal dan jatuh tempo invoice"
      description={invoice.number}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <p>
            Tanggal ini dipakai pada aging. Dokumen lama yang tanggalnya belum
            diketahui dapat tetap kosong. Tanggal dokumen yang sudah diketahui
            dipertahankan; jatuh tempo boleh dikosongkan bila belum diketahui.
            Perubahan dicatat dalam riwayat; tanggal jurnal tidak dipindahkan.
          </p>
          <FormField label="Tanggal dokumen">
            <input
              type="date"
              required={!!invoice.invoice_date}
              value={invoiceDate}
              min="0001-01-01"
              max={companyToday()}
              disabled={busy || review}
              onChange={(e) => setInvoiceDate(e.target.value)}
            />
          </FormField>
          <FormField label="Jatuh tempo">
            <input
              type="date"
              value={dueDate}
              min={invoiceDate || "0001-01-01"}
              max="9999-12-31"
              disabled={busy || review}
              onChange={(e) => setDueDate(e.target.value)}
            />
          </FormField>
          <FormField label="Alasan perubahan *">
            <textarea
              required
              minLength={3}
              maxLength={1000}
              rows={3}
              value={reason}
              disabled={busy || review}
              onChange={(e) => setReason(e.target.value)}
            />
          </FormField>
          {review && (
            <p className="info-box">
              Tanggal dokumen: {invoiceDate || "Belum diketahui"}. Jatuh tempo:{" "}
              {dueDate || "Belum diketahui"}. Alasan: {reason}
            </p>
          )}
          <FormError error={error} />
        </div>
        <div className="form-actions">
          <button
            type="button"
            className="button secondary"
            disabled={busy}
            onClick={() => (review ? setReview(false) : onClose())}
          >
            {review ? "Ubah rincian" : "Batal"}
          </button>
          <button className="button primary" disabled={busy}>
            {busy
              ? "Menyimpan…"
              : review
                ? "Simpan tanggal invoice"
                : "Tinjau perubahan"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function CustomerTermsForm({
  customer,
  onClose,
  onSaved,
}: {
  customer: Customer;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [days, setDays] = useState(String(customer.payment_terms_days ?? 0));
  const [reason, setReason] = useState("");
  const [review, setReview] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busyRef.current) return;
    if (
      !/^\d+$/.test(days) ||
      Number(days) > 3650 ||
      reason.trim().length < 3
    ) {
      setError("Isi 0–3.650 hari dan alasan minimal tiga karakter.");
      return;
    }
    if (!review) {
      setReview(true);
      setError("");
      return;
    }
    const payload = {
      payment_terms_days: Number(days),
      previous_payment_terms_days: customer.payment_terms_days ?? 0,
      reason: reason.trim(),
    };
    identity.current = requestIdentity(identity.current, {
      customer_id: customer.id,
      ...payload,
    });
    busyRef.current = true;
    setBusy(true);
    setError("");
    try {
      await api(`/customers/${customer.id}/terms`, {
        method: "PATCH",
        body: { ...payload, idempotency_key: identity.current.key },
      });
      onSaved();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  return (
    <Modal
      title="Syarat pembayaran pelanggan"
      description={customer.name}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <p>
            Jumlah hari ini menentukan jatuh tempo invoice baru setelah tanggal
            dokumennya. Jatuh tempo invoice yang sudah ada tetap tersimpan dan
            dapat diubah dari rincian invoice.
          </p>
          <FormField label="Jatuh tempo setelah tanggal invoice (hari) *">
            <input
              required
              inputMode="numeric"
              value={days}
              disabled={busy || review}
              onChange={(e) => setDays(e.target.value)}
            />
          </FormField>
          <FormField label="Alasan perubahan *">
            <textarea
              required
              minLength={3}
              maxLength={1000}
              value={reason}
              disabled={busy || review}
              onChange={(e) => setReason(e.target.value)}
              rows={3}
            />
          </FormField>
          {review && (
            <p className="info-box">
              Ubah {customer.payment_terms_days ?? 0} menjadi {Number(days)}{" "}
              hari untuk invoice baru. Alasan: {reason}
            </p>
          )}
          <FormError error={error} />
        </div>
        <div className="form-actions">
          <button
            type="button"
            className="button secondary"
            disabled={busy}
            onClick={() => (review ? setReview(false) : onClose())}
          >
            {review ? "Ubah rincian" : "Batal"}
          </button>
          <button className="button primary" disabled={busy}>
            {busy
              ? "Menyimpan…"
              : review
                ? "Simpan syarat pembayaran"
                : "Tinjau perubahan"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
