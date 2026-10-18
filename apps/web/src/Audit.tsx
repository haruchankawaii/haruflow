import { useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Download,
  History,
  RefreshCw,
} from "lucide-react";
import {
  EmptyState,
  ErrorState,
  Loading,
  Pagination,
  SearchInput,
} from "./components";
import { usePagination, useResource } from "./hooks";
import { downloadCsv } from "./export";
import { dateTime, getCompanyTimezone } from "./format";

interface AuditEvent {
  id: string;
  actor_id: string | null;
  actor_name: string | null;
  actor_email: string | null;
  action: string;
  resource_type: string;
  resource_id: string | null;
  details: unknown;
  created_at: string;
}

const actionNames: Record<string, string> = {
  "company.setup": "Perusahaan disiapkan",
  "company.profile_updated": "Profil perusahaan diperbarui",
  "user.password_reset_local": "Kata sandi direset melalui server",
  "auth.login": "Pengguna masuk",
  "auth.logout": "Pengguna keluar",
  "user.create": "Pengguna ditambahkan",
  "user.update": "Akses pengguna diperbarui",
  "user.password_reset": "Kata sandi direset administrator",
  "user.password_change": "Kata sandi diperbarui",
  "customer.created": "Pelanggan ditambahkan",
  "customer.updated": "Pelanggan diperbarui",
  "customer.deleted": "Pelanggan dihapus",
  "product.created": "Produk ditambahkan",
  "product.updated": "Produk diperbarui",
  "product.deleted": "Produk dihapus",
  "inventory.received": "Stok diterima",
  "inventory.adjusted": "Stok disesuaikan",
  "order.created": "Pesanan dibuat",
  "order.confirmed": "Pesanan dikonfirmasi",
  "order.shipped": "Pesanan dikirim",
  "order.cancelled": "Pesanan dibatalkan",
  "invoice.posted": "Invoice diposting",
  "invoice.payment_received": "Pembayaran diterima",
  "journal.posted": "Jurnal diposting",
  "period.closed": "Periode akuntansi ditutup",
  "payment.reversed": "Pembayaran dibalik",
  "invoice.reversed": "Invoice dibalik",
  "shipment.reversed": "Pengiriman dibalik",
  "account.created": "Akun pembukuan ditambahkan",
};

const resourceNames: Record<string, string> = {
  company: "Perusahaan",
  user: "Pengguna",
  customer: "Pelanggan",
  product: "Produk",
  inventory_movement: "Pergerakan stok",
  sales_order: "Pesanan penjualan",
  invoice: "Invoice",
  payment: "Pembayaran",
  journal_entry: "Jurnal",
  accounting_period: "Periode akuntansi",
  account: "Akun pembukuan",
};

function detailsText(details: unknown): string {
  return JSON.stringify(details ?? {}, null, 2) ?? "{}";
}

export default function Audit() {
  const { data, loading, error, reload } = useResource<{ items: AuditEvent[] }>(
    "/audit-events",
  );
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);
  const indexed = useMemo(
    () =>
      (data?.items ?? []).map((event) => ({
        event,
        text: [
          event.actor_name,
          event.actor_email,
          event.action,
          actionNames[event.action],
          event.resource_type,
          resourceNames[event.resource_type],
          event.resource_id,
          detailsText(event.details),
        ]
          .filter(Boolean)
          .join(" ")
          .toLocaleLowerCase("id-ID"),
      })),
    [data],
  );
  const query = search.trim().toLocaleLowerCase("id-ID");
  const filtered = indexed
    .filter((item) => item.text.includes(query))
    .map((item) => item.event);
  const pagination = usePagination(filtered, search);
  function exportEvents() {
    downloadCsv(
      "haruflow-aktivitas",
      [
        `Waktu (${getCompanyTimezone()})`,
        "Pengguna",
        "Email",
        "Aktivitas",
        "Kode aktivitas",
        "Objek",
        "ID objek",
        "Detail",
      ],
      filtered.map((event) => [
        dateTime(event.created_at),
        event.actor_name || "Sistem",
        event.actor_email,
        actionNames[event.action] ?? event.action,
        event.action,
        resourceNames[event.resource_type] ?? event.resource_type,
        event.resource_id,
        JSON.stringify(event.details ?? {}),
      ]),
    );
  }
  return (
    <>
      <div className="table-toolbar">
        <span className="toolbar-title">
          <History size={18} aria-hidden="true" /> Jejak aktivitas
        </span>
        <button
          className="icon-button"
          type="button"
          onClick={reload}
          disabled={loading}
          aria-label="Muat ulang aktivitas"
          title="Muat ulang aktivitas"
        >
          <RefreshCw size={16} className={loading ? "spin" : ""} />
        </button>
        <button
          className="text-button export-button"
          type="button"
          onClick={exportEvents}
          disabled={loading || Boolean(error) || !filtered.length}
        >
          <Download size={15} aria-hidden="true" /> Ekspor CSV
        </button>
        <SearchInput
          value={search}
          onChange={setSearch}
          placeholder="Cari pengguna, aktivitas, atau objek…"
        />
      </div>
      {loading ? (
        <Loading label="Memuat jejak aktivitas…" />
      ) : error ? (
        <ErrorState message={error} onRetry={reload} />
      ) : filtered.length ? (
        <div className="journal-list">
          {pagination.items.map((event) => (
            <div className="journal-entry" key={event.id}>
              <button
                className="journal-toggle"
                type="button"
                onClick={() =>
                  setExpanded(expanded === event.id ? null : event.id)
                }
                aria-expanded={expanded === event.id}
                aria-controls={
                  expanded === event.id ? `audit-${event.id}` : undefined
                }
              >
                <span className="journal-icon">
                  <History size={17} aria-hidden="true" />
                </span>
                <span className="journal-description">
                  <strong>{actionNames[event.action] ?? event.action}</strong>
                  <small>
                    {event.actor_name || "Sistem"} ·{" "}
                    {resourceNames[event.resource_type] ?? event.resource_type}{" "}
                    ·{" "}
                    <time dateTime={event.created_at}>
                      {dateTime(event.created_at)}
                    </time>
                  </small>
                </span>
                {expanded === event.id ? (
                  <ChevronDown size={17} aria-hidden="true" />
                ) : (
                  <ChevronRight size={17} aria-hidden="true" />
                )}
              </button>
              {expanded === event.id && (
                <div className="audit-detail" id={`audit-${event.id}`}>
                  <dl>
                    <div>
                      <dt>Pengguna</dt>
                      <dd>
                        {event.actor_name || "Sistem"}
                        {event.actor_email ? ` · ${event.actor_email}` : ""}
                      </dd>
                    </div>
                    <div>
                      <dt>Aktivitas</dt>
                      <dd>{event.action}</dd>
                    </div>
                    <div>
                      <dt>ID objek</dt>
                      <dd>{event.resource_id || "—"}</dd>
                    </div>
                    <div>
                      <dt>Waktu</dt>
                      <dd>
                        <time dateTime={event.created_at}>
                          {dateTime(event.created_at)}
                        </time>
                      </dd>
                    </div>
                  </dl>
                  <pre aria-label="Rincian perubahan">
                    {detailsText(event.details)}
                  </pre>
                </div>
              )}
            </div>
          ))}
        </div>
      ) : (
        <EmptyState
          title={query ? "Aktivitas tidak ditemukan" : "Belum ada aktivitas"}
          description={
            query
              ? "Coba pengguna, aktivitas, atau ID objek yang berbeda."
              : "Perubahan dan akses penting akan tercatat di sini."
          }
        />
      )}
      <div className="table-footer">
        {!loading && !error && <Pagination {...pagination} />}
        <small>
          Pencarian dan ekspor mencakup hingga 200 aktivitas terbaru.
        </small>
      </div>
    </>
  );
}
