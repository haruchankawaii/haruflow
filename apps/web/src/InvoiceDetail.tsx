import { useState, type FormEvent } from "react";
import { CreditCard, Printer, RotateCcw } from "lucide-react";
import { api, errorMessage } from "./api";
import {
  EmptyState,
  ErrorState,
  FormActions,
  FormError,
  FormField,
  Loading,
  Modal,
  Status,
} from "./components";
import { date, lineTotal, money, number } from "./format";
import { useResource } from "./hooks";
import type { Company, Customer, Invoice, Order, Session } from "./types";
import { exactMoney, moneyCents } from "./dashboard-charts";
import { JournalPreview } from "./JournalPreview";
import { ReturnsPanel } from "./ReturnsPanel";
import type { ReturnableDocument, ReturnableItem } from "./returns-model";

type DetailedInvoice = Omit<Invoice, "items"> &
  Omit<ReturnableDocument, "items"> & {
    items: (ReturnableItem & {
      unit_price: string;
      product_id: string;
      total: string;
    })[];
    can_reverse?: boolean;
    reversal_blocked_reason?: string | null;
  };

interface Payment {
  id: string;
  amount: string;
  reference?: string;
  created_at: string;
  reversed: boolean;
  reversal_reason?: string;
}

export default function InvoiceDetail({
  invoice: initialInvoice,
  session,
  onClose,
  onChanged,
  onPay,
}: {
  invoice: Invoice;
  session: Session;
  onClose: () => void;
  onChanged: () => void;
  onPay: () => void;
}) {
  const detail = useResource<DetailedInvoice>(`/invoices/${initialInvoice.id}`);
  const invoice = detail.data ?? initialInvoice;
  const order = useResource<Order>(`/orders/${initialInvoice.order_id}`);
  const customers = useResource<{ items: Customer[] }>("/customers");
  const company = useResource<Company>("/company");
  const payments = useResource<{ items: Payment[] }>(
    `/invoices/${invoice.id}/payments`,
  );
  const [reversal, setReversal] = useState<{
    path: string;
    title: string;
  } | null>(null);
  const [journal, setJournal] = useState<string | null>(null);
  const customer = customers.data?.items.find(
    (customer) =>
      customer.id === invoice.customer_id ||
      customer.id === order.data?.customer_id,
  );
  const canManage = ["admin", "finance"].includes(session.user.role);
  const error =
    detail.error ||
    order.error ||
    customers.error ||
    payments.error ||
    company.error;
  const activeReturns =
    detail.data?.returns.some((record) => !record.reversal) ?? false;
  function refreshed() {
    detail.reload();
    payments.reload();
    onChanged();
  }
  return (
    <Modal
      title={invoice.number}
      description="Invoice penjualan, pembayaran, dan riwayat retur."
      onClose={onClose}
      wide
    >
      {detail.loading ||
      order.loading ||
      customers.loading ||
      payments.loading ||
      company.loading ? (
        <Loading />
      ) : error ? (
        <ErrorState
          message={error}
          onRetry={() => {
            order.reload();
            detail.reload();
            customers.reload();
            payments.reload();
            company.reload();
          }}
        />
      ) : (
        <>
          <div className="invoice-print">
            <div className="invoice-document-heading">
              <div>
                <span className="eyebrow">HARUFLOW COMMUNITY</span>
                <h2>{company.data?.name ?? session.company.name}</h2>
                {company.data?.address && (
                  <p className="preserve-lines">{company.data.address}</p>
                )}
                {company.data?.email && <p>{company.data.email}</p>}
                {company.data?.phone && <p>{company.data.phone}</p>}
                {company.data?.tax_id && (
                  <p>NPWP / ID Pajak: {company.data.tax_id}</p>
                )}
              </div>
              <div className="invoice-document-number">
                <h3>INVOICE</h3>
                <strong>{invoice.number}</strong>
                <span>{date(invoice.created_at)}</span>
                <Status status={invoice.status} />
              </div>
            </div>
            <div className="invoice-customer">
              <small>DITAGIHKAN KEPADA</small>
              <strong>{invoice.customer_name}</strong>
              {customer?.address && <p>{customer.address}</p>}
              {customer?.email && <p>{customer.email}</p>}
              {customer?.phone && <p>{customer.phone}</p>}
              <p className="invoice-reference">
                Pesanan: {order.data?.number ?? "—"}
              </p>
            </div>
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Produk</th>
                    <th className="align-right">Jumlah</th>
                    <th className="align-right">Harga satuan</th>
                    <th className="align-right">Subtotal</th>
                  </tr>
                </thead>
                <tbody>
                  {(invoice.items ?? order.data?.items)?.map((item, index) => (
                    <tr key={index}>
                      <td>
                        <strong>
                          {item.product_name ??
                            (item as typeof item & { name?: string }).name ??
                            item.sku}
                        </strong>
                        <small className="block muted">{item.sku}</small>
                        {item.product_kind === "service" && (
                          <span className="subtle-chip">Jasa</span>
                        )}
                      </td>
                      <td className="align-right">{number(item.quantity)}</td>
                      <td className="align-right">{money(item.unit_price)}</td>
                      <td className="align-right">
                        {money(
                          item.total ??
                            lineTotal(item.quantity, item.unit_price),
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="invoice-totals">
              <div>
                <span>Total invoice</span>
                <strong>{exactMoney(invoice.total)}</strong>
              </div>
              {detail.data && moneyCents(detail.data.credit_total) > 0n && (
                <>
                  <div>
                    <span>Kredit retur</span>
                    <strong>−{exactMoney(detail.data.credit_total)}</strong>
                  </div>
                  <div>
                    <span>Nilai invoice bersih</span>
                    <strong>{exactMoney(detail.data.net_total)}</strong>
                  </div>
                </>
              )}
              <div>
                <span>Pembayaran bersih</span>
                <strong>{exactMoney(invoice.paid)}</strong>
              </div>
              <div className="invoice-due">
                <span>Sisa tagihan</span>
                <strong>
                  {exactMoney(
                    invoice.status === "void"
                      ? 0n
                      : (invoice.outstanding ??
                          moneyCents(detail.data?.net_total ?? invoice.total) -
                            moneyCents(invoice.paid)),
                  )}
                </strong>
              </div>
            </div>
            <p className="invoice-thanks">
              Terima kasih atas kepercayaan Anda.
            </p>
          </div>
          <section className="invoice-payments">
            <h3>
              <CreditCard size={17} /> Riwayat pembayaran
            </h3>
            {payments.data?.items.length ? (
              <div className="table-scroll">
                <table>
                  <thead>
                    <tr>
                      <th>Tanggal</th>
                      <th>Referensi</th>
                      <th className="align-right">Jumlah</th>
                      <th>Status</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {payments.data.items.map((payment) => (
                      <tr key={payment.id}>
                        <td>{date(payment.created_at)}</td>
                        <td className="muted">{payment.reference || "—"}</td>
                        <td className="align-right">{money(payment.amount)}</td>
                        <td>
                          <Status
                            status={payment.reversed ? "cancelled" : "paid"}
                          />
                        </td>
                        <td>
                          {canManage && !payment.reversed && !activeReturns && (
                            <button
                              className="text-button danger-text"
                              onClick={() =>
                                setReversal({
                                  path: `/invoices/${invoice.id}/payments/${payment.id}/reverse`,
                                  title: "Batalkan pembayaran",
                                })
                              }
                            >
                              Batalkan
                            </button>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <EmptyState
                title="Belum ada pembayaran"
                description="Pembayaran pelanggan akan tercatat di sini."
              />
            )}
          </section>
          {activeReturns && (
            <p className="muted form-body">
              Pembayaran bersih sudah memperhitungkan pengembalian dana retur.
              Balikkan retur aktif lebih dahulu sebelum membatalkan pembayaran
              atau invoice.
            </p>
          )}
          {detail.data && (
            <ReturnsPanel
              kind="sales"
              documentId={invoice.id}
              version={detail.data.version}
              total={invoice.total}
              paid={invoice.paid}
              creditTotal={detail.data.credit_total}
              items={detail.data.items}
              returns={detail.data.returns}
              canReturn={detail.data.can_return}
              canManage={canManage}
              onSaved={refreshed}
              onJournal={setJournal}
            />
          )}
          <div className="form-actions invoice-actions">
            {canManage &&
              invoice.status !== "void" &&
              !activeReturns &&
              detail.data?.can_reverse !== false && (
                <button
                  className="text-button danger-text"
                  onClick={() =>
                    setReversal({
                      path: `/invoices/${invoice.id}/reverse`,
                      title: "Batalkan invoice",
                    })
                  }
                >
                  <RotateCcw size={15} /> Batalkan invoice
                </button>
              )}
            <button className="button secondary" onClick={() => window.print()}>
              <Printer size={16} /> Cetak / PDF
            </button>
            {canManage && !["paid", "void"].includes(invoice.status) && (
              <button className="button primary" onClick={onPay}>
                <CreditCard size={16} /> Catat pembayaran
              </button>
            )}
          </div>
        </>
      )}
      {reversal && (
        <ReasonForm
          path={reversal.path}
          title={reversal.title}
          onClose={() => setReversal(null)}
          onSaved={() => {
            setReversal(null);
            refreshed();
          }}
        />
      )}
      {journal && (
        <JournalPreview id={journal} onClose={() => setJournal(null)} />
      )}
    </Modal>
  );
}

export function ReasonForm({
  path,
  title,
  onClose,
  onSaved,
}: {
  path: string;
  title: string;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [key] = useState(() => crypto.randomUUID());
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api(path, {
        method: "POST",
        body: {
          reason: new FormData(event.currentTarget).get("reason"),
          idempotency_key: key,
        },
      });
      onSaved();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={title}
      description="Pembatalan dicatat sebagai transaksi balik agar jejak audit tetap utuh."
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <FormField label="Alasan pembatalan *">
            <textarea
              name="reason"
              required
              rows={3}
              minLength={3}
              maxLength={1000}
              autoFocus
              placeholder="Jelaskan alasan koreksi transaksi"
            />
          </FormField>
          <FormField label="Konfirmasi">
            <span className="checkbox-field">
              <input type="checkbox" required /> Saya sudah memeriksa transaksi
              yang akan dibatalkan.
            </span>
          </FormField>
          <FormError error={error} />
        </div>
        <FormActions onClose={onClose} busy={busy} label="Proses pembatalan" />
      </form>
    </Modal>
  );
}
