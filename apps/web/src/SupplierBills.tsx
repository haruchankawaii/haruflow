import { useEffect, useRef, useState, type FormEvent } from "react";
import {
  ArrowRight,
  FileText,
  Info,
  Plus,
  RefreshCw,
  Undo2,
} from "lucide-react";
import { api, ApiError, errorMessage } from "./api";
import {
  EmptyState,
  ErrorState,
  FormError,
  FormField,
  Loading,
  Modal,
  Pagination,
  SearchInput,
} from "./components";
import { calendarDate, exactMoney } from "./dashboard-charts";
import { dateTime } from "./format";
import { downloadCsv } from "./export";
import { usePagination, useResource } from "./hooks";
import { JournalPreview } from "./JournalPreview";
import {
  exactCost,
  exactDecimal,
  requestIdentity,
  type PurchaseOrder,
  type PurchaseReceipt,
  type RequestIdentity,
} from "./purchasing-model";
import {
  billChartRows,
  billDraftIssue,
  billDueNames,
  eligibleBillReceipts,
  type BillDraft,
  type SupplierBill,
  type SupplierBillsSummary,
} from "./bills-model";
import "./supplier-bills.css";
import { SupplierPayments } from "./SupplierPayments";
import { ReturnsPanel } from "./ReturnsPanel";
import { AttachmentsPanel } from "./AttachmentsPanel";
import { SupplierPayablesInsights } from "./SupplierPayablesInsights";
import { billAllocationPreview } from "./bills-model";

export function SupplierBills({
  revision,
  onSelect,
  onCreate,
  canManage,
}: {
  revision: number;
  onSelect: (id: string) => void;
  onCreate: () => void;
  canManage: boolean;
}) {
  const bills = useResource<{ items: SupplierBill[] }>(
    "/purchase-bills",
    revision,
  );
  const summary = useResource<SupplierBillsSummary>(
    "/purchase-bills/summary",
    revision,
  );
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("");
  const [due, setDue] = useState("");
  const items = (bills.data?.items ?? []).filter(
    (bill) =>
      (!status || bill.status === status) &&
      (!due || bill.due_status === due) &&
      `${bill.number} ${bill.supplier_name} ${bill.supplier_code} ${bill.supplier_invoice_number} ${bill.receipt_number} ${bill.order_number}`
        .toLowerCase()
        .includes(search.toLowerCase()),
  );
  const pagination = usePagination(items, `${search}|${status}|${due}`);
  return (
    <>
      <SupplierPayablesInsights revision={revision} onSelect={onSelect} />
      {summary.loading ? (
        <Loading />
      ) : summary.error ? (
        <ErrorState message={summary.error} onRetry={summary.reload} />
      ) : (
        summary.data && (
          <>
            <BillStatusChart
              summary={summary.data}
              selected={due}
              onSelect={(value) => {
                setStatus("");
                setDue(value);
              }}
            />
            <SupplierBalanceReport summary={summary.data} />
          </>
        )
      )}
      <section className="panel purchase-panel">
        <div className="table-toolbar">
          <div>
            <h2 className="bill-list-title">Daftar tagihan supplier</h2>
            <small className="muted">
              Dokumen asal, jatuh tempo, dan sisa utang.
            </small>
          </div>
          <button
            className="text-button export-button"
            disabled={bills.loading || !!bills.error}
            onClick={() =>
              downloadCsv(
                "haruflow-tagihan-supplier",
                [
                  "Nomor tagihan",
                  "Kode supplier",
                  "Supplier",
                  "Invoice supplier",
                  "PO",
                  "Penerimaan",
                  "Tanggal invoice",
                  "Jatuh tempo",
                  "Status",
                  "Posisi jatuh tempo",
                  "Total",
                  "Kredit retur",
                  "Tagihan bersih",
                  "Dibayar",
                  "Sisa utang",
                  "Catatan",
                  "Alasan pembatalan",
                ],
                items.map((bill) => [
                  bill.number,
                  bill.supplier_code,
                  bill.supplier_name,
                  bill.supplier_invoice_number,
                  bill.order_number,
                  bill.receipt_number,
                  bill.invoice_date,
                  bill.due_date,
                  bill.status === "void"
                    ? "Dibatalkan"
                    : bill.status === "paid"
                      ? "Lunas"
                      : bill.status === "partial"
                        ? "Dibayar sebagian"
                        : "Belum dibayar",
                  billDueNames[bill.due_status],
                  bill.total,
                  bill.credit_total,
                  bill.net_total,
                  bill.paid,
                  bill.outstanding,
                  bill.notes,
                  bill.reversal?.reason ?? "",
                ]),
              )
            }
          >
            Ekspor CSV
          </button>
        </div>
        <div className="purchase-filters bill-filters">
          <SearchInput
            value={search}
            onChange={setSearch}
            placeholder="Cari tagihan, invoice, supplier, PO…"
          />
          <label className="purchase-filter">
            <span>Status</span>
            <select value={status} onChange={(e) => setStatus(e.target.value)}>
              <option value="">Semua status</option>
              <option value="unpaid">Belum dibayar</option>
              <option value="partial">Dibayar sebagian</option>
              <option value="paid">Lunas</option>
              <option value="void">Dibatalkan</option>
            </select>
          </label>
          <label className="purchase-filter">
            <span>Jatuh tempo</span>
            <select value={due} onChange={(e) => setDue(e.target.value)}>
              <option value="">Semua tanggal</option>
              <option value="upcoming">Belum jatuh tempo</option>
              <option value="due_today">Hari ini</option>
              <option value="overdue">Lewat jatuh tempo</option>
            </select>
          </label>
        </div>
        {bills.loading ? (
          <Loading />
        ) : bills.error ? (
          <ErrorState message={bills.error} onRetry={bills.reload} />
        ) : items.length ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Tagihan</th>
                  <th>Supplier</th>
                  <th>Invoice supplier</th>
                  <th>Jatuh tempo</th>
                  <th>Status</th>
                  <th className="align-right">Sisa utang</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {pagination.items.map((bill) => (
                  <tr key={bill.id}>
                    <td>
                      <button
                        className="table-link"
                        onClick={() => onSelect(bill.id)}
                      >
                        {bill.number}
                      </button>
                      <small className="bill-cell-sub">
                        {calendarDate(bill.invoice_date)}
                      </small>
                    </td>
                    <td>
                      <span className="purchase-cell">
                        <strong>{bill.supplier_name}</strong>
                        <small>{bill.supplier_code}</small>
                      </span>
                    </td>
                    <td>{bill.supplier_invoice_number}</td>
                    <td>
                      {calendarDate(bill.due_date)}
                      {bill.status !== "void" && (
                        <span className={`bill-due-label ${bill.due_status}`}>
                          {billDueNames[bill.due_status]}
                        </span>
                      )}
                    </td>
                    <td>
                      <BillStatus bill={bill} />
                    </td>
                    <td className="align-right font-medium">
                      {exactMoney(bill.outstanding)}
                    </td>
                    <td>
                      <button
                        className="text-button"
                        onClick={() => onSelect(bill.id)}
                      >
                        Detail <ArrowRight size={14} />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <EmptyState
            title={
              search || status || due
                ? "Tagihan tidak ditemukan"
                : "Catat kewajiban kepada supplier."
            }
            description={
              search || status || due
                ? "Ubah pencarian atau filter untuk melihat tagihan lain."
                : "Pilih penerimaan PO aktif, lalu isi nomor invoice dan jatuh temponya. Nilai tagihan mengikuti barang yang diterima."
            }
            action={
              canManage &&
              !search &&
              !status &&
              !due && (
                <button className="button secondary" onClick={onCreate}>
                  <Plus size={16} /> Catat tagihan
                </button>
              )
            }
          />
        )}
        <div className="table-footer">
          <Pagination {...pagination} />
        </div>
      </section>
      <p className="purchase-history-note">
        Daftar, pencarian, dan ekspor mencakup hingga 500 tagihan terbaru.
        Grafik dan ringkasan supplier mencakup seluruh sisa utang. Pembayaran
        serta koreksinya tersedia pada detail tagihan.
      </p>
    </>
  );
}

function BillStatus({ bill }: { bill: SupplierBill }) {
  return (
    <span className={`bill-status ${bill.status}`}>
      {bill.status === "void"
        ? "Dibatalkan"
        : bill.status === "paid"
          ? "Lunas"
          : bill.status === "partial"
            ? "Dibayar sebagian"
            : "Belum dibayar"}
    </span>
  );
}

function BillStatusChart({
  summary,
  selected,
  onSelect,
}: {
  summary: SupplierBillsSummary;
  selected: string;
  onSelect: (value: string) => void;
}) {
  const rows = billChartRows(summary);
  return (
    <section
      className="purchase-status-chart panel bill-status-chart"
      aria-labelledby="bill-chart-title"
    >
      <div className="purchase-chart-heading">
        <div>
          <span className="purchase-chart-eyebrow">UTANG KEPADA SUPPLIER</span>
          <h2 id="bill-chart-title">Kapan tagihan perlu dibayar?</h2>
          <p>
            Posisi {calendarDate(summary.today)} · zona waktu {summary.timezone}
          </p>
        </div>
        <small>Diperbarui {dateTime(summary.as_of)}</small>
      </div>
      <div className="purchase-chart-content">
        <div
          className="purchase-chart-bars"
          aria-label="Grafik sisa utang menurut jatuh tempo"
        >
          {rows.map((row) => (
            <button
              key={row.status}
              className={`purchase-chart-row bill-chart-row ${row.status} ${selected === row.status ? "selected" : ""}`}
              aria-pressed={selected === row.status}
              aria-label={`${billDueNames[row.status]}: ${row.count} tagihan, ${exactMoney(row.value)}. Filter daftar tagihan.`}
              onClick={() =>
                onSelect(selected === row.status ? "" : row.status)
              }
            >
              <span className="purchase-chart-label">
                <i />
                {billDueNames[row.status]}
              </span>
              <svg
                viewBox="0 0 100 10"
                preserveAspectRatio="none"
                aria-hidden="true"
              >
                <rect
                  className="purchase-chart-track"
                  width="100"
                  height="10"
                  rx="3"
                />
                <rect
                  width={row.width}
                  height="10"
                  rx="3"
                  fill="currentColor"
                />
              </svg>
              <span className="purchase-chart-value">
                <b>{exactMoney(row.value)}</b>
                <small>{row.count.toLocaleString("id-ID")} tagihan</small>
              </span>
            </button>
          ))}
          {summary.counts.unpaid === 0 && (
            <p className="purchase-chart-empty">
              Belum ada utang dari tagihan supplier aktif.
            </p>
          )}
          <p className="purchase-chart-caption">
            Panjang batang menunjukkan sisa utang. Pilih kategori untuk
            memfilter daftar.
          </p>
        </div>
        <div className="purchase-chart-highlights">
          <div className="received">
            <span>Total utang supplier</span>
            <strong>{exactMoney(summary.outstanding_total)}</strong>
            <small>
              {summary.counts.unpaid.toLocaleString("id-ID")} tagihan aktif.
              Tagihan yang dibatalkan tidak dihitung.
            </small>
          </div>
        </div>
      </div>
      <p className="purchase-chart-scope">
        Seluruh tagihan aktif sepanjang waktu, termasuk di luar 500 dokumen
        terbaru. Tidak mengikuti filter daftar. Jatuh tempo mengikuti tanggal
        perusahaan, bukan tanggal perangkat. Ini ringkasan jatuh tempo; kelompok
        umur utang dan pembayaran tersedia pada ringkasan di atas.
      </p>
    </section>
  );
}

function SupplierBalanceReport({ summary }: { summary: SupplierBillsSummary }) {
  return (
    <section className="panel bill-supplier-report">
      <details>
        <summary>
          <span>
            <strong>Utang per supplier</strong>
            <small>
              {summary.suppliers.length.toLocaleString("id-ID")} supplier dengan
              tagihan aktif · seluruh waktu
            </small>
          </span>
        </summary>
        {summary.suppliers.length ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Supplier</th>
                  <th className="align-right">Tagihan aktif</th>
                  <th className="align-right">Sisa utang</th>
                  <th className="align-right">Lewat jatuh tempo</th>
                </tr>
              </thead>
              <tbody>
                {summary.suppliers.map((supplier) => (
                  <tr key={supplier.supplier_id}>
                    <td>
                      <span className="purchase-cell">
                        <strong>{supplier.supplier_name}</strong>
                        <small>{supplier.supplier_code}</small>
                      </span>
                    </td>
                    <td className="align-right">
                      {supplier.bill_count.toLocaleString("id-ID")}
                    </td>
                    <td className="align-right">
                      {exactMoney(supplier.outstanding_total)}
                    </td>
                    <td className="align-right">
                      {exactMoney(supplier.overdue_total)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <p className="bill-empty-report">
            Supplier dengan utang akan muncul setelah tagihan pertama dicatat.
          </p>
        )}
      </details>
    </section>
  );
}

export function SupplierBillForm({
  initialOrderId,
  initialReceiptId,
  revision,
  onClose,
  onSaved,
}: {
  initialOrderId?: string;
  initialReceiptId?: string;
  revision: number;
  onClose: () => void;
  onSaved: (id: string) => void;
}) {
  const orders = useResource<{ items: PurchaseOrder[] }>(
    "/purchase-orders",
    revision,
  );
  const summary = useResource<SupplierBillsSummary>(
    "/purchase-bills/summary",
    revision,
  );
  const sources = eligibleBillReceipts(orders.data?.items ?? []);
  const [orderId, setOrderId] = useState(initialOrderId ?? "");
  const [receiptId, setReceiptId] = useState(initialReceiptId ?? "");
  const [quantities, setQuantities] = useState<Record<string, string>>({});
  const order = useBillSource(orderId, revision);
  const source = order.data?.receipts.find(
    (receipt) =>
      (receiptId ? receipt.id === receiptId : receipt.can_bill) &&
      !receipt.reversal,
  );
  const allocation = billAllocationPreview(source, quantities);
  const [draft, setDraft] = useState<BillDraft>({
    supplier_invoice_number: "",
    invoice_date: "",
    due_date: "",
    notes: "",
  });
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  const [error, setError] = useState("");
  const [conflict, setConflict] = useState(false);
  useEffect(() => {
    if (summary.data)
      setDraft((previous) => ({
        ...previous,
        invoice_date: previous.invoice_date || summary.data!.today,
        due_date: previous.due_date || summary.data!.today,
      }));
  }, [summary.data]);
  const set = (key: keyof BillDraft, value: string) =>
    setDraft((previous) => ({ ...previous, [key]: value }));
  const issue = summary.data
    ? billDraftIssue(draft, summary.data.today)
    : "Tanggal perusahaan sedang dimuat.";
  const eligible =
    !!orderId &&
    order.data?.id === orderId &&
    !!source?.can_bill &&
    !order.loading &&
    !order.error;
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busyRef.current || !order.data || !source || !eligible) return;
    if (issue || allocation.issue) {
      setError(issue || allocation.issue);
      return;
    }
    busyRef.current = true;
    setBusy(true);
    setError("");
    setConflict(false);
    const payload = {
      receipt_id: source.id,
      order_version: order.data.version,
      supplier_invoice_number: draft.supplier_invoice_number.trim(),
      invoice_date: draft.invoice_date,
      due_date: draft.due_date,
      notes: draft.notes.trim() || null,
      items: allocation.items,
    };
    identity.current = requestIdentity(identity.current, payload);
    try {
      const bill = await api<SupplierBill>("/purchase-bills", {
        method: "POST",
        body: { ...payload, idempotency_key: identity.current.key },
      });
      onSaved(bill.id);
    } catch (error) {
      setError(errorMessage(error));
      setConflict(error instanceof ApiError && error.status === 409);
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  return (
    <Modal
      wide
      title="Catat tagihan supplier"
      description="Pilih jumlah barang yang akan ditagih dari penerimaan PO."
      onClose={() => {
        if (!busyRef.current) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body bill-form-body">
          {orders.loading ? (
            <Loading />
          ) : orders.error ? (
            <ErrorState message={orders.error} onRetry={orders.reload} />
          ) : (
            <FormField
              label="Penerimaan barang *"
              hint="Pilihan mencakup penerimaan yang belum ditagih dari 500 PO terbaru. Penerimaan lama dapat dibuka dari detail PO."
            >
              <select
                required
                value={orderId && source ? `${orderId}|${source.id}` : ""}
                disabled={busy}
                onChange={(event) => {
                  const [nextOrder, nextReceipt] =
                    event.target.value.split("|");
                  setOrderId(nextOrder ?? "");
                  setReceiptId(nextReceipt ?? "");
                  setQuantities({});
                  setError("");
                  setConflict(false);
                }}
              >
                <option value="">Pilih penerimaan PO…</option>
                {sources.map((item) => (
                  <option
                    value={`${item.order.id}|${item.receipt.id}`}
                    key={item.receipt.id}
                  >
                    {item.receipt.number} · {item.order.number} ·{" "}
                    {item.order.supplier_name}
                  </option>
                ))}
                {initialOrderId &&
                  !sources.some((item) => item.order.id === initialOrderId) && (
                    <option
                      value={`${initialOrderId}|${source?.id ?? initialReceiptId ?? ""}`}
                    >
                      Penerimaan dari PO yang dibuka
                    </option>
                  )}
              </select>
            </FormField>
          )}
          {!orders.loading &&
            !orders.error &&
            !sources.length &&
            !initialOrderId && (
              <p className="bill-notice">
                <Info size={16} />
                Belum ada penerimaan yang dapat ditagih. Catat penerimaan dari
                PO terlebih dahulu.
              </p>
            )}
          {summary.loading ? (
            <Loading label="Memuat tanggal perusahaan…" />
          ) : summary.error ? (
            <ErrorState message={summary.error} onRetry={summary.reload} />
          ) : (
            summary.data && (
              <p className="bill-date-context">
                Hari ini: {calendarDate(summary.data.today)} ·{" "}
                {summary.data.timezone}. Tanggal invoice boleh lebih awal;
                jurnal dicatat saat tagihan diposting.
              </p>
            )
          )}
          {orderId &&
            (order.loading ? (
              <Loading label="Memeriksa penerimaan terbaru…" />
            ) : order.error ? (
              <ErrorState message={order.error} onRetry={order.reload} />
            ) : (
              order.data && (
                <>
                  <div className="bill-source-summary">
                    <span>
                      <small>Supplier</small>
                      <strong>{order.data.supplier_name}</strong>
                      <small>{order.data.supplier_code}</small>
                    </span>
                    <span>
                      <small>Dokumen asal</small>
                      <strong>
                        {source?.number ?? "Tidak ada penerimaan aktif"}
                      </strong>
                      <small>{order.data.number}</small>
                    </span>
                  </div>
                  {source && (
                    <>
                      <div className="table-scroll">
                        <table>
                          <thead>
                            <tr>
                              <th>Barang</th>
                              <th>Sisa belum ditagih</th>
                              <th>Jumlah ditagih</th>
                            </tr>
                          </thead>
                          <tbody>
                            {source.items
                              .filter(
                                (line) =>
                                  Number(line.remaining_bill_quantity) > 0,
                              )
                              .map((line) => (
                                <tr key={line.id}>
                                  <td>
                                    {line.product_name}
                                    <small className="bill-cell-sub">
                                      {line.unit}
                                    </small>
                                  </td>
                                  <td>
                                    {exactDecimal(line.remaining_bill_quantity)}
                                  </td>
                                  <td>
                                    <input
                                      aria-label={`Jumlah tagihan ${line.product_name}`}
                                      inputMode="decimal"
                                      value={
                                        quantities[line.id] ??
                                        line.remaining_bill_quantity
                                      }
                                      disabled={busy}
                                      onChange={(e) =>
                                        setQuantities((previous) => ({
                                          ...previous,
                                          [line.id]: e.target.value,
                                        }))
                                      }
                                    />
                                  </td>
                                </tr>
                              ))}
                          </tbody>
                        </table>
                      </div>
                      <div className="purchase-detail-total">
                        <span>Total tagihan</span>
                        <strong>{exactMoney(allocation.total)}</strong>
                      </div>
                      <FormError error={allocation.issue} />
                    </>
                  )}
                  {!source?.can_bill && (
                    <p className="bill-blocked">
                      {source?.bill_blocked_reason ||
                        "Penerimaan ini belum dapat ditagih. Muat data terbaru atau pilih penerimaan lain."}
                    </p>
                  )}
                </>
              )
            ))}
          <div className="bill-form-grid">
            <FormField
              label="Nomor invoice supplier *"
              hint="Sesuai dokumen supplier; unik untuk tagihan aktif dari supplier yang sama."
            >
              <input
                required
                maxLength={100}
                value={draft.supplier_invoice_number}
                disabled={busy}
                onChange={(event) =>
                  set("supplier_invoice_number", event.target.value)
                }
                placeholder="Contoh: INV-SUP-012"
              />
            </FormField>
            <FormField label="Tanggal invoice *">
              <input
                required
                type="date"
                min="0001-01-01"
                max={summary.data?.today ?? "9999-12-31"}
                value={draft.invoice_date}
                disabled={busy}
                onChange={(event) => set("invoice_date", event.target.value)}
              />
            </FormField>
            <FormField label="Jatuh tempo *">
              <input
                required
                type="date"
                min={draft.invoice_date || "0001-01-01"}
                max="9999-12-31"
                value={draft.due_date}
                disabled={busy}
                onChange={(event) => set("due_date", event.target.value)}
              />
            </FormField>
          </div>
          <FormField label="Catatan" hint="Opsional, maksimal 2.000 karakter.">
            <textarea
              rows={3}
              maxLength={2000}
              value={draft.notes}
              disabled={busy}
              onChange={(event) => set("notes", event.target.value)}
            />
          </FormField>
          <div className="bill-posting-note">
            <FileText size={19} />
            <div>
              <strong>Ubah kewajiban sementara menjadi utang supplier.</strong>
              <p>
                Jumlah dan biaya mengikuti penerimaan; nilai tagihan tidak dapat
                diubah. Kewajiban barang diterima belum ditagih berkurang dan
                utang supplier bertambah sebesar {exactMoney(allocation.total)}.
                Stok dan kas tetap. Setelah posting, pembayaran dicatat melalui
                detail tagihan.
              </p>
            </div>
          </div>
          <FormError error={error} />
          {conflict && (
            <button
              type="button"
              className="text-button"
              disabled={busy}
              onClick={() => {
                order.reload();
                orders.reload();
                summary.reload();
                setConflict(false);
                setError(
                  "Data terbaru dimuat. Periksa kembali penerimaan, lalu tinjau tagihan sebelum mencoba lagi.",
                );
              }}
            >
              <RefreshCw size={15} /> Muat penerimaan terbaru
            </button>
          )}
        </div>
        <div className="form-actions">
          <button
            type="button"
            className="button secondary"
            disabled={busy}
            onClick={onClose}
          >
            Batal
          </button>
          <button
            className="button primary"
            disabled={
              busy ||
              !eligible ||
              !!issue ||
              !!allocation.issue ||
              summary.loading ||
              !!summary.error
            }
          >
            {busy ? "Mencatat…" : "Catat tagihan"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function useBillSource(id: string, revision: number) {
  const [data, setData] = useState<PurchaseOrder | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    setData(null);
    setError("");
    setLoading(!!id);
    if (id)
      api<PurchaseOrder>(`/purchase-orders/${id}`, {
        signal: controller.signal,
      })
        .then((result) => {
          if (!controller.signal.aborted) setData(result);
        })
        .catch((error) => {
          if (!controller.signal.aborted) setError(errorMessage(error));
        })
        .finally(() => {
          if (!controller.signal.aborted) setLoading(false);
        });
    return () => controller.abort();
  }, [id, revision, retry]);
  return { data, loading, error, reload: () => setRetry((value) => value + 1) };
}

function BillItems({
  items,
  total,
}: {
  items: PurchaseReceipt["items"];
  total: string;
}) {
  return (
    <>
      <div className="table-scroll">
        <table className="purchase-detail-lines">
          <thead>
            <tr>
              <th>Barang dari penerimaan</th>
              <th className="align-right">Jumlah</th>
              <th className="align-right">Biaya / satuan</th>
              <th className="align-right">Total</th>
            </tr>
          </thead>
          <tbody>
            {items.map((item) => (
              <tr key={item.id}>
                <td>
                  <span className="purchase-cell">
                    <strong>{item.product_name}</strong>
                    <small>{item.product_sku}</small>
                  </span>
                </td>
                <td className="align-right">
                  {exactDecimal(item.quantity)} {item.unit}
                </td>
                <td className="align-right">{exactCost(item.unit_cost)}</td>
                <td className="align-right">{exactMoney(item.value)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="purchase-detail-total">
        <span>Total tagihan</span>
        <strong>{exactMoney(total)}</strong>
      </div>
    </>
  );
}

export function SupplierBillDetail({
  id,
  revision,
  canManage,
  onClose,
  onSaved,
  onOpenOrder,
}: {
  id: string;
  revision: number;
  canManage: boolean;
  onClose: () => void;
  onSaved: () => void;
  onOpenOrder: (id: string, receiptId?: string) => void;
}) {
  const resource = useResource<SupplierBill>(`/purchase-bills/${id}`, revision);
  const bill = resource.data;
  const [review, setReview] = useState(false);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  const [error, setError] = useState("");
  const [conflict, setConflict] = useState(false);
  const [journalId, setJournalId] = useState<string | null>(null);
  const reviewRef = useRef<HTMLFormElement>(null);
  useEffect(() => {
    if (review)
      reviewRef.current?.scrollIntoView({
        block: "nearest",
        behavior: "smooth",
      });
  }, [review]);
  async function reverse(event: FormEvent) {
    event.preventDefault();
    if (busyRef.current || !bill || !bill.can_reverse) return;
    if (reason.trim().length < 3 || reason.trim().length > 1000) {
      setError("Alasan harus berisi 3 sampai 1.000 karakter.");
      return;
    }
    busyRef.current = true;
    setBusy(true);
    setError("");
    setConflict(false);
    const payload = { version: bill.version, reason: reason.trim() };
    identity.current = requestIdentity(identity.current, { id, ...payload });
    try {
      await api(`/purchase-bills/${id}/reverse`, {
        method: "POST",
        body: { ...payload, idempotency_key: identity.current.key },
      });
      setReview(false);
      setReason("");
      resource.reload();
      onSaved();
    } catch (error) {
      setError(errorMessage(error));
      setConflict(error instanceof ApiError && error.status === 409);
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  return (
    <Modal
      wide
      title={bill?.number ?? "Detail tagihan supplier"}
      description="Tagihan dan jejak koreksi tersimpan untuk riwayat usaha."
      onClose={() => {
        if (!busyRef.current) onClose();
      }}
    >
      {resource.loading ? (
        <Loading />
      ) : resource.error ? (
        <ErrorState message={resource.error} onRetry={resource.reload} />
      ) : (
        bill && (
          <>
            <div className="purchase-detail-head">
              <div>
                <small>SUPPLIER</small>
                <h3>{bill.supplier_name}</h3>
                <p>{bill.supplier_code}</p>
              </div>
              <BillStatus bill={bill} />
            </div>
            <div className="purchase-detail-meta">
              <div>
                <small>Invoice supplier</small>
                <span>{bill.supplier_invoice_number}</span>
              </div>
              <div>
                <small>Tanggal invoice</small>
                <span>{calendarDate(bill.invoice_date)}</span>
              </div>
              <div>
                <small>Jatuh tempo</small>
                <span>{calendarDate(bill.due_date)}</span>
                <span className={`bill-due-label ${bill.due_status}`}>
                  {billDueNames[bill.due_status]}
                </span>
              </div>
              <div>
                <small>Dicatat</small>
                <span>{dateTime(bill.created_at)}</span>
              </div>
              <div>
                <small>Pesanan asal</small>
                <button
                  className="table-link"
                  onClick={() => onOpenOrder(bill.order_id)}
                >
                  {bill.order_number}
                </button>
              </div>
              <div>
                <small>Penerimaan asal</small>
                <button
                  className="table-link"
                  onClick={() => onOpenOrder(bill.order_id, bill.receipt_id)}
                >
                  {bill.receipt_number}
                </button>
              </div>
            </div>
            <BillItems items={bill.items} total={bill.total} />
            <AttachmentsPanel
              documentType="purchase_bill"
              documentId={bill.id}
              title="Lampiran invoice supplier"
            />
            <SupplierPayments
              bill={bill}
              canManage={canManage}
              onSaved={() => {
                resource.reload();
                onSaved();
              }}
              onJournal={setJournalId}
            />
            <ReturnsPanel
              kind="supplier"
              documentId={bill.id}
              version={bill.version}
              total={bill.total}
              paid={bill.paid}
              creditTotal={bill.credit_total}
              items={bill.items}
              returns={bill.returns}
              canReturn={bill.can_return}
              canManage={canManage}
              onSaved={() => {
                resource.reload();
                onSaved();
              }}
              onJournal={setJournalId}
            />
            <div className="bill-balance">
              <span>
                Dibayar <strong>{exactMoney(bill.paid)}</strong>
              </span>
              <span>
                Sisa utang <strong>{exactMoney(bill.outstanding)}</strong>
              </span>
            </div>
            <div className="purchase-detail-notes">
              {bill.notes && (
                <>
                  <small>Catatan</small>
                  <p className="purchase-prewrap">{bill.notes}</p>
                </>
              )}
              <p className="purchase-history-note">
                Nama supplier, barang, satuan, dan biaya menggunakan data
                tersimpan. Tagihan memindahkan kewajiban barang diterima belum
                ditagih menjadi utang supplier; stok dan kas tidak berubah.
                Pembayaran dan koreksinya tersimpan pada riwayat di bawah.
              </p>
              <div className="purchase-receipt-actions">
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() => setJournalId(bill.journal_id)}
                >
                  Lihat jurnal tagihan <ArrowRight size={13} />
                </button>
                {bill.reversal && (
                  <button
                    className="text-button"
                    disabled={busy}
                    onClick={() => setJournalId(bill.reversal!.journal_id)}
                  >
                    Lihat jurnal pembalikan <ArrowRight size={13} />
                  </button>
                )}
              </div>
              {bill.reversal && (
                <div className="purchase-receipt-reversal">
                  <strong>
                    Dibatalkan pada {dateTime(bill.reversal.created_at)}
                  </strong>
                  <p className="purchase-prewrap">
                    Alasan: {bill.reversal.reason}
                  </p>
                  <p>
                    Penerimaan tetap tersimpan. Tagihan pengganti dapat dibuat
                    jika penerimaan masih aktif dan periode terbuka.
                  </p>
                </div>
              )}
              {!bill.can_reverse &&
                bill.status !== "void" &&
                bill.reversal_blocked_reason && (
                  <p className="bill-blocked">{bill.reversal_blocked_reason}</p>
                )}
            </div>
            {review && (
              <form
                className="purchase-review"
                onSubmit={reverse}
                ref={reviewRef}
              >
                <div className="purchase-review-title">
                  <Undo2 size={19} />
                  <strong>Batalkan tagihan {bill.number}</strong>
                </div>
                <p>
                  Jurnal tagihan {exactMoney(bill.total)} dibalik: utang
                  supplier berkurang dan kewajiban barang diterima belum ditagih
                  kembali bertambah. Barang tetap di gudang. Dokumen dan
                  alasannya tetap tersimpan.
                </p>
                <FormField
                  label="Alasan pembatalan *"
                  hint="3–1.000 karakter; tersimpan dalam riwayat."
                >
                  <textarea
                    required
                    minLength={3}
                    maxLength={1000}
                    rows={3}
                    disabled={busy}
                    value={reason}
                    onChange={(event) => setReason(event.target.value)}
                    placeholder="Jelaskan kesalahan pada tagihan"
                    autoFocus
                  />
                </FormField>
                <FormError error={error} />
                <div className="purchase-review-actions">
                  <button
                    type="button"
                    className="button secondary"
                    disabled={busy}
                    onClick={() => {
                      setReview(false);
                      setError("");
                      setConflict(false);
                    }}
                  >
                    Kembali ke detail
                  </button>
                  <button
                    type="submit"
                    className="button secondary danger-text"
                    disabled={busy}
                  >
                    {busy ? "Memproses…" : "Batalkan tagihan"}
                  </button>
                </div>
              </form>
            )}
            {!review && error && (
              <div className="purchase-action-error">
                <FormError error={error} />
              </div>
            )}
            {conflict && (
              <div className="purchase-action-error">
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() => {
                    setReview(false);
                    setConflict(false);
                    resource.reload();
                    setError(
                      "Data terbaru dimuat. Tinjau status tagihan sebelum mencoba kembali.",
                    );
                  }}
                >
                  <RefreshCw size={15} /> Muat tagihan terbaru
                </button>
              </div>
            )}
          </>
        )
      )}
      <div className="form-actions">
        <button className="button secondary" disabled={busy} onClick={onClose}>
          Tutup
        </button>
        {canManage &&
          bill?.can_reverse &&
          !review &&
          !resource.loading &&
          !resource.error && (
            <button
              className="button secondary danger-text"
              disabled={busy}
              onClick={() => {
                setReview(true);
                setError("");
              }}
            >
              <Undo2 size={16} /> Tinjau pembatalan
            </button>
          )}
      </div>
      {journalId && (
        <JournalPreview id={journalId} onClose={() => setJournalId(null)} />
      )}
    </Modal>
  );
}
