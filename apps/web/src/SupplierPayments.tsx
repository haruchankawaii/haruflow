import { useRef, useState, type FormEvent } from "react";
import { api, errorMessage } from "./api";
import { FormError, FormField } from "./components";
import { exactMoney, moneyCents } from "./dashboard-charts";
import { dateTime } from "./format";
import { requestIdentity, type RequestIdentity } from "./purchasing-model";
import type { SupplierBill, SupplierPayment } from "./bills-model";
import { CashAccountSelect } from "./CashAccountSelect";
import { AttachmentsPanel } from "./AttachmentsPanel";

export function SupplierPayments({
  bill,
  canManage,
  onSaved,
  onJournal,
}: {
  bill: SupplierBill;
  canManage: boolean;
  onSaved: () => void;
  onJournal: (id: string) => void;
}) {
  const [amount, setAmount] = useState("");
  const [reference, setReference] = useState("");
  const [cashAccount, setCashAccount] = useState("1100");
  const [review, setReview] = useState(false);
  const [selected, setSelected] = useState<SupplierPayment | null>(null);
  const [reason, setReason] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busyRef.current) return;
    if (
      !selected &&
      (!/^\d+(\.\d{1,2})?$/.test(amount) ||
        moneyCents(amount) <= 0n ||
        moneyCents(amount) > moneyCents(bill.outstanding))
    ) {
      setError(
        "Isi pembayaran positif, maksimal sisa utang, dengan dua desimal.",
      );
      return;
    }
    if (selected && reason.trim().length < 3) {
      setError("Isi alasan pembalikan minimal tiga karakter.");
      return;
    }
    if (!review) { setReview(true); setError(""); return; }
    const payload = selected
      ? { version: bill.version, reason: reason.trim() }
      : { version: bill.version, amount, reference: reference.trim() || null, cash_account_code: cashAccount };
    const path = selected
      ? `/supplier-payments/${selected.id}/reverse`
      : `/purchase-bills/${bill.id}/payments`;
    identity.current = requestIdentity(identity.current, { path, ...payload });
    busyRef.current = true;
    setBusy(true);
    setError("");
    try {
      await api(path, {
        method: "POST",
        body: { ...payload, idempotency_key: identity.current.key },
      });
      setReview(false);
      setAmount("");
      setReference("");
      setSelected(null);
      setReason("");
      onSaved();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  return (
    <section className="supplier-payment-panel">
      <h3>Pembayaran supplier</h3>
      <p className="muted">
        Pembayaran mengurangi kas dan utang secara bersamaan. Dokumen serta
        koreksi tetap tersimpan.
      </p>
      {bill.payments?.length ? (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>Pembayaran</th>
                <th>Referensi</th>
                <th className="align-right">Nominal</th>
                <th>Riwayat</th>
              </tr>
            </thead>
            <tbody>
              {bill.payments.map((payment) => (
                <tr key={payment.id}>
                  <td>
                    <strong>{payment.number}</strong>
                    <small className="bill-cell-sub">
                      {dateTime(payment.created_at)}
                    </small>
                  </td>
                  <td>{payment.reference || "—"}</td>
                  <td className="align-right">{exactMoney(payment.amount)}</td>
                  <td>
                    <button
                      type="button"
                      className="text-button"
                      onClick={() => onJournal(payment.journal_id)}
                    >
                      Jurnal
                    </button>
                    {payment.reversal ? (
                      <>
                        <p className="purchase-prewrap">
                          Dibalik: {payment.reversal.reason}
                        </p>
                        <button
                          type="button"
                          className="text-button"
                          onClick={() =>
                            onJournal(payment.reversal!.journal_id)
                          }
                        >
                          Jurnal pembalikan
                        </button>
                      </>
                    ) : (
                      canManage &&
                      payment.can_reverse && (
                        <button
                          type="button"
                          className="text-button danger-text"
                          disabled={busy || review}
                          onClick={() => {
                            setReview(false);
                            setSelected(payment);
                            setError("");
                          }}
                        >
                          Koreksi pembayaran
                        </button>
                      )
                    )}
                    <AttachmentsPanel
                      documentType="supplier_payment"
                      documentId={payment.id}
                      title="Bukti pembayaran"
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="muted">Belum ada pembayaran.</p>
      )}
      {canManage && (selected || bill.can_pay) && (
        <form className="supplier-payment-form" onSubmit={submit}>
          <h4>
            {selected ? `Balikkan ${selected.number}` : "Catat pembayaran"}
          </h4>
          {selected ? (
            <>
              <p>
                Kas bertambah dan utang kembali bertambah sebesar{" "}
                {exactMoney(selected.amount)}. Gunakan untuk mengoreksi
                pencatatan pembayaran. Dana kembali ke rekening pada jurnal asal.
              </p>
              <FormField label="Alasan pembalikan *">
                <textarea
                  required
                  minLength={3}
                  maxLength={1000}
                  value={reason}
                  onChange={(e) => setReason(e.target.value)}
                  disabled={busy || review}
                />
              </FormField>
            </>
          ) : (
            <>
              <CashAccountSelect value={cashAccount} onChange={setCashAccount} disabled={busy || review} label="Rekening pembayaran supplier" />
              <div className="bill-form-grid">
                <FormField
                  label="Nominal pembayaran *"
                  hint={`Sisa utang ${exactMoney(bill.outstanding)}. Saldo kas diperiksa saat posting.`}
                >
                  <input
                    required
                    inputMode="decimal"
                    value={amount}
                    onChange={(e) => setAmount(e.target.value)}
                    disabled={busy || review}
                    placeholder="Contoh: 60000"
                  />
                </FormField>
                <FormField label="Referensi pembayaran">
                  <input
                    maxLength={200}
                    value={reference}
                    onChange={(e) => setReference(e.target.value)}
                    disabled={busy || review}
                    placeholder="Nomor bukti pembayaran"
                  />
                </FormField>
              </div>
              <button
                type="button"
                className="text-button"
                disabled={busy || review}
                onClick={() => setAmount(bill.outstanding)}
              >
                Isi seluruh sisa utang
              </button>
            </>
          )}
          {review && <div className="info-box"><p>{selected ? `Pembalikan ${exactMoney(selected.amount)} memakai rekening pada jurnal pembayaran asli.` : `Kas keluar ${exactMoney(amount)} dari akun ${cashAccount}. Pastikan dana benar-benar sudah dibayarkan ke supplier.`}</p></div>}
          <FormError error={error} />
          {error && (
            <button
              type="button"
              className="text-button"
              disabled={busy || review}
              onClick={onSaved}
            >
              Muat tagihan terbaru
            </button>
          )}
          <div className="purchase-review-actions">
            {review && <button type="button" className="button secondary" disabled={busy} onClick={() => setReview(false)}>Ubah rincian</button>}
            {selected && (
              <button
                type="button"
                className="button secondary"
                disabled={busy || review}
                onClick={() => {
                  setReview(false);
                  setSelected(null);
                  setError("");
                }}
              >
                Batal koreksi
              </button>
            )}
            <button className="button primary" disabled={busy}>
              {busy
                ? "Memproses…"
                : !review
                  ? "Tinjau pembayaran"
                  : selected ? "Balikkan pembayaran" : "Konfirmasi pembayaran supplier"}
            </button>
          </div>
        </form>
      )}
      {!selected && error && <FormError error={error} />}
    </section>
  );
}
