import { useState } from "react";
import { EmptyState, ErrorState, Loading } from "./components";
import { exactMoney, moneyCents, calendarDate } from "./dashboard-charts";
import { useResource } from "./hooks";
import { downloadCsv } from "./export";
export interface PayablesAnalysis {
  today: string;
  timezone: string;
  as_of: string;
  months: number;
  cash_balance: string;
  aging: { key: string; count: number; total: string }[];
  bills: {
    id: string;
    number: string;
    supplier_name: string;
    supplier_invoice_number: string;
    due_date: string;
    days_overdue: number;
    bucket: string;
    total: string;
    credit_total: string;
    net_total: string;
    paid: string;
    outstanding: string;
  }[];
  series: { month: string; paid: string; reversed: string; net: string }[];
}
const labels: Record<string, string> = {
  current: "Belum terlambat",
  days_1_30: "Terlambat 1–30 hari",
  days_31_60: "Terlambat 31–60 hari",
  days_61_90: "Terlambat 61–90 hari",
  over_90: "Terlambat >90 hari",
};
export function SupplierPayablesInsights({
  revision,
  onSelect,
}: {
  revision: number;
  onSelect: (id: string) => void;
}) {
  const [months, setMonths] = useState(6);
  const [bucket, setBucket] = useState("");
  const resource = useResource<PayablesAnalysis>(
    `/purchase-bills/analysis?months=${months}`,
    revision,
  );
  const data = resource.data;
  if (resource.loading) return <Loading />;
  if (resource.error)
    return <ErrorState message={resource.error} onRetry={resource.reload} />;
  if (!data) return null;
  const maximum = data.aging.reduce(
    (sum, row) => (moneyCents(row.total) > sum ? moneyCents(row.total) : sum),
    0n,
  );
  const monthMax = data.series.reduce(
    (sum, row) =>
      [moneyCents(row.paid), moneyCents(row.reversed)].reduce(
        (s, x) => (x > s ? x : s),
        sum,
      ),
    0n,
  );
  const width = (value: string, max: bigint) =>
    max === 0n ? 0 : Number((moneyCents(value) * 10000n) / max) / 100;
  const entries = data.bills.filter(
    (bill) => !bucket || bill.bucket === bucket,
  );
  return (
    <section className="panel payables-analysis">
      <div className="table-toolbar">
        <div>
          <h2>Umur utang dan pembayaran</h2>
          <p className="muted">
            Seluruh sisa utang per {calendarDate(data.today)} · {data.timezone}
          </p>
        </div>
        <label>
          Riwayat pembayaran{" "}
          <select
            value={months}
            onChange={(e) => setMonths(Number(e.target.value))}
          >
            {[3, 6, 12].map((n) => (
              <option key={n} value={n}>
                {n} bulan
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="payables-analysis-grid">
        <div>
          <h3>Berapa lama utang terlambat?</h3>
          {data.aging.map((row) => (
            <button
              key={row.key}
              className={`payables-aging-row ${bucket === row.key ? "selected" : ""}`}
              aria-pressed={bucket === row.key}
              onClick={() => setBucket(bucket === row.key ? "" : row.key)}
            >
              <span>
                {labels[row.key]}
                <small>{row.count} tagihan</small>
              </span>
              <span className="payables-bar-track">
                <i style={{ width: `${width(row.total, maximum)}%` }} />
              </span>
              <strong>{exactMoney(row.total)}</strong>
            </button>
          ))}
          <p className="muted">
            Pilih kelompok untuk melihat tagihannya. Hari jatuh tempo belum
            dianggap terlambat.
          </p>
        </div>
        <div>
          <h3>Pembayaran, refund, dan koreksi per bulan</h3>
          {data.series.map((row) => (
            <div className="payables-month" key={row.month}>
              <span>{row.month}</span>
              <div>
                <span className="payables-bar-track">
                  <i style={{ width: `${width(row.paid, monthMax)}%` }} />
                </span>
                <span className="payables-bar-track refund">
                  <i style={{ width: `${width(row.reversed, monthMax)}%` }} />
                </span>
              </div>
              <span>
                <strong>{exactMoney(row.net)}</strong>
                <small>
                  Keluar {exactMoney(row.paid)} · kembali{" "}
                  {exactMoney(row.reversed)}
                </small>
              </span>
            </div>
          ))}
          <p className="muted">
            Hijau: kas keluar. Jingga: kas kembali dari pembalikan atau refund.
            Nominal utama adalah kas keluar bersih sesuai bulan posting.
          </p>
        </div>
      </div>
      <details className="payables-aging-details">
        <summary>
          Rincian umur utang · {entries.length} tagihan{" "}
          {bucket && `· ${labels[bucket]}`}
        </summary>
        <button
          className="text-button"
          onClick={() =>
            downloadCsv(
              "haruflow-umur-utang",
              [
                "Tagihan",
                "Supplier",
                "Invoice supplier",
                "Jatuh tempo",
                "Hari terlambat",
                "Kelompok",
                "Total",
                "Kredit retur",
                "Tagihan bersih",
                "Dibayar",
                "Sisa",
              ],
              entries.map((b) => [
                b.number,
                b.supplier_name,
                b.supplier_invoice_number,
                b.due_date,
                b.days_overdue,
                labels[b.bucket],
                b.total,
                b.credit_total,
                b.net_total,
                b.paid,
                b.outstanding,
              ]),
            )
          }
        >
          Ekspor CSV
        </button>
        {entries.length ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Tagihan</th>
                  <th>Supplier</th>
                  <th>Jatuh tempo</th>
                  <th>Hari terlambat</th>
                  <th className="align-right">Sisa utang</th>
                </tr>
              </thead>
              <tbody>
                {entries.map((bill) => (
                  <tr key={bill.id}>
                    <td>
                      <button
                        className="table-link"
                        onClick={() => onSelect(bill.id)}
                      >
                        {bill.number}
                      </button>
                    </td>
                    <td>{bill.supplier_name}</td>
                    <td>{calendarDate(bill.due_date)}</td>
                    <td>{bill.days_overdue}</td>
                    <td className="align-right">
                      {exactMoney(bill.outstanding)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <EmptyState
            title="Tidak ada sisa utang pada kelompok ini."
            description="Pilih kelompok lain untuk melihat tagihan yang masih terbuka."
          />
        )}
      </details>
    </section>
  );
}
