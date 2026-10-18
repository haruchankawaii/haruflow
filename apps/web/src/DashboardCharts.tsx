import { useId, useState } from "react";
import { ArrowDownLeft, ArrowUpRight, ChartNoAxesCombined } from "lucide-react";
import { ErrorState, Loading } from "./components";
import { useResource } from "./hooks";
import {
  amountTone,
  calendarDate,
  compactMoney,
  exactMoney,
  moneyCents,
  moneyScale,
  monthLabel,
  paidShare,
  scaleY,
  type DashboardInsights,
} from "./dashboard-charts";
import "./dashboard-charts.css";

type MetricKey = "revenue" | "cogs" | "cash_in" | "cash_out";
interface ChartMetric {
  key: MetricKey;
  label: string;
  color: string;
}
const revenueMetrics: ChartMetric[] = [
  { key: "revenue", label: "Pendapatan", color: "#295d46" },
  { key: "cogs", label: "HPP", color: "#b98a4a" },
];
const cashMetrics: ChartMetric[] = [
  { key: "cash_in", label: "Kas masuk", color: "#447b89" },
  { key: "cash_out", label: "Kas keluar", color: "#bd7458" },
];

function MonthlyChart({
  series,
  metrics,
  label,
  resultKey,
  resultLabel,
}: {
  series: DashboardInsights["series"];
  metrics: ChartMetric[];
  label: string;
  resultKey: "gross_profit" | "cash_net";
  resultLabel: string;
}) {
  const id = useId();
  const [activeMonth, setActiveMonth] = useState<string | null>(null);
  const current = series.find((item) => item.month === activeMonth);
  const values = series.flatMap((item) =>
    metrics.map((metric) => moneyCents(item[metric.key])),
  );
  const empty = values.every((value) => value === 0n);
  const scale = moneyScale(values);
  const width = 540;
  const height = 228;
  const left = 80;
  const top = 12;
  const plotWidth = width - left - 14;
  const plotHeight = height - top - 38;
  const zero = top + scaleY(0n, scale, plotHeight);
  const groupWidth = plotWidth / Math.max(series.length, 1);
  const barWidth = Math.min(23, groupWidth * 0.28);
  const pointDescription = (item: DashboardInsights["series"][number]) =>
    `${monthLabel(item.month, true)}: ${metrics.map((metric) => `${metric.label} ${exactMoney(item[metric.key])}`).join("; ")}; ${resultLabel} ${exactMoney(item[resultKey])}`;
  return (
    <>
      <ul className="insight-legend" aria-label={`Legenda ${label}`}>
        {metrics.map((metric) => (
          <li key={metric.key}>
            <span style={{ backgroundColor: metric.color }} />
            {metric.label}
          </li>
        ))}
      </ul>
      <div className="insight-plot-scroll">
        <svg
          className={`insight-plot ${series.length > 6 ? "many-months" : ""}`}
          viewBox={`0 0 ${width} ${height}`}
          role="group"
          aria-labelledby={`${id}-title ${id}-description`}
          onMouseLeave={() => setActiveMonth(null)}
        >
          <title id={`${id}-title`}>{label}</title>
          <desc id={`${id}-description`}>
            Nilai rupiah per bulan. Fokuskan atau arahkan penunjuk ke bulan
            untuk melihat nilai lengkap. Garis tebal menunjukkan nol.
          </desc>
          {scale.ticks.map((tick) => {
            const y = top + scaleY(tick, scale, plotHeight);
            return (
              <g key={tick.toString()} aria-hidden="true">
                <line
                  x1={left}
                  x2={width - 14}
                  y1={y}
                  y2={y}
                  className={tick === 0n ? "insight-zero-line" : "insight-grid-line"}
                />
                <text x={left - 10} y={y + 4} textAnchor="end" className="insight-axis">
                  {compactMoney(tick)}
                </text>
              </g>
            );
          })}
          {series.map((item, index) => {
            const x = left + groupWidth * index;
            const center = x + groupWidth / 2;
            return (
              <g
                key={item.month}
                tabIndex={0}
                role="img"
                aria-label={pointDescription(item)}
                className={`insight-month ${activeMonth === item.month ? "active" : ""}`}
                onMouseEnter={() => setActiveMonth(item.month)}
                onFocus={() => setActiveMonth(item.month)}
                onBlur={() => setActiveMonth(null)}
              >
                <title>{pointDescription(item)}</title>
                <rect
                  x={x + 2}
                  y={top}
                  width={Math.max(groupWidth - 4, 1)}
                  height={plotHeight + 30}
                  rx={5}
                  className="insight-month-target"
                />
                {metrics.map((metric, metricIndex) => {
                  const value = moneyCents(item[metric.key]);
                  const y = top + scaleY(value, scale, plotHeight);
                  return (
                    <rect
                      key={metric.key}
                      x={center + (metricIndex - metrics.length / 2) * (barWidth + 3)}
                      y={Math.min(y, zero)}
                      width={barWidth}
                      height={Math.abs(zero - y)}
                      rx={2}
                      fill={metric.color}
                      className="insight-bar"
                    />
                  );
                })}
                <text
                  x={center}
                  y={height - 10}
                  textAnchor="middle"
                  className="insight-axis insight-month-label"
                  aria-hidden="true"
                >
                  {monthLabel(item.month)}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
      <div className={`insight-readout ${current ? "is-active" : ""}`} role="status" aria-live="polite">
        {current ? (
          <>
            <strong>{monthLabel(current.month, true)}</strong>
            <div>
              {metrics.map((metric) => (
                <span key={metric.key}>
                  {metric.label} <b>{exactMoney(current[metric.key])}</b>
                </span>
              ))}
              <span>
                {resultLabel} <b>{exactMoney(current[resultKey])}</b>
              </span>
            </div>
          </>
        ) : (
          <span>
            {empty
              ? "Belum ada nilai tercatat pada periode ini."
              : "Arahkan penunjuk atau fokuskan bulan untuk rincian rupiah."}
          </span>
        )}
      </div>
      <details className="insight-details">
        <summary>Lihat rincian per bulan</summary>
        <div className="table-scroll">
          <table>
            <caption className="sr-only">{label}, nilai rupiah lengkap</caption>
            <thead>
              <tr>
                <th>Bulan</th>
                {metrics.map((metric) => <th key={metric.key} className="align-right">{metric.label}</th>)}
                <th className="align-right">{resultLabel}</th>
              </tr>
            </thead>
            <tbody>
              {series.map((item) => (
                <tr key={item.month}>
                  <th scope="row">{monthLabel(item.month, true)}</th>
                  {metrics.map((metric) => <td key={metric.key} className="align-right">{exactMoney(item[metric.key])}</td>)}
                  <td className="align-right">{exactMoney(item[resultKey])}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </details>
    </>
  );
}

function ReceivablesChart({ data }: { data: DashboardInsights["receivables"] }) {
  const id = useId();
  const paid = paidShare(data.paid, data.invoiced);
  const empty = moneyCents(data.invoiced) === 0n;
  const fraction = Math.max(0, Math.min(paid, 100)) / 100;
  const circumference = 2 * Math.PI * 66;
  const count = data.unpaid_count + data.partial_count + data.paid_count;
  return (
    <article className="panel insight-card insight-receivables">
      <div className="insight-card-heading">
        <div>
          <span className="insight-kicker">PENAGIHAN</span>
          <h3>Pembayaran invoice</h3>
          <p>Posisi invoice aktif saat ini · seluruh waktu</p>
        </div>
        <span className="insight-snapshot">Tidak mengikuti rentang grafik</span>
      </div>
      <div className="insight-receivables-body">
        <div className="insight-donut-wrap">
          <svg viewBox="0 0 180 180" className="insight-donut" role="img" aria-labelledby={`${id}-title`}>
            <title id={`${id}-title`}>
              {empty ? "Belum ada invoice aktif" : `Pembayaran diterima ${exactMoney(data.paid)}; sisa piutang ${exactMoney(data.outstanding)}; total invoice aktif ${exactMoney(data.invoiced)}`}
            </title>
            <circle cx={90} cy={90} r={66} fill="none" stroke={empty ? "#e9ede5" : "#e4b865"} strokeWidth={19} />
            {!empty && <circle cx={90} cy={90} r={66} fill="none" stroke="#295d46" strokeWidth={19} strokeDasharray={`${fraction * circumference} ${circumference}`} transform="rotate(-90 90 90)" />}
            <text x={90} y={86} textAnchor="middle" className="insight-donut-value">{empty ? "—" : `${paid.toLocaleString("id-ID", { maximumFractionDigits: 2 })}%`}</text>
            <text x={90} y={109} textAnchor="middle" className="insight-donut-label">{empty ? "Belum ada invoice" : "nilai terbayar"}</text>
          </svg>
        </div>
        <dl className="insight-receivable-values">
          <div>
            <dt><span className="insight-key paid" />Pembayaran diterima</dt>
            <dd>{exactMoney(data.paid)}</dd>
          </div>
          <div>
            <dt><span className="insight-key outstanding" />Sisa piutang</dt>
            <dd>{exactMoney(data.outstanding)}</dd>
          </div>
          <div className="insight-invoiced-total">
            <dt>Nilai invoice aktif</dt>
            <dd>{exactMoney(data.invoiced)}</dd>
          </div>
        </dl>
        <div className="insight-invoice-counts">
          <span>{count.toLocaleString("id-ID")} invoice aktif</span>
          <p><b>{data.unpaid_count.toLocaleString("id-ID")}</b> belum dibayar</p>
          <p><b>{data.partial_count.toLocaleString("id-ID")}</b> dibayar sebagian</p>
          <p><b>{data.paid_count.toLocaleString("id-ID")}</b> lunas</p>
          <small>Invoice yang dibatalkan tidak dihitung.</small>
        </div>
      </div>
    </article>
  );
}

export default function DashboardCharts({ revision }: { revision: number }) {
  const [months, setMonths] = useState(6);
  const { data, loading, error, reload } = useResource<DashboardInsights>(
    `/dashboard/insights?months=${months}`,
    revision,
  );
  return (
    <section className="dashboard-insights" aria-labelledby="dashboard-insights-title">
      <div className="insight-section-heading">
        <div>
          <span className="insight-section-eyebrow"><ChartNoAxesCombined size={16} aria-hidden="true" /> BACA ARAH BISNIS</span>
          <h2 id="dashboard-insights-title">Angka yang bercerita</h2>
          <p>
            {data && !loading ? `${calendarDate(data.from)} – ${calendarDate(data.through)} · ${data.timezone}` : "Ringkasan pembukuan menurut kalender perusahaan."}
          </p>
        </div>
        <fieldset className="insight-range">
          <legend className="sr-only">Rentang grafik bulanan</legend>
          {[3, 6, 12].map((value) => (
            <button key={value} type="button" aria-pressed={months === value} onClick={() => setMonths(value)}>{value} bulan</button>
          ))}
        </fieldset>
      </div>
      <div aria-busy={loading}>
        {loading ? <div className="insight-loading"><Loading /></div> : error || !data ? <ErrorState message={error} onRetry={reload} /> : (
          <div className="dashboard-insights-grid">
            <article className="panel insight-card">
              <div className="insight-card-heading">
                <div>
                  <span className="insight-kicker">PENDAPATAN & BIAYA BARANG</span>
                  <h3>Pendapatan dan HPP</h3>
                  <p>Bandingkan pendapatan dengan harga pokok penjualan.</p>
                </div>
              </div>
              <div className="insight-result-row">
                <div>
                  <span>Laba kotor periode ini</span>
                  <strong className={`insight-amount ${amountTone(data.totals.gross_profit)}`}>{exactMoney(data.totals.gross_profit)}</strong>
                </div>
                <div className="insight-mini-values">
                  <span>Pendapatan <b>{exactMoney(data.totals.revenue)}</b></span>
                  <span>HPP <b>{exactMoney(data.totals.cogs)}</b></span>
                </div>
              </div>
              <p className="insight-card-note">Pendapatan dan HPP bersih menurut bulan posting; koreksi ikut bulan pembalikan. Laba kotor = pendapatan − HPP, sebelum biaya lain.</p>
              <MonthlyChart series={data.series} metrics={revenueMetrics} label="Pendapatan dan harga pokok penjualan per bulan" resultKey="gross_profit" resultLabel="Laba kotor" />
            </article>
            <article className="panel insight-card">
              <div className="insight-card-heading">
                <div>
                  <span className="insight-kicker">UANG MASUK & KELUAR</span>
                  <h3>Mutasi kas</h3>
                  <p>Lihat pergerakan kas pada setiap bulan.</p>
                </div>
              </div>
              <div className="insight-result-row">
                <div>
                  <span>Perubahan kas periode ini</span>
                  <strong className={`insight-amount ${amountTone(data.totals.cash_net)}`}>{exactMoney(data.totals.cash_net)}</strong>
                </div>
                <div className="insight-mini-values">
                  <span><ArrowDownLeft size={12} aria-hidden="true" /> Masuk <b>{exactMoney(data.totals.cash_in)}</b></span>
                  <span><ArrowUpRight size={12} aria-hidden="true" /> Keluar <b>{exactMoney(data.totals.cash_out)}</b></span>
                </div>
              </div>
              <p className="insight-card-note">Termasuk modal, saldo awal, jurnal manual, dan koreksi. Kas masuk bukan hanya pembayaran pelanggan; perubahan ini bukan saldo kas akhir.</p>
              <MonthlyChart series={data.series} metrics={cashMetrics} label="Mutasi kas masuk dan keluar per bulan" resultKey="cash_net" resultLabel="Perubahan kas" />
            </article>
            <ReceivablesChart data={data.receivables} />
          </div>
        )}
      </div>
    </section>
  );
}
