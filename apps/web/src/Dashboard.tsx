import {
  ArrowDownLeft,
  ArrowRight,
  ArrowUpRight,
  Banknote,
  Boxes,
  CircleDollarSign,
  Package,
  Plus,
  ShoppingBag,
  Users,
  Wallet,
} from "lucide-react";
import {
  EmptyState,
  ErrorState,
  Loading,
  PageHeader,
  Status,
} from "./components";
import { date, money, number } from "./format";
import { useResource } from "./hooks";
import type { Dashboard as DashboardData, Session } from "./types";

export default function Dashboard({
  session,
  navigate,
  revision,
}: {
  session: Session;
  navigate: (page: string) => void;
  revision: number;
}) {
  const { data, loading, error, reload } = useResource<DashboardData>(
    "/dashboard",
    revision,
  );
  const today = new Intl.DateTimeFormat("id-ID", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: session.company.timezone || "Asia/Jakarta",
  }).format(new Date());
  if (loading) return <Loading />;
  if (error || !data) return <ErrorState message={error} onRetry={reload} />;
  const metrics = [
    {
      title: "Total penjualan",
      amount: data.revenue,
      icon: CircleDollarSign,
      note: "Dari invoice yang diterbitkan",
      tone: "green",
    },
    {
      title: "Piutang usaha",
      amount: data.outstanding,
      icon: Wallet,
      note: "Invoice yang belum lunas",
      tone: "amber",
    },
    {
      title: "Nilai persediaan",
      amount: data.inventory_value,
      icon: Boxes,
      note: `${number(data.products_count)} produk dalam katalog`,
      tone: "violet",
    },
    {
      title: "Kas & bank",
      amount: data.cash,
      icon: Banknote,
      note: "Pembayaran yang tercatat",
      tone: "blue",
    },
  ];
  const maxSales = Math.max(
    ...data.monthly_sales.map((item) => Number(item.total)),
    1,
  );
  const totalSales = data.monthly_sales.reduce(
    (total, item) => total + Number(item.total),
    0,
  );
  const canSell = ["admin", "sales"].includes(session.user.role);
  return (
    <>
      <PageHeader
        eyebrow="SEMUANYA DALAM SATU PANDANGAN"
        title={`Bisnis yang lebih terarah.`}
        description={`Selamat datang, ${session.user.name.split(" ")[0]}. Ini kabar bisnis Anda hari ini.`}
        actions={<span className="date-chip">{today}</span>}
      />
      <div className="metrics-grid">
        {metrics.map((metric) => (
          <article className="metric-card" key={metric.title}>
            <div className="metric-top">
              <span>{metric.title}</span>
              <span className={`metric-icon ${metric.tone}`}>
                <metric.icon size={19} strokeWidth={1.7} />
              </span>
            </div>
            <strong>{money(metric.amount)}</strong>
            <small>{metric.note}</small>
          </article>
        ))}
      </div>
      <div className="dashboard-middle">
        <section className="panel sales-chart">
          <div className="panel-heading">
            <div>
              <h2>Alur penjualan</h2>
              <p>Nilai invoice per bulan</p>
            </div>
            <span className="subtle-chip">6 bulan terakhir</span>
          </div>
          <div className="chart-summary">
            <strong>{money(totalSales)}</strong>
            <span>
              <span className="legend-dot" /> Penjualan tercatat
            </span>
          </div>
          {data.monthly_sales.length ? (
            <div
              className="bar-chart"
              role="img"
              aria-label={`Penjualan enam bulan terakhir ${money(totalSales)}`}
            >
              <div className="chart-grid">
                <span>{money(maxSales)}</span>
                <span>{money(maxSales / 2)}</span>
                <span>Rp0</span>
              </div>
              <div className="chart-bars">
                {data.monthly_sales.map((item, index) => (
                  <div className="chart-column" key={item.month}>
                    <div className="bar-track">
                      <div
                        className={`chart-bar ${index === data.monthly_sales.length - 1 ? "current" : ""}`}
                        style={{
                          height: `${(Number(item.total) / maxSales) * 100}%`,
                          minHeight: Number(item.total) > 0 ? 6 : 0,
                        }}
                        title={`${item.month}: ${money(item.total)}`}
                      />
                    </div>
                    <span>
                      {new Intl.DateTimeFormat("id-ID", {
                        month: "short",
                      }).format(new Date(`${item.month}-01T12:00:00`))}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <div className="chart-no-data">
              <span>Belum ada penjualan tercatat</span>
              <p>Grafik akan terisi setelah invoice pertama diterbitkan.</p>
            </div>
          )}
        </section>
        <section className="quick-start">
          <span className="eyebrow">LANGKAH KECIL, DAMPAK BESAR</span>
          <div className="quick-illustration" aria-hidden="true">
            <div>
              <Package size={31} strokeWidth={1.5} />
            </div>
            <span>
              <ArrowUpRight size={22} />
            </span>
          </div>
          <h2>
            Hari yang produktif
            <br />
            dimulai di sini.
          </h2>
          <p>
            Hubungkan pelanggan, produk, dan penjualan dalam satu alur yang
            rapi.
          </p>
          <button
            className="button dark"
            onClick={() => navigate(canSell ? "sales" : "inventory")}
          >
            {canSell ? (
              <>
                <Plus size={17} /> Buat penjualan
              </>
            ) : (
              <>
                <Package size={17} /> Lihat inventori
              </>
            )}
            <ArrowRight size={16} />
          </button>
        </section>
      </div>
      <section className="panel">
        <div className="panel-heading">
          <div>
            <h2>
              Penjualan terbaru{" "}
              <span className="count-pill">{data.orders_count}</span>
            </h2>
            <p>Ikuti setiap pesanan, dari draf hingga selesai.</p>
          </div>
          <button className="text-button" onClick={() => navigate("sales")}>
            Lihat semua <ArrowRight size={16} />
          </button>
        </div>
        {data.recent_orders.length ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Nomor pesanan</th>
                  <th>Pelanggan</th>
                  <th>Tanggal</th>
                  <th>Status</th>
                  <th className="align-right">Total</th>
                </tr>
              </thead>
              <tbody>
                {data.recent_orders.map((order) => (
                  <tr key={order.id}>
                    <td>
                      <button
                        className="table-link"
                        onClick={() => navigate("sales")}
                      >
                        {order.number}
                      </button>
                    </td>
                    <td>
                      <span className="customer-cell">
                        <span className="initial-avatar">
                          {order.customer_name.slice(0, 1).toUpperCase()}
                        </span>
                        {order.customer_name}
                      </span>
                    </td>
                    <td className="muted">{date(order.created_at)}</td>
                    <td>
                      <Status status={order.status} />
                    </td>
                    <td className="align-right font-medium">
                      {money(order.total)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <EmptyState
            title="Penjualan pertama Anda menunggu."
            description="Tambahkan pelanggan dan produk, lalu buat pesanan pertama Anda."
            action={
              <button
                className="button secondary"
                onClick={() => navigate("sales")}
              >
                Buka penjualan <ArrowRight size={16} />
              </button>
            }
          />
        )}
      </section>
      <div className="overview-strip">
        <button onClick={() => navigate("customers")}>
          <span className="overview-icon">
            <Users size={20} />
          </span>
          <div>
            <strong>{number(data.customers_count)} pelanggan</strong>
            <span>Hubungan yang terus bertumbuh</span>
          </div>
          <ArrowRight size={17} />
        </button>
        <button onClick={() => navigate("inventory")}>
          <span className="overview-icon">
            <Package size={20} />
          </span>
          <div>
            <strong>{number(data.products_count)} produk</strong>
            <span>Persediaan dalam kendali</span>
          </div>
          <ArrowRight size={17} />
        </button>
        <button onClick={() => navigate("accounting")}>
          <span className="overview-icon">
            <ShoppingBag size={20} />
          </span>
          <div>
            <strong>{number(data.orders_count)} pesanan</strong>
            <span>Terhubung ke pembukuan Anda</span>
          </div>
          <ArrowRight size={17} />
        </button>
      </div>
      <div className="page-bottom-note">
        <span>
          <ArrowDownLeft size={13} /> Data langsung dari ruang kerja Anda
        </span>
        <span>Haruflow Community · Berkembang bersama Anda</span>
      </div>
    </>
  );
}
