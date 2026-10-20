import { useState } from "react";
import {
  BookOpen,
  CheckCheck,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  Scale,
} from "lucide-react";
import {
  EmptyState,
  ErrorState,
  Loading,
  PageHeader,
  SearchInput,
} from "./components";
import { date, money } from "./format";
import { useResource } from "./hooks";
import type { Account, Journal, TrialBalance, Role } from "./types";
import { ManualJournalForm, PeriodForm } from "./Corrections";
import { downloadCsv } from "./export";
import { AccountForm, Ledger } from "./Ledger";
import OpeningBalances from "./OpeningBalances";
import { DocumentReversal } from "./DocumentReversal";
import FinancialReports from "./FinancialReports";
import { AttachmentsPanel } from "./AttachmentsPanel";

const kindNames: Record<string, string> = {
  asset: "Aset",
  liability: "Liabilitas",
  equity: "Ekuitas",
  revenue: "Pendapatan",
  expense: "Beban",
};

export default function Accounting({
  role,
  changed,
}: {
  role: Role;
  changed: () => void;
}) {
  const [tab, setTab] = useState("trial");
  const [search, setSearch] = useState("");
  const [form, setForm] = useState<"journal" | "period" | "account" | null>(
    null,
  );
  const [revision, setRevision] = useState(0);
  const canEdit = ["admin", "finance"].includes(role);
  function saved() {
    setForm(null);
    setRevision((v) => v + 1);
    changed();
  }
  return (
    <>
      <PageHeader
        eyebrow="ANGKA YANG BISA DIPERCAYA"
        title="Akuntansi"
        description="Transaksi operasional terhubung langsung ke pembukuan Anda."
        actions={
          canEdit && (
            <>
              {role === "admin" && (
                <button
                  className="button secondary"
                  onClick={() => setForm("period")}
                >
                  Tutup periode
                </button>
              )}
              <button
                className="button secondary"
                onClick={() => setForm("account")}
              >
                Tambah akun
              </button>
              <button
                className="button primary"
                onClick={() => setForm("journal")}
              >
                Buat jurnal
              </button>
            </>
          )
        }
      />
      <div className="accounting-note">
        <span className="metric-icon green">
          <BookOpen size={21} />
        </span>
        <div>
          <strong>Pembukuan yang mengikuti alur bisnis Anda.</strong>
          <p>
            Penerimaan stok, pengiriman, invoice, dan pembayaran membuat jurnal
            secara otomatis.
          </p>
        </div>
        <span className="subtle-chip">Double-entry</span>
      </div>
      <section className="panel">
        <div className="table-toolbar">
          <div className="tabs" role="tablist" aria-label="Laporan akuntansi">
            {[
              ["trial", "Neraca saldo"],
              ["reports", "Laporan keuangan"],
              ["journals", "Jurnal umum"],
              ["accounts", "Daftar akun"],
              ["opening", "Saldo awal"],
            ].map(([key, name]) => (
              <button
                key={key}
                role="tab"
                aria-selected={tab === key}
                className={tab === key ? "active" : ""}
                onClick={() => {
                  setTab(key);
                  setSearch("");
                }}
              >
                {name}
              </button>
            ))}
          </div>
          {tab !== "opening" && tab !== "reports" && (
            <SearchInput
              value={search}
              onChange={setSearch}
              placeholder={
                tab === "journals"
                  ? "Cari keterangan jurnal…"
                  : "Cari nama atau kode akun…"
              }
            />
          )}
        </div>
        <div>
          {tab === "trial" ? (
            <TrialBalanceView search={search} revision={revision} />
          ) : tab === "journals" ? (
            <JournalsView
              search={search}
              revision={revision}
              canEdit={canEdit}
              changed={changed}
              clearSearch={() => setSearch("")}
            />
          ) : tab === "reports" ? (
            <FinancialReports role={role} />
          ) : tab === "opening" ? null : (
            <AccountsView search={search} revision={revision} />
          )}
        </div>
        <div hidden={tab !== "opening"}>
          <OpeningBalances role={role} changed={changed} />
        </div>
      </section>
      {form === "journal" && (
        <ManualJournalForm onClose={() => setForm(null)} onSaved={saved} />
      )}
      {form === "account" && (
        <AccountForm onClose={() => setForm(null)} onSaved={saved} />
      )}
      {form === "period" && (
        <PeriodForm onClose={() => setForm(null)} onSaved={saved} />
      )}
    </>
  );
}

function TrialBalanceView({
  search,
  revision,
}: {
  search: string;
  revision: number;
}) {
  const { data, loading, error, reload } = useResource<{
    items: TrialBalance[];
    total_debit: string;
    total_credit: string;
    balanced: boolean;
  }>("/accounting/trial-balance", revision);
  const items = (data?.items ?? []).filter((item) =>
    `${item.code} ${item.name}`.toLowerCase().includes(search.toLowerCase()),
  );
  if (loading) return <Loading />;
  if (error || !data) return <ErrorState message={error} onRetry={reload} />;
  return (
    <>
      <div className={`balance-banner ${data.balanced ? "" : "unbalanced"}`}>
        {data.balanced ? <CheckCheck size={19} /> : <CircleAlert size={19} />}
        <span>
          {data.balanced ? "Pembukuan seimbang" : "Pembukuan perlu diperiksa"}
        </span>
        <small>Akumulasi seluruh transaksi</small>
      </div>
      {items.length ? (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>Kode akun</th>
                <th>Nama akun</th>
                <th>Kelompok</th>
                <th className="align-right">Debit</th>
                <th className="align-right">Kredit</th>
              </tr>
            </thead>
            <tbody>
              {items.map((item) => (
                <tr key={item.code}>
                  <td className="mono muted">{item.code}</td>
                  <td className="font-medium">{item.name}</td>
                  <td>
                    <span className="account-kind">
                      {kindNames[item.kind ?? ""] ?? item.kind ?? "—"}
                    </span>
                  </td>
                  <td className="align-right">{money(item.debit)}</td>
                  <td className="align-right">{money(item.credit)}</td>
                </tr>
              ))}
            </tbody>
            {!search && (
              <tfoot>
                <tr>
                  <td colSpan={3}>
                    <Scale size={16} /> Total neraca saldo
                  </td>
                  <td className="align-right">{money(data.total_debit)}</td>
                  <td className="align-right">{money(data.total_credit)}</td>
                </tr>
              </tfoot>
            )}
          </table>
        </div>
      ) : (
        <EmptyState
          title="Akun tidak ditemukan"
          description="Coba kata kunci lain."
        />
      )}
      <div className="table-footer">
        <button
          className="text-button"
          onClick={() =>
            downloadCsv(
              "haruflow-neraca-saldo",
              ["Kode", "Akun", "Debit", "Kredit", "Saldo"],
              items.map((a) => [a.code, a.name, a.debit, a.credit, a.balance]),
            )
          }
        >
          Ekspor CSV
        </button>{" "}
        · Neraca saldo menampilkan total debit dan kredit yang sudah diposting.
      </div>
    </>
  );
}

function JournalsView({
  search,
  revision,
  canEdit,
  changed,
  clearSearch,
}: {
  search: string;
  revision: number;
  canEdit: boolean;
  changed: () => void;
  clearSearch: () => void;
}) {
  const { data, loading, error, reload } = useResource<{ items: Journal[] }>(
    "/accounting/journals",
    revision,
  );
  const [expanded, setExpanded] = useState<string | null>(null);
  const [reversing, setReversing] = useState<Journal | null>(null);
  function reveal(id: string) {
    clearSearch();
    setExpanded(id);
    requestAnimationFrame(() =>
      document
        .getElementById(`journal-${id}`)
        ?.scrollIntoView({ behavior: "smooth", block: "center" }),
    );
  }
  const items = (data?.items ?? []).filter((item) =>
    `${item.description} ${item.number ?? ""} ${item.reference ?? ""}`
      .toLowerCase()
      .includes(search.toLowerCase()),
  );
  if (loading) return <Loading />;
  if (error) return <ErrorState message={error} onRetry={reload} />;
  return items.length ? (
    <>
      <div className="journal-list">
        {items.map((journal) => (
          <div
            className="journal-entry"
            key={journal.id}
            id={`journal-${journal.id}`}
          >
            <button
              className="journal-toggle"
              onClick={() =>
                setExpanded(expanded === journal.id ? null : journal.id)
              }
              aria-expanded={expanded === journal.id}
            >
              <span className="journal-icon">
                <BookOpen size={18} />
              </span>
              <span className="journal-description">
                <strong>{journal.description}</strong>
                <small>
                  {journal.number ?? journal.source_type ?? "Jurnal otomatis"} ·{" "}
                  {date(journal.created_at)}
                  {journal.reversal && " · Sudah dibalik"}
                  {journal.reversed_source && " · Jurnal pembalikan"}
                </small>
              </span>
              <strong>
                {money(
                  journal.lines.reduce(
                    (sum, line) => sum + Number(line.debit),
                    0,
                  ),
                )}
              </strong>
              {expanded === journal.id ? (
                <ChevronDown size={17} />
              ) : (
                <ChevronRight size={17} />
              )}
            </button>
            {expanded === journal.id && (
              <>
                <div className="table-scroll journal-lines">
                  <table>
                    <thead>
                      <tr>
                        <th>Akun</th>
                        <th className="align-right">Debit</th>
                        <th className="align-right">Kredit</th>
                      </tr>
                    </thead>
                    <tbody>
                      {journal.lines.map((line, index) => (
                        <tr key={`${line.account_code}-${index}`}>
                          <td>
                            <span className="mono muted">
                              {line.account_code}
                            </span>{" "}
                            <span className="font-medium">
                              {line.account_name}
                            </span>
                          </td>
                          <td className="align-right">{money(line.debit)}</td>
                          <td className="align-right">{money(line.credit)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                <AttachmentsPanel
                  documentType="journal"
                  documentId={journal.id}
                  title="Lampiran jurnal"
                />
                <div className="correction-history">
                  {journal.reversal && (
                    <>
                      <p>
                        <strong>
                          Sudah dibalik pada {date(journal.reversal.created_at)}
                          .
                        </strong>{" "}
                        Alasan: {journal.reversal.reason}
                      </p>
                      {data?.items.some(
                        (item) => item.id === journal.reversal!.journal_id,
                      ) ? (
                        <button
                          className="text-button"
                          onClick={() => reveal(journal.reversal!.journal_id)}
                        >
                          Lihat jurnal pembalikan
                        </button>
                      ) : (
                        <p>
                          Jurnal pembalikan ·{" "}
                          <span className="mono">
                            {journal.reversal.journal_id.slice(0, 8)}
                          </span>
                        </p>
                      )}
                    </>
                  )}
                  {journal.reversed_source && (
                    <p>
                      Pembalikan transaksi asal:{" "}
                      {journal.reversed_source.source_type === "manual" &&
                      data?.items.some(
                        (item) =>
                          item.id === journal.reversed_source!.source_id,
                      ) ? (
                        <button
                          className="text-button"
                          onClick={() =>
                            reveal(journal.reversed_source!.source_id)
                          }
                        >
                          Lihat jurnal asal
                        </button>
                      ) : (
                        <span className="mono">
                          {journal.reversed_source.source_id.slice(0, 8)}
                        </span>
                      )}
                    </p>
                  )}
                  {canEdit && journal.can_reverse && (
                    <div className="correction-history-actions">
                      <button
                        className="text-button danger-text"
                        onClick={() => setReversing(journal)}
                      >
                        Balikkan jurnal
                      </button>
                      <span>Jurnal asal tetap tersimpan.</span>
                    </div>
                  )}
                  {canEdit &&
                    !journal.can_reverse &&
                    !journal.reversal &&
                    journal.source_type === "manual" &&
                    journal.reversal_blocked_reason && (
                      <p>{journal.reversal_blocked_reason}</p>
                    )}
                  {!journal.reversal &&
                    !journal.reversed_source &&
                    journal.source_type !== "manual" && (
                      <p>
                        Jurnal otomatis dikoreksi melalui transaksi asalnya.
                      </p>
                    )}
                </div>
              </>
            )}
          </div>
        ))}
      </div>
      {reversing && (
        <DocumentReversal
          journal={reversing}
          onClose={() => setReversing(null)}
          onSaved={() => {
            setReversing(null);
            reload();
            changed();
          }}
        />
      )}
    </>
  ) : (
    <EmptyState
      title={
        search ? "Jurnal tidak ditemukan" : "Jurnal Anda akan tercatat di sini."
      }
      description={
        search
          ? "Coba keterangan yang berbeda."
          : "Catat penerimaan stok atau proses penjualan untuk membuat jurnal pertama."
      }
    />
  );
}

function AccountsView({
  search,
  revision,
}: {
  search: string;
  revision: number;
}) {
  const [selected, setSelected] = useState<Account | null>(null);
  const { data, loading, error, reload } = useResource<{ items: Account[] }>(
    "/accounting/accounts",
    revision,
  );
  const items = (data?.items ?? []).filter((item) =>
    `${item.code} ${item.name}`.toLowerCase().includes(search.toLowerCase()),
  );
  if (loading) return <Loading />;
  if (error) return <ErrorState message={error} onRetry={reload} />;
  return items.length ? (
    <>
      <div className="table-scroll">
        <table>
          <thead>
            <tr>
              <th>Kode akun</th>
              <th>Nama akun</th>
              <th>Kelompok</th>
              <th>Sumber</th>
            </tr>
          </thead>
          <tbody>
            {items.map((account) => (
              <tr key={account.code}>
                <td className="mono muted">{account.code}</td>
                <td>
                  <button
                    className="table-link"
                    onClick={() => setSelected(account)}
                  >
                    {account.name}
                  </button>
                </td>
                <td>
                  <span className="account-kind">
                    {kindNames[account.kind] ?? account.kind}
                  </span>
                </td>
                <td>
                  <button
                    className="text-button"
                    onClick={() => setSelected(account)}
                  >
                    Buka buku besar
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="table-footer">
        Pilih nama akun untuk melihat buku besar dan transaksi penyusunnya.
      </div>
      {selected && (
        <Ledger account={selected} onClose={() => setSelected(null)} />
      )}
    </>
  ) : (
    <EmptyState
      title="Akun tidak ditemukan"
      description="Coba nama atau kode akun yang berbeda."
    />
  );
}
