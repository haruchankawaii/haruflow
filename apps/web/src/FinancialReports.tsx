import { useRef, useState, type FormEvent } from "react";
import { CheckCheck, CircleAlert, Download, Filter } from "lucide-react";
import { ApiError, api, errorMessage } from "./api";
import {
  EmptyState,
  ErrorState,
  FormActions,
  FormError,
  FormField,
  Loading,
  Modal,
  Pagination,
} from "./components";
import { calendarDate, exactMoney, moneyCents } from "./dashboard-charts";
import { downloadCsv } from "./export";
import { companyToday, dateTime } from "./format";
import { usePagination, useResource } from "./hooks";
import { JournalPreview } from "./JournalPreview";
import type { Role } from "./types";
import {
  cashCategoryNames,
  financialReportCsv,
  reportBarWidth,
  reportRangeError,
  type CashEntry,
  type FinancialReport,
  type ReportAccount,
} from "./financial-reports-model";
import "./financial-reports.css";

type AccountSelection = { account: ReportAccount; from?: string; to: string };

export default function FinancialReports({ role }: { role: Role }) {
  const today = companyToday();
  const [range, setRange] = useState({
    from: `${today.slice(0, 7)}-01`,
    to: today,
  });
  const [draft, setDraft] = useState(range);
  const [error, setError] = useState("");
  const [tab, setTab] = useState("income");
  const [revision, setRevision] = useState(0);
  const [account, setAccount] = useState<AccountSelection | null>(null);
  const [journal, setJournal] = useState<string | null>(null);
  const [classification, setClassification] = useState<CashEntry | null>(null);
  const resource = useResource<FinancialReport>(
    `/accounting/reports?${new URLSearchParams(range)}`,
    revision,
  );
  const report = resource.data;
  function apply(event: FormEvent) {
    event.preventDefault();
    const problem = reportRangeError(draft.from, draft.to);
    setError(problem);
    if (!problem) setRange({ ...draft });
  }
  function inspect(item: ReportAccount, cumulative = false) {
    if (report)
      setAccount({
        account: item,
        ...(cumulative ? {} : { from: report.from }),
        to: report.to,
      });
  }
  return (
    <div className="financial-reports">
      <form className="financial-report-filter" onSubmit={apply}>
        <FormField label="Dari tanggal">
          <input
            type="date"
            min="0001-01-01"
            max="9999-12-31"
            required
            value={draft.from}
            onChange={(e) => setDraft({ ...draft, from: e.target.value })}
          />
        </FormField>
        <FormField label="Sampai tanggal (inklusif)">
          <input
            type="date"
            min="0001-01-01"
            max="9999-12-31"
            required
            value={draft.to}
            onChange={(e) => setDraft({ ...draft, to: e.target.value })}
          />
        </FormField>
        <button className="button secondary" type="submit">
          <Filter size={16} /> Tampilkan
        </button>
        <button
          className="button secondary"
          type="button"
          disabled={!report || resource.loading || !!resource.error}
          onClick={() =>
            report &&
            downloadCsv(
              `laporan-keuangan-${report.from}-${report.to}`,
              [
                "Laporan",
                "Keterangan",
                "Akun / jurnal",
                "Nominal IDR / nilai",
                "Waktu",
                "Klasifikasi",
                "Sumber / versi",
                "Dasar / pelaku",
                "Alasan klasifikasi",
              ],
              financialReportCsv(report),
            )
          }
        >
          <Download size={16} /> CSV laporan
        </button>
      </form>
      <FormError error={error} />
      {resource.loading ? (
        <Loading label="Menghitung laporan dari jurnal…" />
      ) : resource.error || !report ? (
        <ErrorState message={resource.error} onRetry={resource.reload} />
      ) : (
        <>
          <div className="financial-report-basis">
            <strong>{report.company_name}</strong>
            <span>
              {calendarDate(report.from)} – {calendarDate(report.to)} · IDR ·{" "}
              {report.timezone}
            </span>
            <small>
              Snapshot {dateTime(report.as_of)}. Koreksi mengikuti tanggal
              jurnal koreksi; periode tertutup tetap dapat dilihat.
            </small>
          </div>
          <div
            className="tabs financial-report-tabs"
            role="tablist"
            aria-label="Jenis laporan keuangan"
          >
            {[
              ["income", "Laba rugi"],
              ["balance", "Neraca"],
              ["cash", "Arus kas"],
            ].map(([key, label]) => (
              <button
                key={key}
                role="tab"
                aria-selected={tab === key}
                className={tab === key ? "active" : ""}
                onClick={() => setTab(key)}
              >
                {label}
              </button>
            ))}
          </div>
          {tab === "income" ? (
            <>
              <p className="financial-report-hint">
                Pendapatan dikurangi seluruh beban pada periode terpilih.
                Pembelian persediaan dan pembayaran utang mengikuti jurnal aset
                atau liabilitasnya.
              </p>
              <ReportBars
                title="Pendapatan, beban, dan hasil periode"
                rows={[
                  {
                    name: "Pendapatan",
                    amount: report.income_statement.revenue_total,
                  },
                  {
                    name: "Beban",
                    amount: report.income_statement.expense_total,
                  },
                  {
                    name: "Laba / rugi bersih",
                    amount: report.income_statement.net_profit,
                  },
                ]}
              />
              <div className="financial-report-columns">
                <AccountGroup
                  title="Pendapatan"
                  items={report.income_statement.revenue_accounts}
                  total={report.income_statement.revenue_total}
                  inspect={inspect}
                />
                <AccountGroup
                  title="Beban"
                  items={report.income_statement.expense_accounts}
                  total={report.income_statement.expense_total}
                  inspect={inspect}
                />
              </div>
              <ReportTotal
                title="Laba / rugi bersih periode"
                amount={report.income_statement.net_profit}
              />
            </>
          ) : tab === "balance" ? (
            <>
              <Reconciliation
                okay={report.balance_sheet.balanced}
                okayText="Aset = liabilitas + ekuitas termasuk laba akumulasi"
                difference={report.balance_sheet.difference}
              />
              <p className="financial-report-hint">
                Posisi akumulasi seluruh jurnal sampai {calendarDate(report.to)}
                . Tanggal awal filter hanya berlaku untuk laba rugi dan arus
                kas. Laba akumulasi berasal dari saldo akun pendapatan dan
                beban, terpisah dari akun ekuitas.
              </p>
              <ReportBars
                title="Posisi keuangan"
                rows={[
                  { name: "Aset", amount: report.balance_sheet.assets_total },
                  {
                    name: "Liabilitas",
                    amount: report.balance_sheet.liabilities_total,
                  },
                  {
                    name: "Ekuitas + laba akumulasi",
                    amount: centsDecimal(
                      moneyCents(report.balance_sheet.equity_total) +
                        moneyCents(report.balance_sheet.retained_earnings),
                    ),
                  },
                ]}
              />
              <div className="financial-report-columns">
                <AccountGroup
                  title="Aset"
                  items={report.balance_sheet.asset_accounts}
                  total={report.balance_sheet.assets_total}
                  inspect={(a) => inspect(a, true)}
                />
                <div>
                  <AccountGroup
                    title="Liabilitas"
                    items={report.balance_sheet.liability_accounts}
                    total={report.balance_sheet.liabilities_total}
                    inspect={(a) => inspect(a, true)}
                  />
                  <AccountGroup
                    title="Ekuitas tercatat"
                    items={report.balance_sheet.equity_accounts}
                    total={report.balance_sheet.equity_total}
                    inspect={(a) => inspect(a, true)}
                  />
                  <details className="financial-retained">
                    <summary>
                      Laba / rugi akumulasi{" "}
                      <strong>
                        {exactMoney(report.balance_sheet.retained_earnings)}
                      </strong>
                    </summary>
                    <p>
                      Berasal dari seluruh pendapatan dikurangi beban sampai
                      tanggal neraca. Tidak dijumlahkan lagi sebagai akun
                      ekuitas.
                    </p>
                    <AccountGroup
                      title="Akun pembentuk laba akumulasi"
                      showKind
                      items={report.balance_sheet.retained_earnings_accounts}
                      inspect={(a) => inspect(a, true)}
                    />
                  </details>
                  <ReportTotal
                    title="Liabilitas + ekuitas + laba akumulasi"
                    amount={report.balance_sheet.liabilities_and_equity_total}
                  />
                </div>
              </div>
            </>
          ) : (
            <CashFlowView
              report={report}
              canClassify={["admin", "finance"].includes(role)}
              onJournal={setJournal}
              onClassify={setClassification}
            />
          )}
        </>
      )}
      {account && (
        <ReportAccountLedger
          selection={account}
          onClose={() => setAccount(null)}
        />
      )}
      {journal && (
        <JournalPreview id={journal} onClose={() => setJournal(null)} />
      )}
      {classification && (
        <ClassificationForm
          entry={classification}
          onClose={() => setClassification(null)}
          onSaved={() => {
            setClassification(null);
            setRevision((v) => v + 1);
          }}
        />
      )}
    </div>
  );
}

function centsDecimal(cents: bigint) {
  const value = cents < 0n ? -cents : cents;
  return `${cents < 0n ? "-" : ""}${value / 100n}.${(value % 100n).toString().padStart(2, "0")}`;
}

function ReportTotal({ title, amount }: { title: string; amount: string }) {
  return (
    <div
      className={`financial-report-total ${moneyCents(amount) < 0n ? "negative" : ""}`}
    >
      <span>{title}</span>
      <strong>{exactMoney(amount)}</strong>
    </div>
  );
}
function Reconciliation({
  okay,
  okayText,
  difference,
}: {
  okay: boolean;
  okayText: string;
  difference: string;
}) {
  return (
    <div className={`balance-banner ${okay ? "" : "unbalanced"}`}>
      {okay ? <CheckCheck size={18} /> : <CircleAlert size={18} />}
      <span>
        {okay
          ? okayText
          : `Selisih pembukuan ${exactMoney(difference)} — periksa jurnal`}
      </span>
    </div>
  );
}
function ReportBars({
  title,
  rows,
}: {
  title: string;
  rows: { name: string; amount: string }[];
}) {
  return (
    <section className="financial-report-chart" aria-label={title}>
      <h3>{title}</h3>
      {rows.map((row) => (
        <div key={row.name} className="financial-report-bar-row">
          <span>{row.name}</span>
          <strong>{exactMoney(row.amount)}</strong>
          <div className="financial-report-track">
            <div
              className={moneyCents(row.amount) < 0n ? "negative" : ""}
              style={{
                width: `${reportBarWidth(
                  row.amount,
                  rows.map((r) => r.amount),
                )}%`,
              }}
            />
          </div>
        </div>
      ))}
    </section>
  );
}
function AccountGroup({
  title,
  items,
  total,
  inspect,
  showKind = false,
}: {
  title: string;
  items: ReportAccount[];
  total?: string;
  inspect: (a: ReportAccount) => void;
  showKind?: boolean;
}) {
  return (
    <section className="financial-account-group">
      <h3>{title}</h3>
      {items.length ? (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>Akun</th>
                <th className="align-right">Nominal</th>
              </tr>
            </thead>
            <tbody>
              {items.map((a) => (
                <tr key={a.code}>
                  <td>
                    <button
                      className="financial-account-link"
                      onClick={() => inspect(a)}
                    >
                      <span className="mono muted">{a.code}</span> {a.name}
                    </button>
                    {showKind && (
                      <small className="financial-cell-note">
                        {a.kind === "revenue"
                          ? "Pendapatan — ditambahkan"
                          : "Beban — dikurangkan"}
                      </small>
                    )}
                  </td>
                  <td className="align-right">{exactMoney(a.amount)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="muted">Belum ada akun pada kelompok ini.</p>
      )}
      {total !== undefined && (
        <ReportTotal title={`Total ${title.toLowerCase()}`} amount={total} />
      )}
    </section>
  );
}
function CashFlowView({
  report,
  canClassify,
  onJournal,
  onClassify,
}: {
  report: FinancialReport;
  canClassify: boolean;
  onJournal: (id: string) => void;
  onClassify: (entry: CashEntry) => void;
}) {
  const cash = report.cash_flow;
  const [category, setCategory] = useState("all");
  const entries = cash.entries.filter(
    (e) => category === "all" || e.category === category,
  );
  const pages = usePagination(
    entries,
    `${report.from}:${report.to}:${category}`,
  );
  return (
    <>
      <Reconciliation
        okay={cash.reconciled}
        okayText="Saldo awal + kas masuk − kas keluar = saldo akhir"
        difference={cash.difference}
      />
      <p className="financial-report-hint">
        Metode langsung untuk akun Kas 1100. Rekening aset lain belum termasuk
        kas atau setara kas. Saldo awal pembukuan, jurnal campuran, dan akun
        aset/liabilitas kustom yang belum jelas masuk kelompok belum
        terklasifikasi. Klasifikasi manual memiliki alasan dan riwayat, termasuk
        pada periode tertutup.
      </p>
      <div className="financial-cash-balances">
        <ReportTotal
          title="Saldo awal periode"
          amount={cash.beginning_balance}
        />
        <ReportTotal title="Saldo akhir periode" amount={cash.ending_balance} />
      </div>
      <ReportBars
        title="Arus kas menurut aktivitas (neto)"
        rows={cash.categories.map((c) => ({
          name: c.name,
          amount: c.net_change,
        }))}
      />
      <div className="table-scroll">
        <table>
          <thead>
            <tr>
              <th>Aktivitas</th>
              <th className="align-right">Kas masuk</th>
              <th className="align-right">Kas keluar</th>
              <th className="align-right">Neto</th>
            </tr>
          </thead>
          <tbody>
            {cash.categories.map((c) => (
              <tr key={c.key}>
                <td>{c.name}</td>
                <td className="align-right">{exactMoney(c.cash_in)}</td>
                <td className="align-right">{exactMoney(c.cash_out)}</td>
                <td className="align-right">{exactMoney(c.net_change)}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr>
              <th>Total periode</th>
              <th className="align-right">{exactMoney(cash.cash_in)}</th>
              <th className="align-right">{exactMoney(cash.cash_out)}</th>
              <th className="align-right">{exactMoney(cash.net_change)}</th>
            </tr>
          </tfoot>
        </table>
      </div>
      <div className="financial-cash-sources">
        <h3>Sumber pergerakan kas</h3>
        <label>
          Aktivitas{" "}
          <select
            value={category}
            onChange={(e) => setCategory(e.target.value)}
          >
            <option value="all">Semua aktivitas</option>
            {cash.categories.map((c) => (
              <option key={c.key} value={c.key}>
                {c.name}
              </option>
            ))}
          </select>
        </label>
      </div>
      {!entries.length ? (
        <EmptyState
          title="Belum ada pergerakan kas"
          description="Jurnal dengan akun Kas 1100 pada periode dan aktivitas ini akan muncul di sini."
        />
      ) : (
        <>
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Tanggal / sumber jurnal</th>
                  <th>Aktivitas / dasar</th>
                  <th className="align-right">Masuk</th>
                  <th className="align-right">Keluar</th>
                  <th>Tindakan</th>
                </tr>
              </thead>
              <tbody>
                {pages.items.map((e) => (
                  <tr key={e.journal_id}>
                    <td>
                      <strong>{e.description}</strong>
                      <small className="financial-cell-note">
                        {dateTime(e.created_at)} · {e.source_type}
                      </small>
                      <small className="financial-cell-note">
                        {e.counterparts
                          .map((a) => `${a.code} ${a.name}`)
                          .join(", ") || "Antarbaris Kas 1100"}
                      </small>
                    </td>
                    <td>
                      {cashCategoryNames[e.category]}
                      <small className="financial-cell-note">
                        {classificationReason(e.classification_reason)}
                      </small>
                      {e.classification && (
                        <details className="financial-classification-history">
                          <summary>
                            Riwayat klasifikasi ({e.classification.version})
                          </summary>
                          {e.classification.history.map((h) => (
                            <p key={h.version}>
                              <strong>
                                {h.category
                                  ? cashCategoryNames[
                                      h.category as keyof typeof cashCategoryNames
                                    ]
                                  : "Kembali ke otomatis"}
                              </strong>
                              <br />
                              {h.reason}
                              <br />
                              <small>
                                {h.created_by_name} · {dateTime(h.created_at)}
                              </small>
                            </p>
                          ))}
                        </details>
                      )}
                    </td>
                    <td className="align-right">{exactMoney(e.cash_in)}</td>
                    <td className="align-right">{exactMoney(e.cash_out)}</td>
                    <td>
                      <div className="financial-source-actions">
                        <button
                          className="button secondary small"
                          onClick={() => onJournal(e.journal_id)}
                        >
                          Lihat jurnal
                        </button>
                        {canClassify && (
                          <button
                            className="button secondary small"
                            onClick={() => onClassify(e)}
                          >
                            Klasifikasi
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <Pagination {...pages} />
        </>
      )}
    </>
  );
}
function classificationReason(reason: string) {
  return (
    (
      {
        manual_override: "Klasifikasi manual beralasan",
        counterpart_mapping: "Pemetaan akun lawan",
        opening_balance: "Saldo awal pembukuan",
        cash_both_directions: "Jurnal berisi kas masuk dan keluar",
        mixed_or_unmapped_counterparts: "Akun lawan campuran / belum dipetakan",
      } as Record<string, string>
    )[reason] ?? reason
  );
}

function ClassificationForm({
  entry,
  onClose,
  onSaved,
}: {
  entry: CashEntry;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [category, setCategory] = useState(
    entry.classification?.category ?? "",
  );
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [conflict, setConflict] = useState(false);
  const attempts = useRef(new Map<string, string>());
  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError("");
    setConflict(false);
    const body = {
      category: category || null,
      reason: reason.trim(),
      version: entry.classification?.version ?? 0,
    };
    const key = JSON.stringify(body);
    if (!attempts.current.has(key))
      attempts.current.set(key, crypto.randomUUID());
    try {
      await api(`/accounting/reports/cash-flow/${entry.journal_id}`, {
        method: "PUT",
        body: { ...body, idempotency_key: attempts.current.get(key) },
      });
      onSaved();
    } catch (error) {
      setError(errorMessage(error));
      setConflict(error instanceof ApiError && error.status === 409);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title="Klasifikasi arus kas"
      description={entry.description}
      onClose={() => !busy && onClose()}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <p>
            Klasifikasi mengubah kelompok laporan untuk jurnal ini. Nominal dan
            tanggal jurnal tetap. Alasan, pelaku, dan seluruh perubahan
            disimpan.
          </p>
          <FormField label="Aktivitas">
            <select
              value={category}
              disabled={busy}
              onChange={(e) => setCategory(e.target.value)}
            >
              <option value="">Otomatis / belum terklasifikasi</option>
              <option value="operating">Operasi</option>
              <option value="investing">Investasi</option>
              <option value="financing">Pendanaan</option>
            </select>
          </FormField>
          <FormField
            label="Alasan klasifikasi *"
            hint="Jelaskan sifat penerimaan atau pengeluaran kas, misalnya pembelian peralatan."
          >
            <textarea
              required
              minLength={3}
              maxLength={1000}
              value={reason}
              disabled={busy}
              onChange={(e) => setReason(e.target.value)}
            />
          </FormField>
          <FormError error={error} />
          {conflict && (
            <button
              type="button"
              className="button secondary"
              onClick={onSaved}
            >
              Muat ulang klasifikasi terbaru
            </button>
          )}
        </div>
        <FormActions onClose={onClose} busy={busy} label="Simpan klasifikasi" />
      </form>
    </Modal>
  );
}

interface ReportLedgerData {
  items: {
    journal_id: string;
    description: string;
    created_at: string;
    source_type: string;
    debit: string;
    credit: string;
    balance: string;
  }[];
  opening_balance: string;
  closing_balance: string;
  truncated: boolean;
}
function ReportAccountLedger({
  selection,
  onClose,
}: {
  selection: AccountSelection;
  onClose: () => void;
}) {
  const [journal, setJournal] = useState<string | null>(null);
  const params = new URLSearchParams({
    ...(selection.from ? { from: selection.from } : {}),
    to: selection.to,
  });
  const resource = useResource<ReportLedgerData>(
    `/accounting/accounts/${selection.account.code}/ledger?${params}`,
  );
  const pages = usePagination(resource.data?.items ?? []);
  return (
    <Modal
      wide
      title={`Rincian akun · ${selection.account.code}`}
      description={selection.account.name}
      onClose={onClose}
    >
      <div className="form-body">
        <p>
          {selection.from
            ? `${calendarDate(selection.from)} – `
            : "Seluruh jurnal sampai "}
          {calendarDate(selection.to)}. Saldo buku besar memakai debit − kredit;
          laporan memakai saldo normal kelompok akun.
        </p>
        {resource.loading ? (
          <Loading />
        ) : resource.error || !resource.data ? (
          <ErrorState message={resource.error} onRetry={resource.reload} />
        ) : (
          <>
            <div className="financial-cash-balances">
              <ReportTotal
                title="Saldo awal (debit − kredit)"
                amount={resource.data.opening_balance}
              />
              <ReportTotal
                title="Saldo akhir (debit − kredit)"
                amount={resource.data.closing_balance}
              />
            </div>
            {resource.data.truncated && (
              <p role="status">
                Rincian menampilkan 5.000 jurnal pertama. Saldo awal/akhir dan
                nominal laporan tetap mencakup seluruh jurnal.
              </p>
            )}
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Jurnal</th>
                    <th className="align-right">Debit</th>
                    <th className="align-right">Kredit</th>
                    <th className="align-right">Saldo</th>
                  </tr>
                </thead>
                <tbody>
                  {pages.items.map((item) => (
                    <tr key={item.journal_id}>
                      <td>
                        <button
                          className="financial-account-link"
                          onClick={() => setJournal(item.journal_id)}
                        >
                          {item.description}
                        </button>
                        <small className="financial-cell-note">
                          {dateTime(item.created_at)} · {item.source_type}
                        </small>
                      </td>
                      <td className="align-right">{exactMoney(item.debit)}</td>
                      <td className="align-right">{exactMoney(item.credit)}</td>
                      <td className="align-right">
                        {exactMoney(item.balance)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <Pagination {...pages} />
          </>
        )}
      </div>
      <div className="form-actions">
        <button className="button secondary" onClick={onClose}>
          Tutup rincian
        </button>
      </div>
      {journal && (
        <JournalPreview id={journal} onClose={() => setJournal(null)} />
      )}
    </Modal>
  );
}
