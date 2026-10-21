import { useRef, useState } from "react";
import { Download, FileUp, Plus, Trash2 } from "lucide-react";
import { api, errorMessage } from "./api";
import {
  ErrorState,
  FormError,
  FormField,
  Loading,
  Pagination,
} from "./components";
import { exactMoney } from "./dashboard-charts";
import { downloadCsv } from "./export";
import { downloadFile } from "./file-api";
import { companyToday } from "./format";
import { usePagination, useResource } from "./hooks";
import { csvFileError, decodeCsv } from "./imports-model";
import {
  mergeOpeningParties,
  openingPartyCustomer,
  openingPartyNames,
  openingPartyPreview,
  openingPartySide,
  type OpeningParty,
  type OpeningPartyCsvPreview,
  type OpeningPartyKind,
} from "./opening-parties";
import type { Customer } from "./types";
import type { Supplier } from "./purchasing-model";
import "./party-balances.css";

export function OpeningPartiesEditor({
  parties,
  onChange,
}: {
  parties: OpeningParty[];
  onChange: (parties: OpeningParty[]) => void;
}) {
  const customers = useResource<{ items: Customer[] }>("/customers");
  const suppliers = useResource<{ items: Supplier[] }>("/suppliers");
  const [file, setFile] = useState<File | null>(null);
  const [delimiter, setDelimiter] = useState("comma");
  const [preview, setPreview] = useState<OpeningPartyCsvPreview | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const pagination = usePagination(parties, "", 20);
  const validation = openingPartyPreview(parties);
  const stateRef = useRef(parties);
  stateRef.current = parties;
  function change(index: number, patch: Partial<OpeningParty>) {
    setPreview(null);
    onChange(
      parties.map((row, i) => (i === index ? { ...row, ...patch } : row)),
    );
  }
  async function validateCsv() {
    if (!file || busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setError("");
    setPreview(null);
    try {
      const issue = csvFileError(file);
      if (issue) throw new Error(issue);
      const csv = decodeCsv(await file.arrayBuffer());
      const result = await api<OpeningPartyCsvPreview>(
        "/accounting/opening/parties/preview",
        { method: "POST", body: { csv, delimiter } },
      );
      setPreview(result);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  async function template() {
    setError("");
    try {
      await downloadFile(
        `/accounting/opening/parties/template?delimiter=${delimiter}`,
        "haruflow-template-saldo-pihak.csv",
      );
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  return (
    <section className="opening-party-editor">
      <div className="opening-section-heading">
        <div>
          <h3>Piutang, utang, dan saldo kredit lama</h3>
          <p>
            Isi sisa tagihan atau dana yang belum selesai saat mulai memakai
            Haruflow. Tanggal lama hanya untuk penelusuran dan aging; pembukaan
            tidak mengulang pendapatan, beban, atau kas historis.
          </p>
        </div>
      </div>
      {customers.loading || suppliers.loading ? (
        <Loading />
      ) : customers.error || suppliers.error ? (
        <ErrorState
          message={customers.error || suppliers.error}
          onRetry={() => {
            customers.reload();
            suppliers.reload();
          }}
        />
      ) : (
        <>
          {pagination.items.map((row, offset) => {
            const index = (pagination.current - 1) * 20 + offset;
            const customer = openingPartyCustomer(row.kind);
            const choices = customer
              ? (customers.data?.items ?? []).map((party) => ({
                  id: party.id,
                  label: party.name,
                }))
              : (suppliers.data?.items ?? []).map((party) => ({
                  id: party.id,
                  label: `${party.code} · ${party.name}`,
                }));
            return (
              <div className="opening-party-row" key={index}>
                <FormField label={`Jenis saldo ${index + 1}`}>
                  <select
                    value={row.kind}
                    onChange={(e) =>
                      change(index, {
                        kind: e.target.value as OpeningPartyKind,
                        party_id: "",
                        party_name: "",
                      })
                    }
                  >
                    {Object.entries(openingPartyNames).map(([value, label]) => (
                      <option key={value} value={value}>
                        {label}
                      </option>
                    ))}
                  </select>
                </FormField>
                <FormField label={customer ? "Pelanggan *" : "Supplier *"}>
                  <select
                    required
                    value={row.party_id}
                    onChange={(e) =>
                      change(index, {
                        party_id: e.target.value,
                        party_name: choices.find(
                          (party) => party.id === e.target.value,
                        )?.label,
                      })
                    }
                  >
                    <option value="">Pilih pihak</option>
                    {choices.map((party) => (
                      <option key={party.id} value={party.id}>
                        {party.label}
                      </option>
                    ))}
                  </select>
                </FormField>
                <FormField label="Referensi tagihan/saldo *">
                  <input
                    required
                    maxLength={200}
                    value={row.reference}
                    onChange={(e) =>
                      change(index, { reference: e.target.value })
                    }
                  />
                </FormField>
                <FormField label="Tanggal dokumen lama *">
                  <input
                    required
                    type="date"
                    max={companyToday()}
                    min="0001-01-01"
                    value={row.document_date}
                    onChange={(e) =>
                      change(index, { document_date: e.target.value })
                    }
                  />
                </FormField>
                <FormField
                  label="Jatuh tempo"
                  hint="Biarkan kosong bila tidak diketahui."
                >
                  <input
                    type="date"
                    min={row.document_date || "0001-01-01"}
                    max="9999-12-31"
                    value={row.due_date ?? ""}
                    onChange={(e) =>
                      change(index, { due_date: e.target.value || null })
                    }
                  />
                </FormField>
                <FormField
                  label={`Sisa saldo (Rp) · ${openingPartySide(row.kind) === "debit" ? "Debit" : "Kredit"}`}
                >
                  <input
                    required
                    inputMode="decimal"
                    value={row.amount}
                    onChange={(e) => change(index, { amount: e.target.value })}
                    placeholder="Contoh: 2000000.00"
                  />
                </FormField>
                <button
                  type="button"
                  className="icon-button danger-text"
                  aria-label={`Hapus saldo pihak ${index + 1}`}
                  onClick={() => {
                    onChange(parties.filter((_, i) => i !== index));
                    setPreview(null);
                  }}
                >
                  <Trash2 size={16} />
                </button>
              </div>
            );
          })}
          {!!parties.length && <Pagination {...pagination} />}
          <button
            type="button"
            className="text-button"
            disabled={parties.length >= 1000}
            onClick={() => {
              onChange([
                ...parties,
                {
                  kind: "receivable",
                  party_id: "",
                  reference: "",
                  document_date: companyToday(),
                  due_date: null,
                  amount: "",
                },
              ]);
              pagination.go(Math.ceil((parties.length + 1) / 20));
              setPreview(null);
            }}
          >
            <Plus size={16} /> Tambah saldo pihak
          </button>
          <div className="party-inline-summary">
            <span>
              Debit pihak <strong>{exactMoney(validation.debit)}</strong>
            </span>
            <span>
              Kredit pihak <strong>{exactMoney(validation.credit)}</strong>
            </span>
            <span>{parties.length}/1.000 rincian</span>
          </div>
          {!!validation.errors.length && (
            <FormError error={validation.errors.slice(0, 3).join(" ")} />
          )}
          <details className="opening-party-csv">
            <summary>
              <FileUp size={16} /> Impor rincian saldo awal dari CSV
            </summary>
            <p>
              CSV maksimal 256 KiB. Preview memeriksa semua baris. Tambahkan
              seluruh hasil valid ke draf, lalu simpan dan posting melalui
              pemeriksaan saldo awal.
            </p>
            <div className="party-form-grid">
              <FormField label="Pemisah kolom">
                <select
                  value={delimiter}
                  disabled={busy}
                  onChange={(e) => {
                    setDelimiter(e.target.value);
                    setPreview(null);
                  }}
                >
                  <option value="comma">Koma (,)</option>
                  <option value="semicolon">Titik koma (;)</option>
                </select>
              </FormField>
              <FormField label="File CSV UTF-8">
                <input
                  type="file"
                  accept=".csv,text/csv"
                  disabled={busy}
                  onChange={(e) => {
                    setFile(e.target.files?.[0] ?? null);
                    setPreview(null);
                  }}
                />
              </FormField>
            </div>
            <p className="muted">
              Gunakan jenis receivable/payable/customer_credit/supplier_credit.
              Kolom party memakai nama pelanggan atau kode supplier; party_id
              dapat diisi dari daftar referensi. Isi debit untuk piutang/kredit
              supplier, kredit untuk utang/kredit pelanggan. Harga memakai titik
              desimal tanpa pemisah ribuan.
            </p>
            <div className="data-file-actions">
              <button
                type="button"
                className="button secondary"
                disabled={busy}
                onClick={() => void template()}
              >
                <Download size={15} /> Unduh template saldo pihak
              </button>
              <button
                type="button"
                className="text-button"
                onClick={() =>
                  downloadCsv(
                    "haruflow-referensi-pihak",
                    ["Jenis", "party_id", "Nama", "Kode supplier"],
                    [
                      ...(customers.data?.items ?? []).map((party) => [
                        "customer",
                        party.id,
                        party.name,
                        "",
                      ]),
                      ...(suppliers.data?.items ?? []).map((party) => [
                        "supplier",
                        party.id,
                        party.name,
                        party.code,
                      ]),
                    ],
                  )
                }
              >
                Unduh daftar referensi pihak
              </button>
              <button
                type="button"
                className="button secondary"
                disabled={busy || !file}
                onClick={() => void validateCsv()}
              >
                {busy ? "Memvalidasi…" : "Validasi dan preview CSV"}
              </button>
            </div>
            <FormError error={error} />
            {preview && (
              <div className="import-preview">
                <h4>
                  {preview.valid_count} baris valid · {preview.errors.length}{" "}
                  kesalahan
                </h4>
                {preview.errors.length ? (
                  <>
                    <ul>
                      {preview.errors.slice(0, 20).map((issue, i) => (
                        <li key={i}>
                          Baris {issue.row} · {issue.column}: {issue.message}
                        </li>
                      ))}
                    </ul>
                    <button
                      type="button"
                      className="text-button"
                      onClick={() =>
                        downloadCsv(
                          "haruflow-kesalahan-saldo-awal",
                          ["Baris", "Kolom", "Kesalahan"],
                          preview.errors.map((issue) => [
                            issue.row,
                            issue.column,
                            issue.message,
                          ]),
                        )
                      }
                    >
                      Unduh seluruh kesalahan
                    </button>
                  </>
                ) : (
                  <>
                    <OpeningPartiesSnapshot
                      parties={preview.parties.slice(0, 20)}
                    />
                    {preview.parties.length > 20 && (
                      <p>
                        Menampilkan 20 dari {preview.parties.length} baris.
                        Seluruh baris valid akan ditambahkan.
                      </p>
                    )}
                    <button
                      type="button"
                      className="button primary"
                      onClick={() => {
                        try {
                          onChange(
                            mergeOpeningParties(stateRef.current, preview),
                          );
                          setPreview(null);
                          setError("");
                        } catch (e) {
                          setError(errorMessage(e));
                        }
                      }}
                    >
                      Tambahkan semua {preview.parties.length} rincian ke draf
                    </button>
                    <p className="muted">
                      Tindakan ini hanya mengisi draf lokal. Pembukuan belum
                      berubah.
                    </p>
                  </>
                )}
              </div>
            )}
          </details>
        </>
      )}
    </section>
  );
}

export function OpeningPartiesSnapshot({
  parties,
}: {
  parties: OpeningParty[];
}) {
  return parties.length ? (
    <div className="table-scroll">
      <table>
        <thead>
          <tr>
            <th>Jenis / pihak</th>
            <th>Referensi</th>
            <th>Tanggal dokumen</th>
            <th>Jatuh tempo</th>
            <th className="align-right">Debit</th>
            <th className="align-right">Kredit</th>
          </tr>
        </thead>
        <tbody>
          {parties.map((row, i) => (
            <tr key={row.id ?? i}>
              <td>
                {openingPartyNames[row.kind]}
                <small className="block muted">
                  {row.party_name ?? row.party_id}
                </small>
              </td>
              <td>{row.reference}</td>
              <td>{row.document_date}</td>
              <td>{row.due_date || "Belum diketahui"}</td>
              <td className="align-right">
                {openingPartySide(row.kind) === "debit"
                  ? exactMoney(row.amount)
                  : "—"}
              </td>
              <td className="align-right">
                {openingPartySide(row.kind) === "credit"
                  ? exactMoney(row.amount)
                  : "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  ) : null;
}
