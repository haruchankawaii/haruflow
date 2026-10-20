import { useRef, useState, type FormEvent } from "react";
import { api, errorMessage } from "./api";
import { AttachmentsPanel } from "./AttachmentsPanel";
import { FormError, FormField } from "./components";
import { exactMoney, moneyCents } from "./dashboard-charts";
import { dateTime } from "./format";
import { requestIdentity, type RequestIdentity } from "./purchasing-model";
import {
  previewReturn,
  quantityMillis,
  returnEndpoint,
  returnItemName,
  returnQuantityLabel,
  returnReversalEndpoint,
  type ReturnKind,
  type ReturnableItem,
  type ReturnRecord,
} from "./returns-model";
import "./returns.css";

export function ReturnsPanel({
  kind,
  documentId,
  version,
  total,
  paid,
  creditTotal,
  items,
  returns,
  canReturn,
  canManage,
  onSaved,
  onJournal,
}: {
  kind: ReturnKind;
  documentId: string;
  version: number;
  total: string;
  paid: string;
  creditTotal: string;
  items: ReturnableItem[];
  returns: ReturnRecord[];
  canReturn: boolean;
  canManage: boolean;
  onSaved: () => void;
  onJournal: (id: string) => void;
}) {
  const [creating, setCreating] = useState(false);
  const [formVersion, setFormVersion] = useState(version);
  const [selected, setSelected] = useState<ReturnRecord | null>(null);
  const [quantities, setQuantities] = useState<Record<string, string>>({});
  const [reason, setReason] = useState("");
  const [review, setReview] = useState(false);
  const [confirmed, setConfirmed] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  const preview = previewReturn(items, quantities, total, paid, creditTotal);
  const supplier = kind === "supplier";
  function reset() {
    setCreating(false);
    setSelected(null);
    setQuantities({});
    setReason("");
    setReview(false);
    setConfirmed(false);
    setError("");
  }
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busyRef.current) return;
    if (reason.trim().length < 3) {
      setError("Isi alasan minimal tiga karakter.");
      return;
    }
    if (!selected && (preview.error || (preview.refund > 0n && !confirmed))) {
      setError(
        preview.error || "Konfirmasikan pengembalian uang yang sudah terjadi.",
      );
      return;
    }
    if (selected && moneyCents(selected.refund) > 0n && !confirmed) {
      setError(
        "Konfirmasikan perpindahan uang untuk pembalikan retur yang sudah terjadi.",
      );
      return;
    }
    if (!review) {
      setReview(true);
      setError("");
      return;
    }
    const path = selected
      ? returnReversalEndpoint(kind, selected.id)
      : returnEndpoint(kind, documentId);
    const payload = selected
      ? { version: formVersion, reason: reason.trim() }
      : {
          version: formVersion,
          reason: reason.trim(),
          refund_confirmed: confirmed,
          items: preview.items.map((item) => ({
            [supplier ? "bill_line_id" : "invoice_line_id"]: item.id,
            quantity: item.quantity,
          })),
        };
    identity.current = requestIdentity(identity.current, { path, ...payload });
    busyRef.current = true;
    setBusy(true);
    setError("");
    try {
      await api(path, {
        method: "POST",
        body: { ...payload, idempotency_key: identity.current.key },
      });
      reset();
      onSaved();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  return (
    <section className="returns-panel">
      <div className="returns-heading">
        <div>
          <h3>{supplier ? "Retur ke supplier" : "Retur pelanggan"}</h3>
          <p>
            Retur mencatat pengembalian barang/jasa dan kredit tagihan. Pesanan
            tetap menyimpan jumlah yang sudah dipenuhi.
          </p>
        </div>
        {canManage && canReturn && !creating && !selected && (
          <button
            type="button"
            className="button secondary"
            onClick={() => {
              reset();
              setFormVersion(version);
              setCreating(true);
            }}
          >
            Catat retur
          </button>
        )}
      </div>
      <div className="returns-credit">
        <span>
          Total kredit retur <strong>{exactMoney(creditTotal)}</strong>
        </span>
      </div>
      {(returns ?? []).map((record) => (
        <article className="return-record" key={record.id}>
          <header>
            <strong>{record.number}</strong>
            <span>
              {record.reversal ? "Sudah dibalik" : "Retur tercatat"} ·{" "}
              {dateTime(record.created_at)}
            </span>
          </header>
          <p className="purchase-prewrap">Alasan: {record.reason}</p>
          <div className="returns-amounts">
            <span>
              Kredit <strong>{exactMoney(record.credit_total)}</strong>
            </span>
            <span>
              {supplier
                ? "Uang kembali dari supplier"
                : "Pengembalian uang pelanggan"}{" "}
              <strong>{exactMoney(record.refund)}</strong>
            </span>
          </div>
          <details>
            <summary>{record.items.length} jenis barang/jasa</summary>
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Barang/jasa</th>
                    <th>Jumlah</th>
                    <th className="align-right">Kredit</th>
                  </tr>
                </thead>
                <tbody>
                  {record.items.map((item) => (
                    <tr key={item.id}>
                      <td>{returnItemName(item)}</td>
                      <td>
                        {returnQuantityLabel(item.quantity)} {item.unit}
                      </td>
                      <td className="align-right">{exactMoney(item.credit)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </details>
          <div className="purchase-receipt-actions">
            <button
              type="button"
              className="text-button"
              onClick={() => onJournal(record.journal_id)}
            >
              Jurnal retur
            </button>
            {record.reversal ? (
              <button
                type="button"
                className="text-button"
                onClick={() => onJournal(record.reversal!.journal_id)}
              >
                Jurnal pembalikan
              </button>
            ) : (
              canManage &&
              record.can_reverse && (
                <button
                  type="button"
                  className="text-button danger-text"
                  disabled={busy}
                  onClick={() => {
                    reset();
                    setFormVersion(version);
                    setSelected(record);
                  }}
                >
                  Koreksi retur
                </button>
              )
            )}
          </div>
          {record.reversal && (
            <p className="purchase-prewrap">
              Dibalik {dateTime(record.reversal.created_at)} ·{" "}
              {record.reversal.reason}
            </p>
          )}
          {!record.reversal &&
            !record.can_reverse &&
            record.reversal_blocked_reason && (
              <p className="bill-blocked">{record.reversal_blocked_reason}</p>
            )}
          <AttachmentsPanel
            documentType={supplier ? "supplier_return" : "sales_return"}
            documentId={record.id}
            title={`Lampiran ${record.number}`}
          />
        </article>
      ))}
      {canManage && (creating || selected) && (
        <form className="returns-form" onSubmit={submit}>
          <h4>
            {selected
              ? `Balikkan ${selected.number}`
              : supplier
                ? "Barang dikembalikan ke supplier"
                : "Barang/jasa diretur pelanggan"}
          </h4>
          {!selected && !review && (
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Barang/jasa</th>
                    <th>Sisa dapat diretur</th>
                    <th>Jumlah retur</th>
                  </tr>
                </thead>
                <tbody>
                  {items
                    .filter(
                      (item) =>
                        quantityMillis(item.remaining_return_quantity) > 0n,
                    )
                    .map((item) => (
                      <tr key={item.id}>
                        <td>
                          {returnItemName(item)}
                          <small className="bill-cell-sub">
                            {item.product_sku ?? item.sku}
                          </small>
                        </td>
                        <td>
                          {returnQuantityLabel(item.remaining_return_quantity)}{" "}
                          {item.unit}
                        </td>
                        <td>
                          <input
                            aria-label={`Jumlah retur ${returnItemName(item)}`}
                            inputMode="decimal"
                            placeholder="0"
                            value={quantities[item.id] ?? ""}
                            disabled={busy}
                            onChange={(event) =>
                              setQuantities((previous) => ({
                                ...previous,
                                [item.id]: event.target.value,
                              }))
                            }
                          />
                        </td>
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
          )}
          {!selected && (
            <>
              <div className="returns-amounts">
                <span>
                  Kredit retur <strong>{exactMoney(preview.credit)}</strong>
                </span>
                <span>
                  Sisa setelah retur{" "}
                  <strong>{exactMoney(preview.outstandingAfter)}</strong>
                </span>
                <span>
                  Pengembalian uang{" "}
                  <strong>{exactMoney(preview.refund)}</strong>
                </span>
              </div>
              {review && (
                <ul>
                  {preview.items.map((item) => (
                    <li key={item.id}>
                      {returnItemName(
                        items.find((line) => line.id === item.id)!,
                      )}{" "}
                      · {item.quantity} · {exactMoney(item.credit)}
                    </li>
                  ))}
                </ul>
              )}
              <p>
                {supplier
                  ? "Stok berkurang menurut biaya rata-rata saat posting. Selisih dengan nilai kredit dicatat pada akun selisih retur."
                  : "Barang stok kembali memakai biaya historis pengiriman. Retur jasa mengurangi tagihan tanpa mutasi stok."}
              </p>
              {preview.refund > 0n && (
                <label className="returns-confirm">
                  <input
                    type="checkbox"
                    checked={confirmed}
                    disabled={busy || review}
                    onChange={(e) => setConfirmed(e.target.checked)}
                  />
                  <span>
                    Saya memastikan{" "}
                    {supplier
                      ? "uang kembali dari supplier sudah diterima"
                      : "uang pelanggan sudah dikembalikan"}{" "}
                    sebesar {exactMoney(preview.refund)}. Retur ini mencatat
                    perubahan kas tersebut.
                  </span>
                </label>
              )}
              <FormError error={preview.error} />
            </>
          )}
          {selected && (
            <p>
              Jurnal retur dan kredit {exactMoney(selected.credit_total)}{" "}
              dibalik. Stok dipulihkan hanya jika belum memiliki mutasi
              lanjutan. Pengembalian uang yang tercatat ikut dibalik sebesar{" "}
              {exactMoney(selected.refund)}.
            </p>
          )}
          {selected && moneyCents(selected.refund) > 0n && (
            <label className="returns-confirm">
              <input
                type="checkbox"
                checked={confirmed}
                disabled={busy || review}
                onChange={(e) => setConfirmed(e.target.checked)}
              />
              <span>
                Saya memastikan{" "}
                {supplier
                  ? "dana refund sudah dikembalikan ke supplier"
                  : "dana refund sudah diterima kembali dari pelanggan"}{" "}
                sebesar {exactMoney(selected.refund)}. Pembalikan ini mencatat{" "}
                {supplier ? "kas keluar" : "kas masuk"} tersebut.
              </span>
            </label>
          )}
          <FormField
            label="Alasan *"
            hint="3–1.000 karakter, tersimpan dalam audit."
          >
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
          <FormError error={error} />
          {error && (
            <button
              type="button"
              className="text-button"
              disabled={busy}
              onClick={() => {
                reset();
                onSaved();
              }}
            >
              Muat dokumen terbaru
            </button>
          )}
          <div className="purchase-review-actions">
            <button
              type="button"
              className="button secondary"
              disabled={busy}
              onClick={reset}
            >
              Batal
            </button>
            {review && (
              <button
                type="button"
                className="button secondary"
                disabled={busy}
                onClick={() => setReview(false)}
              >
                Ubah rincian
              </button>
            )}
            <button
              className="button primary"
              disabled={busy || (!selected && !!preview.error)}
            >
              {busy
                ? "Memproses…"
                : !review
                  ? "Tinjau retur"
                  : selected
                    ? "Balikkan retur"
                    : "Posting retur"}
            </button>
          </div>
        </form>
      )}
    </section>
  );
}
