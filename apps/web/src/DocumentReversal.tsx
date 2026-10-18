import { useRef, useState, type FormEvent } from "react";
import { Undo2 } from "lucide-react";
import { api, errorMessage } from "./api";
import { FormActions, FormError, FormField, Modal } from "./components";
import { costMoney, date, money, number } from "./format";
import type { Journal, Movement } from "./types";
import "./document-corrections.css";

type Props = {
  onClose: () => void;
  onSaved: () => void;
} & (
  { journal: Journal; receipt?: never } | { receipt: Movement; journal?: never }
);

function ReceiptSummary({ receipt }: { receipt: Movement }) {
  return (
    <div className="correction-summary">
      <strong>{receipt.product_name}</strong>
      <span>
        {date(receipt.created_at)}
        {receipt.reference ? ` · ${receipt.reference}` : ""}
      </span>
      <p>
        {number(receipt.quantity)} × {costMoney(receipt.unit_cost)} · Nilai
        persediaan <strong>{money(receipt.value)}</strong>
      </p>
    </div>
  );
}

export function ReceiptHistory({
  receipt,
  onClose,
}: {
  receipt: Movement;
  onClose: () => void;
}) {
  return (
    <Modal
      title="Penerimaan asal"
      description="Riwayat penerimaan yang dikoreksi tetap tersimpan."
      onClose={onClose}
    >
      <div className="form-body">
        <ReceiptSummary receipt={receipt} />
        {receipt.reversal && (
          <div className="correction-history">
            <strong>Dibatalkan pada {date(receipt.reversal.created_at)}</strong>
            <p>Alasan: {receipt.reversal.reason}</p>
          </div>
        )}
      </div>
      <div className="form-actions">
        <button className="button secondary" onClick={onClose}>
          Tutup
        </button>
      </div>
    </Modal>
  );
}

export function DocumentReversal({
  journal,
  receipt,
  onClose,
  onSaved,
}: Props) {
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const submitting = useRef(false);
  const attempt = useRef<{ reason: string; key: string } | null>(null);
  const title = journal ? "Balikkan jurnal" : "Batalkan penerimaan";
  const close = () => {
    if (!submitting.current) onClose();
  };
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (submitting.current) return;
    const trimmed = reason.trim();
    if (trimmed.length < 3 || trimmed.length > 1000) {
      setError("Alasan harus berisi 3 sampai 1.000 karakter.");
      return;
    }
    if (attempt.current?.reason !== trimmed) {
      attempt.current = { reason: trimmed, key: crypto.randomUUID() };
    }
    submitting.current = true;
    setBusy(true);
    setError("");
    try {
      await api(
        journal
          ? `/accounting/journals/${journal.id}/reverse`
          : `/inventory/receipts/${receipt.id}/reverse`,
        {
          method: "POST",
          body: { reason: trimmed, idempotency_key: attempt.current.key },
        },
      );
      onSaved();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      submitting.current = false;
      setBusy(false);
    }
  }
  return (
    <Modal
      title={title}
      description="Periksa transaksi asal dan isi alasan koreksi."
      onClose={close}
      wide={!!journal}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          {journal ? (
            <div className="correction-summary">
              <strong>{journal.description}</strong>
              <span>{date(journal.created_at)}</span>
              <div className="manual-totals">
                <span>
                  Total debit{" "}
                  <strong>
                    {money(
                      journal.lines.reduce(
                        (sum, line) => sum + Number(line.debit),
                        0,
                      ),
                    )}
                  </strong>
                </span>
                <span>
                  Total kredit{" "}
                  <strong>
                    {money(
                      journal.lines.reduce(
                        (sum, line) => sum + Number(line.credit),
                        0,
                      ),
                    )}
                  </strong>
                </span>
              </div>
            </div>
          ) : (
            <ReceiptSummary receipt={receipt} />
          )}
          {journal && (
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Akun asal</th>
                    <th className="align-right">Debit</th>
                    <th className="align-right">Kredit</th>
                  </tr>
                </thead>
                <tbody>
                  {journal.lines.map((line, index) => (
                    <tr key={`${line.account_code}-${index}`}>
                      <td>
                        <span className="mono muted">{line.account_code}</span>{" "}
                        {line.account_name}
                      </td>
                      <td className="align-right">{money(line.debit)}</td>
                      <td className="align-right">{money(line.credit)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <div className="info-box">
            <Undo2 size={18} />
            <p>
              {journal
                ? "Pembalikan membuat jurnal dengan debit dan kredit berlawanan. Jurnal asal tetap tersimpan. Setelah selesai, buat jurnal pengganti bila diperlukan."
                : "Pembatalan mengurangi stok dan membalik jurnal penerimaan. Riwayat asal tetap tersimpan. Hanya penerimaan terakhir produk yang dapat dibatalkan; transaksi stok sesudahnya harus diperiksa lebih dahulu."}
            </p>
          </div>
          <FormField
            label="Alasan koreksi *"
            hint="Alasan disimpan bersama pengguna dan waktu pembalikan."
          >
            <textarea
              required
              autoFocus
              minLength={3}
              maxLength={1000}
              rows={3}
              disabled={busy}
              value={reason}
              onChange={(event) => setReason(event.target.value)}
              placeholder={
                journal
                  ? "Contoh: Nominal beban tercatat terlalu besar"
                  : "Contoh: Jumlah atau biaya penerimaan salah dicatat"
              }
            />
          </FormField>
          <FormError error={error} />
        </div>
        <FormActions onClose={close} busy={busy} label={title} />
      </form>
    </Modal>
  );
}
