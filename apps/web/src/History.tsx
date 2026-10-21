import { useContext, useState, type FormEvent } from "react";
import { Download, Search, ChevronLeft, ChevronRight } from "lucide-react";
import {
  EmptyState,
  ErrorState,
  Loading,
  PageHeader,
  FormError,
} from "./components";
import { SessionRoleContext, useResource } from "./hooks";
import { errorMessage } from "./api";
import { dateTime, money } from "./format";
import { downloadFile } from "./file-api";
import { JournalPreview } from "./JournalPreview";
import {
  emptyHistoryFilters,
  historyKinds,
  historyQuery,
  historyStatuses,
  historyStatusLabel,
} from "./history-model";

interface RecordRow {
  id: string;
  label: string;
  party: string;
  amount: string | null;
  status: string;
  created_at: string;
  url: string | null;
}
interface Records {
  items: RecordRow[];
  pagination: { page: number; page_size: number; total: number; pages: number };
}

export default function History() {
  const role = useContext(SessionRoleContext);
  const [kind, setKind] = useState("sales-orders"),
    [page, setPage] = useState(1);
  const [filters, setFilters] = useState(emptyHistoryFilters),
    [applied, setApplied] = useState(emptyHistoryFilters);
  const [exporting, setExporting] = useState(false),
    [error, setError] = useState("");
  const [journal, setJournal] = useState<string | null>(null);
  const resource = useResource<Records>(
    `/records/${kind}?${historyQuery(applied, page)}`,
  );
  const data = resource.data;
  function search(event: FormEvent) {
    event.preventDefault();
    setPage(1);
    setApplied({ ...filters });
    setError("");
  }
  async function exportAll() {
    setExporting(true);
    setError("");
    try {
      await downloadFile(
        `/records/${kind}/export?${historyQuery(applied, 1, true)}`,
        `haruflow-${kind}.csv`,
      );
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setExporting(false);
    }
  }
  return (
    <>
      <PageHeader
        eyebrow="TELUSURI SETIAP CATATAN"
        title="Riwayat lengkap"
        description="Cari catatan lama melalui server. Hasil pencarian dan ekspor mencakup seluruh riwayat sesuai filter."
        actions={
          <button
            className="button secondary"
            onClick={exportAll}
            disabled={exporting || resource.loading}
          >
            <Download size={16} />
            {exporting ? "Menyiapkan CSV…" : "Ekspor seluruh hasil"}
          </button>
        }
      />
      <section className="settings-card">
        <form className="settings-body" onSubmit={search}>
          <div className="form-grid">
            <label className="form-field">
              <span>Jenis catatan</span>
              <select
                value={kind}
                onChange={(event) => {
                  setKind(event.target.value);
                  setPage(1);
                  setFilters(emptyHistoryFilters);
                  setApplied(emptyHistoryFilters);
                  setError("");
                }}
              >
                {historyKinds
                  .filter(
                    ([key]) =>
                      key !== "audit" || ["admin", "finance"].includes(role),
                  )
                  .map(([key, name]) => (
                    <option key={key} value={key}>
                      {name}
                    </option>
                  ))}
              </select>
            </label>
            <label className="form-field">
              <span>Nomor, nama, atau referensi</span>
              <input
                value={filters.q}
                maxLength={200}
                onChange={(e) => setFilters({ ...filters, q: e.target.value })}
                placeholder="Cari seluruh riwayat…"
              />
            </label>
            {historyStatuses(kind).length > 0 && <label className="form-field">
              <span>Status / jenis</span>
              <select value={filters.status} onChange={event => setFilters({ ...filters, status: event.target.value })}>
                <option value="">Semua status / jenis</option>
                {historyStatuses(kind).map(status => <option key={status} value={status}>{historyStatusLabel(status)}</option>)}
              </select>
            </label>}
            <label className="form-field">
              <span>Mulai tanggal pencatatan</span>
              <input
                type="date"
                value={filters.from}
                onChange={(e) =>
                  setFilters({ ...filters, from: e.target.value })
                }
              />
            </label>
            <label className="form-field">
              <span>Sampai tanggal pencatatan</span>
              <input
                type="date"
                value={filters.to}
                min={filters.from || undefined}
                onChange={(e) => setFilters({ ...filters, to: e.target.value })}
              />
            </label>
          </div>
          <button className="button primary" type="submit">
            <Search size={16} />
            Cari catatan
          </button>
          <p className="muted">
            Tanggal mengikuti zona waktu perusahaan. Ekspor maksimal 10.000
            baris per pencarian; persempit filter bila melebihi batas.
          </p>
          <FormError error={error} />
        </form>
        {resource.loading ? (
          <Loading label="Menelusuri riwayat…" />
        ) : resource.error ? (
          <ErrorState message={resource.error} onRetry={resource.reload} />
        ) : !data?.items.length ? (
          <EmptyState
            title="Tidak ada catatan yang cocok"
            description="Coba kata pencarian atau rentang tanggal lain."
          />
        ) : (
          <>
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Dokumen / catatan</th>
                    <th>Pihak / keterangan</th>
                    <th>Tanggal</th>
                    <th>Nominal / sisa</th>
                    <th>Status / jenis</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {data.items.map((row) => (
                    <tr key={row.id}>
                      <td>
                        <strong>{row.label}</strong>
                      </td>
                      <td>{row.party || "—"}</td>
                      <td>{dateTime(row.created_at)}</td>
                      <td>{row.amount === null ? "—" : money(row.amount)}</td>
                      <td>
                        <span className={`status status-${row.status}`}><span />{historyStatusLabel(row.status)}</span>
                      </td>
                      <td>
                        {kind === "journals" ? (
                          <button
                            className="button small secondary"
                            onClick={() => setJournal(row.id)}
                          >
                            Lihat jurnal
                          </button>
                        ) : row.url ? (
                          <a className="button small secondary" href={row.url}>
                            Buka dokumen
                          </a>
                        ) : null}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="table-toolbar">
              <p>
                {data.pagination.total.toLocaleString("id-ID")} catatan ·
                Halaman {page} dari {data.pagination.pages}
              </p>
              <div className="button-group">
                <button
                  className="button secondary"
                  disabled={page <= 1}
                  onClick={() => setPage(page - 1)}
                >
                  <ChevronLeft size={16} />
                  Sebelumnya
                </button>
                <button
                  className="button secondary"
                  disabled={page >= data.pagination.pages}
                  onClick={() => setPage(page + 1)}
                >
                  Berikutnya
                  <ChevronRight size={16} />
                </button>
              </div>
            </div>
          </>
        )}
      </section>
      {journal && (
        <JournalPreview id={journal} onClose={() => setJournal(null)} />
      )}
    </>
  );
}
