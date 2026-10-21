import { useState, type FormEvent } from "react";
import {
  CheckCheck,
  Download,
  Filter,
  HandCoins,
  Plus,
  Printer,
  Scale,
} from "lucide-react";
import {
  ErrorState,
  FormError,
  FormField,
  Loading,
  PageHeader,
  Pagination,
  SearchInput,
  EmptyState,
} from "./components";
import { calendarDate, exactMoney, moneyCents } from "./dashboard-charts";
import { downloadCsv } from "./export";
import { companyToday, dateTime } from "./format";
import { usePagination, useResource } from "./hooks";
import { JournalPreview } from "./JournalPreview";
import { reportBarWidth, reportRangeError } from "./financial-reports-model";
import {
  agingNames,
  eventHref,
  eventNames,
  fundKindNames,
  statementCsv,
  targetHref,
  type PartyFund,
  type PartySide,
  type PartyStatement,
  type PartyTarget,
} from "./party-balances-model";
import type { Customer, Role } from "./types";
import type { Supplier } from "./purchasing-model";
import {
  PartyFundDetail,
  PartyFundingForm,
  OpeningItemDetail,
} from "./PartyFundDetail";
import "./party-balances.css";

export default function PartyBalances({
  role,
  changed,
}: {
  role: Role;
  changed: () => void;
}) {
  const params = new URLSearchParams(location.search);
  const [side, setSide] = useState<PartySide>(() =>
    (params.get("party_side") ?? params.get("side")) === "supplier"
      ? "supplier"
      : "customer",
  );
  const [partyId, setPartyId] = useState(
    () => params.get("party_id") ?? params.get("party") ?? "",
  );
  const [search, setSearch] = useState("");
  const [tab, setTab] = useState("statement");
  const [fundId, setFundId] = useState<string | null>(() => params.get("fund"));
  const [openingId, setOpeningId] = useState<string | null>(() =>
    params.get("opening_item"),
  );
  const [creating, setCreating] = useState(false);
  const customers = useResource<{ items: Customer[] }>("/customers");
  const suppliers = useResource<{ items: Supplier[] }>("/suppliers");
  const choices =
    side === "customer"
      ? (customers.data?.items ?? []).map((item) => ({
          id: item.id,
          name: item.name,
          label: item.name,
        }))
      : (suppliers.data?.items ?? []).map((item) => ({
          id: item.id,
          name: item.name,
          label: `${item.code} · ${item.name}`,
        }));
  const selected = choices.find((party) => party.id === partyId);
  const visibleChoices = choices.filter(
    (party) =>
      party.id === partyId ||
      party.label
        .toLocaleLowerCase("id-ID")
        .includes(search.toLocaleLowerCase("id-ID")),
  );
  const canManage = ["admin", "finance"].includes(role);
  function updated() {
    changed();
  }
  return (
    <>
      <PageHeader
        eyebrow="DARI TAGIHAN SAMPAI DANA SELESAI"
        title="Saldo pihak"
        description="Lacak piutang, utang, uang muka, kredit, dan refund pelanggan atau supplier."
        actions={
          canManage && (
            <button
              className="button primary"
              disabled={!selected}
              onClick={() => setCreating(true)}
            >
              <Plus size={17} /> Catat uang muka
            </button>
          )
        }
      />
      <section className="panel party-selector">
        <div className="party-form-grid">
          <FormField label="Jenis pihak">
            <select
              value={side}
              onChange={(e) => {
                setSide(e.target.value as PartySide);
                setPartyId("");
                setSearch("");
              }}
            >
              <option value="customer">
                Pelanggan · piutang dan dana pelanggan
              </option>
              <option value="supplier">
                Supplier · utang dan dana supplier
              </option>
            </select>
          </FormField>
          <FormField label="Cari pihak">
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Cari nama atau kode supplier"
            />
          </FormField>
          <FormField
            label={side === "customer" ? "Pilih pelanggan" : "Pilih supplier"}
          >
            <select
              value={partyId}
              onChange={(e) => setPartyId(e.target.value)}
            >
              <option value="">Pilih pihak untuk melihat rincian</option>
              {visibleChoices.map((party) => (
                <option key={party.id} value={party.id}>
                  {party.label}
                </option>
              ))}
            </select>
          </FormField>
        </div>
        {customers.error || suppliers.error ? (
          <ErrorState
            message={customers.error || suppliers.error}
            onRetry={() => {
              customers.reload();
              suppliers.reload();
            }}
          />
        ) : null}
      </section>
      {selected ? (
        <>
          <div
            className="tabs purchase-top-tabs"
            role="tablist"
            aria-label="Saldo pihak"
          >
            <button
              role="tab"
              aria-selected={tab === "statement"}
              className={tab === "statement" ? "active" : ""}
              onClick={() => setTab("statement")}
            >
              <Scale size={16} /> Buku pembantu dan aging
            </button>
            <button
              role="tab"
              aria-selected={tab === "funds"}
              className={tab === "funds" ? "active" : ""}
              onClick={() => setTab("funds")}
            >
              <HandCoins size={16} /> Uang muka, kredit, dan refund
            </button>
          </div>
          {tab === "statement" ? (
            <PartyStatementView
              key={`${side}:${partyId}`}
              side={side}
              partyId={partyId}
              onFund={setFundId}
              onOpening={setOpeningId}
            />
          ) : (
            <PartyFundsList
              side={side}
              partyId={partyId}
              onSelect={setFundId}
            />
          )}
        </>
      ) : customers.loading || suppliers.loading ? (
        <Loading />
      ) : (
        <section className="panel">
          <EmptyState
            title="Pilih pelanggan atau supplier"
            description="Tagihan belum selesai, kredit tersedia, dan dana refund disajikan terpisah supaya posisi usaha mudah diperiksa."
          />
        </section>
      )}
      {creating && selected && (
        <PartyFundingForm
          side={side}
          party={{ id: selected.id, name: selected.name }}
          onClose={() => setCreating(false)}
          onSaved={(fund) => {
            setCreating(false);
            setFundId(fund.id);
            updated();
          }}
        />
      )}
      {fundId && (
        <PartyFundDetail
          id={fundId}
          canManage={canManage}
          onClose={() => setFundId(null)}
          onChanged={updated}
        />
      )}
      {openingId && (
        <OpeningItemDetail
          id={openingId}
          canManage={canManage}
          onClose={() => setOpeningId(null)}
          onChanged={updated}
        />
      )}
    </>
  );
}

function PartyFundsList({
  side,
  partyId,
  onSelect,
}: {
  side: PartySide;
  partyId: string;
  onSelect: (id: string) => void;
}) {
  const resource = useResource<{ items: PartyFund[] }>(
    `/party-funds?${new URLSearchParams({ side, party_id: partyId })}`,
  );
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("active");
  const items = (resource.data?.items ?? []).filter(
    (item) =>
      `${item.number} ${item.reference ?? ""} ${item.description ?? ""}`
        .toLocaleLowerCase("id-ID")
        .includes(search.toLocaleLowerCase("id-ID")) &&
      (status === "all" ||
        (status === "reversed" ? item.reversed : !item.reversed)),
  );
  const pagination = usePagination(items, `${search}:${status}`, 20);
  return (
    <section className="panel">
      <div className="table-toolbar">
        <SearchInput
          value={search}
          onChange={setSearch}
          placeholder="Cari nomor atau referensi dana…"
        />
        <label className="purchase-filter">
          <span>Status</span>
          <select value={status} onChange={(e) => setStatus(e.target.value)}>
            <option value="active">Aktif</option>
            <option value="reversed">Dibalik</option>
            <option value="all">Semua</option>
          </select>
        </label>
        <button
          className="text-button"
          disabled={resource.loading || !!resource.error}
          onClick={() =>
            downloadCsv(
              "haruflow-saldo-dana-pihak",
              [
                "Nomor",
                "Pihak",
                "Jenis",
                "Tanggal",
                "Nilai asal",
                "Dialokasikan",
                "Refund aktual",
                "Disisihkan refund",
                "Tersedia",
                "Dibalik",
                "Referensi",
              ],
              items.map((item) => [
                item.number,
                item.party_name,
                fundKindNames[item.kind] ?? item.kind,
                item.created_at,
                item.amount,
                item.allocated,
                item.refunded,
                item.reserved,
                item.available,
                item.reversed ? "Ya" : "Tidak",
                item.reference,
              ]),
            )
          }
        >
          <Download size={15} /> Ekspor semua hasil
        </button>
      </div>
      <p className="party-help">
        Saldo dana di bawah adalah posisi saat ini. Kredit belum dipakai tidak
        mengurangi tampilan tagihan terlambat. Dana yang disisihkan untuk refund
        tidak tersedia bagi alokasi lain.
      </p>
      {resource.loading ? (
        <Loading />
      ) : resource.error ? (
        <ErrorState message={resource.error} onRetry={resource.reload} />
      ) : items.length ? (
        <>
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Sumber dana</th>
                  <th className="align-right">Asal</th>
                  <th className="align-right">Dipakai</th>
                  <th className="align-right">Refund aktual</th>
                  <th className="align-right">Disisihkan</th>
                  <th className="align-right">Tersedia</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {pagination.items.map((item) => (
                  <tr key={item.id}>
                    <td>
                      <strong>{item.number}</strong>
                      <small className="block muted">
                        {fundKindNames[item.kind] ?? item.kind} ·{" "}
                        {dateTime(item.created_at)}
                        {item.reversed && " · Dibalik"}
                      </small>
                      {item.reference && (
                        <small className="block muted">{item.reference}</small>
                      )}
                    </td>
                    <td className="align-right">{exactMoney(item.amount)}</td>
                    <td className="align-right">
                      {exactMoney(item.allocated)}
                    </td>
                    <td className="align-right">{exactMoney(item.refunded)}</td>
                    <td className="align-right">{exactMoney(item.reserved)}</td>
                    <td className="align-right">
                      <strong>{exactMoney(item.available)}</strong>
                    </td>
                    <td>
                      <button
                        className="text-button"
                        onClick={() => onSelect(item.id)}
                      >
                        Rincian dan penyelesaian
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <Pagination {...pagination} />
        </>
      ) : (
        <EmptyState
          title="Belum ada saldo dana"
          description="Uang muka, kredit retur, dan kredit pembukaan milik pihak ini akan tercatat di sini."
        />
      )}
    </section>
  );
}

function PartyStatementView({
  side,
  partyId,
  onFund,
  onOpening,
}: {
  side: PartySide;
  partyId: string;
  onFund: (id: string) => void;
  onOpening: (id: string) => void;
}) {
  const [range, setRange] = useState({ from: "", as_of: companyToday() });
  const [draft, setDraft] = useState(range);
  const [error, setError] = useState("");
  const [bucket, setBucket] = useState("");
  const [status, setStatus] = useState("outstanding");
  const [eventFilter, setEventFilter] = useState("");
  const [journal, setJournal] = useState<string | null>(null);
  const query = new URLSearchParams({
    as_of: range.as_of,
    ...(range.from ? { from: range.from } : {}),
  });
  const resource = useResource<PartyStatement>(
    `/parties/${side}/${partyId}/statement?${query}`,
  );
  const statement = resource.data;
  const docs = (statement?.documents ?? []).filter(
    (doc) =>
      (!bucket || doc.bucket === bucket) &&
      (status === "all" ||
        (status === "outstanding"
          ? moneyCents(doc.outstanding) > 0n
          : moneyCents(doc.outstanding) === 0n)),
  );
  const documents = usePagination(
    docs,
    `${bucket}:${status}:${range.as_of}`,
    20,
  );
  const eventItems = (statement?.events ?? []).filter(
    (event) => !eventFilter || event.event_type === eventFilter,
  );
  const events = usePagination(
    eventItems,
    `${eventFilter}:${range.from}:${range.as_of}`,
    30,
  );
  function apply(event: FormEvent) {
    event.preventDefault();
    const problem =
      reportRangeError(draft.from || draft.as_of, draft.as_of) ||
      (draft.as_of > companyToday()
        ? "Tanggal posisi tidak dapat melewati hari ini."
        : "");
    setError(problem);
    if (!problem) setRange({ ...draft });
  }
  return (
    <section className="panel party-statement">
      <form className="party-report-filter" onSubmit={apply}>
        <FormField label="Aktivitas dari (opsional)">
          <input
            type="date"
            value={draft.from}
            max={draft.as_of}
            min="0001-01-01"
            onChange={(e) => setDraft({ ...draft, from: e.target.value })}
          />
        </FormField>
        <FormField label="Posisi saldo per tanggal (inklusif)">
          <input
            required
            type="date"
            value={draft.as_of}
            max={companyToday()}
            min="0001-01-01"
            onChange={(e) => setDraft({ ...draft, as_of: e.target.value })}
          />
        </FormField>
        <button className="button secondary">
          <Filter size={15} /> Tampilkan
        </button>
        <button
          type="button"
          className="text-button"
          disabled={!statement || resource.loading || !!resource.error}
          onClick={() => {
            if (statement)
              downloadCsv(
                "haruflow-buku-pembantu",
                ["Buku pembantu pihak", statement.party.name],
                statementCsv(statement),
              );
          }}
        >
          <Download size={15} /> CSV seluruh laporan
        </button>
        <button
          type="button"
          className="text-button"
          disabled={!statement || resource.loading || !!resource.error}
          onClick={() => window.print()}
        >
          <Printer size={15} /> Cetak
        </button>
      </form>
      <FormError error={error} />
      {resource.loading ? (
        <Loading />
      ) : resource.error ? (
        <ErrorState message={resource.error} onRetry={resource.reload} />
      ) : (
        statement && (
          <>
            <div className="party-statement-heading">
              <h2>{statement.party.name}</h2>
              <p>
                Posisi pada akhir {calendarDate(statement.as_of)} ·{" "}
                {statement.timezone}.{" "}
                {statement.from
                  ? `Aktivitas sejak ${calendarDate(statement.from)}.`
                  : "Aktivitas sejak awal pembukuan."}
              </p>
            </div>
            <div className="party-metrics">
              <Metric
                label={
                  side === "customer"
                    ? "Piutang belum selesai"
                    : "Utang belum selesai"
                }
                value={statement.summary.outstanding}
              />
              <Metric
                label="Kredit/uang muka tersedia"
                value={statement.summary.available_credit}
              />
              <Metric
                label="Disisihkan untuk refund"
                value={statement.summary.reserved_refunds}
              />
              <Metric
                label="Seluruh saldo kredit"
                value={statement.summary.credit_balance}
              />
            </div>
            <p className="party-help">
              Tagihan dan kredit disajikan terpisah. Alokasi mengurangi keduanya
              tanpa arus kas baru. Saldo bersih untuk pembanding:{" "}
              {exactMoney(statement.summary.net_balance)}. Posisi historis
              mengikuti kejadian sampai tanggal laporan; tindakan baru tetap
              dibukukan pada waktu transaksi sebenarnya.
            </p>
            <div className="party-chart-grid">
              <section className="party-chart">
                <h3>Posisi saldo pihak</h3>
                {[
                  {
                    label: side === "customer" ? "Sisa piutang" : "Sisa utang",
                    value: statement.summary.outstanding,
                  },
                  {
                    label: "Kredit tersedia",
                    value: statement.summary.available_credit,
                  },
                  {
                    label: "Refund disisihkan",
                    value: statement.summary.reserved_refunds,
                  },
                ].map((row) => (
                  <div className="party-bar-row" key={row.label}>
                    <div>
                      <span>{row.label}</span>
                      <strong>{exactMoney(row.value)}</strong>
                    </div>
                    <span className="party-bar-track">
                      <span
                        style={{
                          width: `${reportBarWidth(row.value, [statement.summary.outstanding, statement.summary.available_credit, statement.summary.reserved_refunds])}%`,
                        }}
                      />
                    </span>
                  </div>
                ))}
              </section>
              <section className="party-chart">
                <h3>Umur tagihan</h3>
                <p className="muted">
                  Pilih kelompok untuk melihat dokumen sumber.
                </p>
                {statement.aging.map((row) => (
                  <button
                    key={row.key}
                    className={`party-aging-bar${bucket === row.key ? " selected" : ""}`}
                    onClick={() => setBucket(bucket === row.key ? "" : row.key)}
                  >
                    <span>
                      <span>
                        {agingNames[row.key] ?? row.key} · {row.count}
                      </span>
                      <strong>{exactMoney(row.total)}</strong>
                    </span>
                    <span className="party-bar-track">
                      <span
                        style={{
                          width: `${reportBarWidth(
                            row.total,
                            statement.aging.map((item) => item.total),
                          )}%`,
                        }}
                      />
                    </span>
                  </button>
                ))}
              </section>
            </div>
            <section className="party-section party-screen-only">
              <div className="data-file-section-head">
                <h3>
                  Dokumen sumber {bucket && `· ${agingNames[bucket] ?? bucket}`}
                </h3>
                <div className="data-file-actions">
                  {bucket && (
                    <button
                      className="text-button"
                      onClick={() => setBucket("")}
                    >
                      Semua umur
                    </button>
                  )}
                  <select
                    aria-label="Status tagihan"
                    value={status}
                    onChange={(e) => setStatus(e.target.value)}
                  >
                    <option value="outstanding">Belum selesai</option>
                    <option value="settled">Selesai</option>
                    <option value="all">Semua</option>
                  </select>
                </div>
              </div>
              {docs.length ? (
                <>
                  <div className="table-scroll">
                    <table>
                      <thead>
                        <tr>
                          <th>Dokumen</th>
                          <th>Tanggal / jatuh tempo</th>
                          <th>Umur</th>
                          <th className="align-right">Nilai awal</th>
                          <th className="align-right">Kredit retur</th>
                          <th className="align-right">Sisa</th>
                          <th />
                        </tr>
                      </thead>
                      <tbody>
                        {documents.items.map((doc) => (
                          <tr key={`${doc.target_type}:${doc.id}`}>
                            <td>
                              <strong>{doc.number}</strong>
                              <small className="block muted">
                                {doc.target_type.startsWith("opening")
                                  ? "Tagihan pembukaan"
                                  : doc.target_type === "invoice"
                                    ? "Invoice penjualan"
                                    : "Tagihan supplier"}
                              </small>
                            </td>
                            <td>
                              {doc.document_date ?? "—"}
                              <small className="block muted">
                                Jatuh tempo: {doc.due_date ?? "Belum diketahui"}
                              </small>
                            </td>
                            <td>
                              {agingNames[doc.bucket ?? "unknown"] ??
                                doc.bucket}
                            </td>
                            <td className="align-right">
                              {exactMoney(doc.total)}
                            </td>
                            <td className="align-right">
                              {exactMoney(doc.credit_total)}
                            </td>
                            <td className="align-right">
                              <strong>{exactMoney(doc.outstanding)}</strong>
                            </td>
                            <td>
                              <TargetLink target={doc} onOpening={onOpening} />
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                  <Pagination {...documents} />
                </>
              ) : (
                <EmptyState
                  title="Tidak ada tagihan dalam filter ini"
                  description="Pilih kelompok umur atau status yang berbeda untuk memeriksa dokumen lainnya."
                />
              )}
            </section>
            <section className="party-section party-screen-only">
              <div className="data-file-section-head">
                <div>
                  <h3>Perubahan selama rentang laporan</h3>
                  <p className="muted">
                    Saldo awal rentang: tagihan{" "}
                    {exactMoney(statement.opening.debt_balance)} · kredit{" "}
                    {exactMoney(statement.opening.credit_balance)}.
                  </p>
                </div>
                <select
                  aria-label="Jenis aktivitas pihak"
                  value={eventFilter}
                  onChange={(e) => setEventFilter(e.target.value)}
                >
                  <option value="">Semua aktivitas</option>
                  {[
                    ...new Set(
                      statement.events.map((event) => event.event_type),
                    ),
                  ].map((value) => (
                    <option key={value} value={value}>
                      {eventNames[value] ?? value}
                    </option>
                  ))}
                </select>
              </div>
              {eventItems.length ? (
                <>
                  <div className="table-scroll">
                    <table>
                      <thead>
                        <tr>
                          <th>Waktu / aktivitas</th>
                          <th>Sumber</th>
                          <th className="align-right">Perubahan tagihan</th>
                          <th className="align-right">Perubahan kredit</th>
                          <th className="align-right">Kas aktual (+ masuk)</th>
                          <th className="align-right">Saldo tagihan</th>
                          <th className="align-right">Saldo kredit</th>
                          <th />
                        </tr>
                      </thead>
                      <tbody>
                        {events.items.map((event) => (
                          <tr key={event.id}>
                            <td>
                              {dateTime(event.created_at)}
                              <small className="block muted">
                                {eventNames[event.event_type] ??
                                  event.event_type}
                              </small>
                            </td>
                            <td>
                              {eventHref(event) ? (
                                <a
                                  className="text-button"
                                  href={eventHref(event)!}
                                >
                                  {event.number}
                                </a>
                              ) : event.source_type === "party_fund" ? (
                                <button
                                  className="text-button"
                                  onClick={() => onFund(event.source_id)}
                                >
                                  {event.number}
                                </button>
                              ) : event.target_type?.startsWith("opening") &&
                                event.target_id ? (
                                <button
                                  className="text-button"
                                  onClick={() => onOpening(event.target_id!)}
                                >
                                  {event.number}
                                </button>
                              ) : (
                                event.number
                              )}
                            </td>
                            <td className="align-right">
                              {exactMoney(event.debt_delta)}
                            </td>
                            <td className="align-right">
                              {exactMoney(event.credit_delta)}
                            </td>
                            <td className="align-right">
                              {exactMoney(event.cash_delta)}
                            </td>
                            <td className="align-right">
                              {exactMoney(event.debt_balance)}
                            </td>
                            <td className="align-right">
                              {exactMoney(event.credit_balance)}
                            </td>
                            <td>
                              {event.journal_id && (
                                <button
                                  className="text-button"
                                  onClick={() => setJournal(event.journal_id)}
                                >
                                  Jurnal
                                </button>
                              )}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                  <Pagination {...events} />
                </>
              ) : (
                <p className="party-help">
                  Tidak ada aktivitas dalam rentang dan jenis yang dipilih.
                </p>
              )}
            </section>
            <div className="party-print-only">
              <h3>Seluruh dokumen pada tanggal laporan</h3>
              <table>
                <thead>
                  <tr>
                    <th>Dokumen</th>
                    <th>Tanggal / jatuh tempo</th>
                    <th>Nilai awal</th>
                    <th>Kredit retur</th>
                    <th>Diselesaikan</th>
                    <th>Sisa</th>
                  </tr>
                </thead>
                <tbody>
                  {statement.documents.map((doc) => (
                    <tr key={`${doc.target_type}:${doc.id}`}>
                      <td>{doc.number}</td>
                      <td>
                        {doc.document_date ?? "Belum diketahui"}
                        <br />
                        {doc.due_date ?? "Jatuh tempo belum diketahui"}
                      </td>
                      <td>{exactMoney(doc.total)}</td>
                      <td>{exactMoney(doc.credit_total)}</td>
                      <td>{exactMoney(doc.paid)}</td>
                      <td>{exactMoney(doc.outstanding)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <h3>Seluruh aktivitas dalam rentang laporan</h3>
              <p>
                Saldo awal rentang: tagihan{" "}
                {exactMoney(statement.opening.debt_balance)} · kredit{" "}
                {exactMoney(statement.opening.credit_balance)}.
              </p>
              <table>
                <thead>
                  <tr>
                    <th>Waktu / sumber</th>
                    <th>Aktivitas</th>
                    <th>Perubahan tagihan</th>
                    <th>Perubahan kredit</th>
                    <th>Kas aktual</th>
                    <th>Saldo tagihan / kredit</th>
                  </tr>
                </thead>
                <tbody>
                  {statement.events.map((event) => (
                    <tr key={event.id}>
                      <td>
                        {dateTime(event.created_at)}
                        <br />
                        {event.number}
                      </td>
                      <td>
                        {eventNames[event.event_type] ?? event.event_type}
                      </td>
                      <td>{exactMoney(event.debt_delta)}</td>
                      <td>{exactMoney(event.credit_delta)}</td>
                      <td>{exactMoney(event.cash_delta)}</td>
                      <td>
                        {exactMoney(event.debt_balance)}
                        <br />
                        {exactMoney(event.credit_balance)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <section
              className={`party-reconciliation ${statement.reconciliation.balanced ? "balanced" : "difference"}`}
            >
              <h3>
                {statement.reconciliation.balanced ? (
                  <CheckCheck size={18} />
                ) : (
                  <Scale size={18} />
                )}{" "}
                Rekonsiliasi seluruh{" "}
                {side === "customer" ? "pelanggan" : "supplier"}
              </h3>
              <p>
                Pemeriksaan ini mencakup semua pihak sejenis pada tanggal
                laporan, bukan hanya pihak yang sedang dibuka.
              </p>
              <div className="table-scroll">
                <table>
                  <thead>
                    <tr>
                      <th>Kontrol</th>
                      <th className="align-right">Ledger</th>
                      <th className="align-right">Buku pembantu</th>
                      <th className="align-right">Selisih</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr>
                      <td>
                        {side === "customer" ? "Piutang" : "Utang supplier"}
                      </td>
                      <td className="align-right">
                        {exactMoney(statement.reconciliation.ar_or_ap_ledger)}
                      </td>
                      <td className="align-right">
                        {exactMoney(
                          statement.reconciliation.ar_or_ap_subledger,
                        )}
                      </td>
                      <td className="align-right">
                        {exactMoney(
                          statement.reconciliation.ar_or_ap_difference,
                        )}
                      </td>
                    </tr>
                    <tr>
                      <td>Kredit/uang muka</td>
                      <td className="align-right">
                        {exactMoney(statement.reconciliation.credits_ledger)}
                      </td>
                      <td className="align-right">
                        {exactMoney(statement.reconciliation.credits_subledger)}
                      </td>
                      <td className="align-right">
                        {exactMoney(
                          statement.reconciliation.credits_difference,
                        )}
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </section>
          </>
        )
      )}
      {journal && (
        <JournalPreview id={journal} onClose={() => setJournal(null)} />
      )}
    </section>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span>{label}</span>
      <strong>{exactMoney(value)}</strong>
    </div>
  );
}
export function TargetLink({
  target,
  onOpening,
}: {
  target: Pick<PartyTarget, "target_type" | "id">;
  onOpening: (id: string) => void;
}) {
  const href = targetHref(target);
  return href ? (
    <a className="text-button" href={href}>
      Lihat dokumen
    </a>
  ) : (
    <button
      type="button"
      className="text-button"
      onClick={() => onOpening(target.id)}
    >
      Lihat tagihan awal
    </button>
  );
}
