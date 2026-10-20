import { useRef, useState, type FormEvent } from "react";
import { Download, FileUp, History, Upload } from "lucide-react";
import { api, errorMessage } from "./api";
import {
  EmptyState,
  ErrorState,
  FormError,
  FormField,
  Loading,
  Pagination,
} from "./components";
import { dateTime } from "./format";
import { usePagination, useResource } from "./hooks";
import { downloadFile } from "./file-api";
import { downloadCsv } from "./export";
import { requestIdentity, type RequestIdentity } from "./purchasing-model";
import {
  csvFileError,
  decodeCsv,
  importColumns,
  importKinds,
  importNames,
  importStatus,
  type ImportBatch,
  type ImportKind,
} from "./imports-model";
import type { Role } from "./types";
import "./data-files.css";

export function ImportShortcut({ kind }: { kind: ImportKind }) {
  return (
    <a className="text-button" href={`?import=${kind}#settings`}>
      <FileUp size={15} /> Impor CSV
    </a>
  );
}

export default function DataImports({
  role,
  changed,
}: {
  role: Role;
  changed: () => void;
}) {
  const allowed = importKinds(role);
  const [kind, setKind] = useState<ImportKind>(() => {
    const requested = new URLSearchParams(location.search).get(
      "import",
    ) as ImportKind;
    return allowed.includes(requested)
      ? requested
      : (allowed[0] ?? "customers");
  });
  const [delimiter, setDelimiter] = useState("comma");
  const [file, setFile] = useState<File | null>(null);
  const [batch, setBatch] = useState<ImportBatch | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const identity = useRef<RequestIdentity | null>(null);
  const history = useResource<{ items: ImportBatch[] }>(
    `/imports?kind=${kind}`,
  );
  const pagination = usePagination(batch?.rows ?? [], batch?.id ?? "", 20);
  if (!allowed.length)
    return (
      <EmptyState
        title="Impor data"
        description="Impor tersedia bagi Administrator, Penjualan, dan Gudang sesuai data yang dikelola."
      />
    );
  async function work(action: () => Promise<void>) {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await action();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  async function preview(event: FormEvent) {
    event.preventDefault();
    setBatch(null);
    setConfirmed(false);
    await work(async () => {
      if (!file) throw new Error("Pilih file CSV lebih dahulu.");
      const issue = csvFileError(file);
      if (issue) throw new Error(issue);
      const csv = decodeCsv(await file.arrayBuffer());
      const result = await api<ImportBatch>(`/imports/${kind}/preview`, {
        method: "POST",
        body: { filename: file.name, csv, delimiter },
      });
      setBatch(result);
      setConfirmed(false);
      history.reload();
    });
  }
  async function commit() {
    if (!batch || !batch.can_commit || !confirmed) return;
    const id = batch.id;
    await work(async () => {
      identity.current = requestIdentity(identity.current, { import_id: id });
      const result = await api<ImportBatch>(`/imports/${id}/commit`, {
        method: "POST",
        body: { idempotency_key: identity.current.key },
      });
      setBatch(result);
      setConfirmed(false);
      history.reload();
      changed();
      setNotice(
        `${result.created_count ?? result.row_count} data berhasil ditambahkan.`,
      );
    });
  }
  return (
    <div className="settings-body data-imports">
      <div className="settings-section-heading">
        <div className="settings-big-icon">
          <FileUp size={25} />
        </div>
        <div>
          <h2>Impor data CSV</h2>
          <p>
            Tambahkan data usaha sekaligus setelah memeriksa hasil validasi.
          </p>
        </div>
      </div>
      <div className="info-box">
        <FileUp size={18} />
        <p>
          Impor hanya menambah data baru. Kode supplier, SKU produk, atau nama
          pelanggan yang sama ditolak. Seluruh baris harus valid sebelum satu
          batch dapat disimpan. Stok dan saldo awal diisi melalui menu Saldo
          awal.
        </p>
      </div>
      <form onSubmit={preview} className="import-upload-form">
        <div className="form-grid">
          <FormField label="Jenis data">
            <select
              value={kind}
              disabled={busy}
              onChange={(e) => {
                setKind(e.target.value as ImportKind);
                setBatch(null);
                setConfirmed(false);
                setError("");
                setNotice("");
              }}
            >
              {allowed.map((value) => (
                <option key={value} value={value}>
                  {importNames[value]}
                </option>
              ))}
            </select>
          </FormField>
          <FormField label="Pemisah kolom">
            <select
              value={delimiter}
              disabled={busy}
              onChange={(e) => {
                setDelimiter(e.target.value);
                setBatch(null);
                setConfirmed(false);
              }}
            >
              <option value="comma">Koma (,)</option>
              <option value="semicolon">Titik koma (;)</option>
            </select>
          </FormField>
        </div>
        <div className="data-file-actions">
          <button
            type="button"
            className="button secondary"
            disabled={busy}
            onClick={() =>
              void work(async () => {
                await downloadFile(
                  `/imports/${kind}/template?delimiter=${delimiter}`,
                  `haruflow-template-${kind}.csv`,
                );
              })
            }
          >
            <Download size={16} /> Unduh template{" "}
            {importNames[kind].toLowerCase()}
          </button>
        </div>
        <FormField
          label="File CSV UTF-8 *"
          hint="Maksimal 256 KiB dan 1.000 baris data. Gunakan angka desimal dengan titik, tanpa pemisah ribuan. Pilih pemisah sesuai file."
        >
          <input
            type="file"
            accept=".csv,text/csv"
            disabled={busy}
            onChange={(e) => {
              const next = e.target.files?.[0] ?? null;
              setFile(next);
              setBatch(null);
              setConfirmed(false);
              setError(next ? csvFileError(next) : "");
            }}
          />
        </FormField>
        {kind === "products" && (
          <p className="muted">
            Kolom kind: stock untuk barang atau service untuk jasa. Harga
            memakai titik desimal, contoh 25000.50, tanpa pemisah ribuan. Impor
            tidak menambahkan stok.
          </p>
        )}
        <button className="button primary" disabled={busy || !file}>
          <Upload size={16} />{" "}
          {busy ? "Memproses…" : "Validasi dan lihat preview"}
        </button>
      </form>
      <FormError error={error} />
      {notice && (
        <p role="status" className="data-file-success">
          {notice}
        </p>
      )}
      {batch && (
        <section className="import-preview" aria-label="Hasil validasi impor">
          <div className="data-file-section-head">
            <div>
              <h3>{batch.filename}</h3>
              <p>
                {importNames[batch.kind]} · {importStatus(batch.status)} ·{" "}
                {batch.valid_count} dari {batch.row_count} baris valid
              </p>
            </div>
            <span
              className={`status status-${batch.status === "committed" ? "paid" : batch.status === "ready" ? "confirmed" : "cancelled"}`}
            >
              <span />
              {importStatus(batch.status)}
            </span>
          </div>
          <p className="muted">
            Dibuat {dateTime(batch.created_at)}
            {batch.creator && ` oleh ${batch.creator.name}`}
            {batch.committed_at &&
              ` · Disimpan ${dateTime(batch.committed_at)}`}
          </p>
          {!!batch.errors?.length && (
            <div className="import-errors" role="status">
              <strong>
                {batch.errors.length} kesalahan ditemukan. Perbaiki file lalu
                buat preview baru.
              </strong>
              <ul>
                {batch.errors.slice(0, 30).map((issue, i) => (
                  <li key={i}>
                    Baris {issue.row}
                    {issue.column ? ` · ${issue.column}` : ""}: {issue.message}
                  </li>
                ))}
              </ul>
              {batch.errors.length > 30 && (
                <p>
                  Menampilkan 30 kesalahan pertama. Unduh daftar untuk melihat
                  semuanya.
                </p>
              )}
              <button
                className="text-button"
                onClick={() =>
                  downloadCsv(
                    "haruflow-kesalahan-impor",
                    ["Baris", "Kolom", "Kesalahan"],
                    batch.errors.map((issue) => [
                      issue.row,
                      issue.column,
                      issue.message,
                    ]),
                  )
                }
              >
                Unduh semua kesalahan CSV
              </button>
            </div>
          )}
          {!!batch.rows?.length && (
            <>
              <div className="table-scroll">
                <table>
                  <thead>
                    <tr>
                      <th>Baris</th>
                      {importColumns[batch.kind].map((column) => (
                        <th key={column}>{column}</th>
                      ))}
                      <th>Validasi</th>
                    </tr>
                  </thead>
                  <tbody>
                    {pagination.items.map((row) => (
                      <tr key={row.row}>
                        <td>{row.row}</td>
                        {importColumns[batch.kind].map((column) => (
                          <td key={column} className="import-cell">
                            {row.data[column] || "—"}
                          </td>
                        ))}
                        <td>
                          {row.errors.length
                            ? row.errors
                                .map((issue) => issue.message)
                                .join("; ")
                            : "Valid"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <Pagination {...pagination} />
            </>
          )}
          {batch.status === "ready" &&
            (batch.can_commit ? (
              <div className="import-commit">
                <label className="checkbox-field">
                  <input
                    type="checkbox"
                    checked={confirmed}
                    disabled={busy}
                    onChange={(e) => setConfirmed(e.target.checked)}
                  />{" "}
                  Saya sudah memeriksa {batch.row_count} baris dan ingin
                  menambahkan semuanya.
                </label>
                <p>
                  Jika ada data yang berubah atau menjadi duplikat sebelum
                  penyimpanan, seluruh batch ditolak. Buat preview baru setelah
                  memperbaiki file.
                </p>
                <button
                  className="button primary"
                  disabled={busy || !confirmed}
                  onClick={() => void commit()}
                >
                  {busy
                    ? "Menyimpan…"
                    : `Simpan ${batch.row_count} ${importNames[batch.kind].toLowerCase()}`}
                </button>
              </div>
            ) : (
              <p className="muted">
                Hanya pembuat preview yang dapat menyimpan batch ini.
              </p>
            ))}
        </section>
      )}
      <section className="import-history">
        <h3>
          <History size={18} /> Riwayat impor {importNames[kind].toLowerCase()}
        </h3>
        <p className="muted">
          50 batch terbaru, termasuk preview yang perlu diperbaiki.
        </p>
        {history.loading ? (
          <Loading />
        ) : history.error ? (
          <ErrorState message={history.error} onRetry={history.reload} />
        ) : history.data?.items.length ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>File</th>
                  <th>Dibuat</th>
                  <th>Pembuat</th>
                  <th>Baris</th>
                  <th>Status</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {history.data.items.map((item) => (
                  <tr key={item.id}>
                    <td className="import-cell">{item.filename}</td>
                    <td>{dateTime(item.created_at)}</td>
                    <td>{item.creator?.name ?? "—"}</td>
                    <td>{item.row_count}</td>
                    <td>{importStatus(item.status)}</td>
                    <td>
                      <button
                        className="text-button"
                        disabled={busy}
                        onClick={() =>
                          void work(async () => {
                            const detail = await api<ImportBatch>(
                              `/imports/${item.id}`,
                            );
                            setBatch(detail);
                            setConfirmed(false);
                          })
                        }
                      >
                        Lihat detail
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <EmptyState
            title="Belum ada riwayat impor"
            description="Unduh template dan validasi file pertama Anda."
          />
        )}
      </section>
    </div>
  );
}
