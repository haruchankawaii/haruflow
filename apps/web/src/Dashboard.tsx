import {
  ArrowDownLeft,
  ArrowRight,
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
import DashboardCharts from "./DashboardCharts";
import { exactMoney } from "./dashboard-charts";
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
      title: "Pendapatan tercatat",
      amount: data.revenue,
      icon: CircleDollarSign,
      note: "Akumulasi pendapatan bersih · seluruh waktu",
      tone: "green",
    },
    {
      title: "Piutang usaha",
      amount: data.outstanding,
      icon: Wallet,
      note: "Saldo piutang pembukuan, termasuk saldo awal",
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
      note: "Saldo pembukuan saat ini · seluruh waktu",
      tone: "blue",
    },
  ];
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
            <strong>{exactMoney(metric.amount)}</strong>
            <small>{metric.note}</small>
          </article>
        ))}
      </div>
      <DashboardCharts revision={revision} />
      <div className="dashboard-action-row">
        <section className="quick-start">
          <span className="eyebrow">LANGKAH KECIL, DAMPAK BESAR</span>
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
