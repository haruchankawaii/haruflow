import { exactMoney } from "./dashboard-charts";
import { dateTime, statusNames } from "./format";
import { purchaseStatusRows, type PurchasingSummary } from "./purchasing-model";

export function PurchaseStatusChart({
  summary,
  selected,
  onSelect,
}: {
  summary: PurchasingSummary;
  selected: string;
  onSelect: (status: string) => void;
}) {
  const rows = purchaseStatusRows(summary);
  const count = rows.reduce((total, row) => total + row.count, 0);
  return (
    <section
      className="purchase-status-chart panel"
      aria-labelledby="purchase-chart-title"
    >
      <div className="purchase-chart-heading">
        <div>
          <span className="purchase-chart-eyebrow">GAMBARAN PEMBELIAN</span>
          <h2 id="purchase-chart-title">Posisi seluruh PO</h2>
          <p>
            Jumlah pesanan menurut status saat ini ·{" "}
            {count.toLocaleString("id-ID")} PO
          </p>
        </div>
        <small>Diperbarui {dateTime(summary.as_of)}</small>
      </div>
      <div className="purchase-chart-content">
        <div
          className="purchase-chart-bars"
          aria-label="Grafik jumlah PO per status"
        >
          {rows.map((row) => (
            <button
              key={row.status}
              className={`purchase-chart-row ${row.status} ${selected === row.status ? "selected" : ""}`}
              aria-pressed={selected === row.status}
              aria-label={`${statusNames[row.status]}: ${row.count} PO, ${exactMoney(row.value)}. Filter daftar PO.`}
              onClick={() =>
                onSelect(selected === row.status ? "" : row.status)
              }
            >
              <span className="purchase-chart-label">
                <i />
                {statusNames[row.status]}
              </span>
              <svg
                viewBox="0 0 100 10"
                preserveAspectRatio="none"
                aria-hidden="true"
              >
                <rect
                  className="purchase-chart-track"
                  width="100"
                  height="10"
                  rx="3"
                />
                <rect
                  width={row.width}
                  height="10"
                  rx="3"
                  fill="currentColor"
                />
              </svg>
              <span className="purchase-chart-value">
                <b>{row.count.toLocaleString("id-ID")} PO</b>
                <small>{exactMoney(row.value)}</small>
              </span>
            </button>
          ))}
          {count === 0 && (
            <p className="purchase-chart-empty">
              Belum ada PO. Buat pesanan pertama untuk mulai memantau pembelian.
            </p>
          )}
          <p className="purchase-chart-caption">
            Panjang batang menunjukkan jumlah PO. Pilih status untuk memfilter
            daftar di bawah.
          </p>
        </div>
        <div className="purchase-chart-highlights">
          <div>
            <span>Menunggu penerimaan</span>
            <strong>{exactMoney(summary.pending_receipt_total)}</strong>
            <small>PO dikonfirmasi; barang belum dicatat masuk.</small>
          </div>
          <div className="received">
            <span>Barang diterima, belum ditagih</span>
            <strong>{exactMoney(summary.received_unbilled_total)}</strong>
            <small>
              Persediaan dan kewajiban sementara sudah tercatat. Tagihan serta
              pembayaran supplier tersedia pada tahap berikutnya.
            </small>
          </div>
        </div>
      </div>
      <p className="purchase-chart-scope">
        Seluruh PO sepanjang waktu, termasuk di luar 500 dokumen terbaru. Grafik
        tidak mengikuti pencarian atau filter daftar. Nilai menggunakan biaya
        beli tersimpan dalam IDR.
      </p>
    </section>
  );
}
