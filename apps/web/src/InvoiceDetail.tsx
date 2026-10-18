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
import type { Customer, Invoice, Order, Session } from "./types";

interface Payment {
  id: string;
  amount: string;
  reference?: string;
  created_at: string;
  reversed: boolean;
  reversal_reason?: string;
}

export default function InvoiceDetail({
  invoice,
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
  const order = useResource<Order>(`/orders/${invoice.order_id}`);
  const customers = useResource<{ items: Customer[] }>("/customers");
  const payments = useResource<{ items: Payment[] }>(
    `/invoices/${invoice.id}/payments`,
  );
  const [reversal, setReversal] = useState<{
    path: string;
    title: string;
  } | null>(null);
  const customer = customers.data?.items.find(
    (customer) =>
      customer.id === invoice.customer_id ||
      customer.id === order.data?.customer_id,
  );
  const canManage = ["admin", "finance"].includes(session.user.role);
  const error = order.error || customers.error || payments.error;
  return (
    <Modal
      title={invoice.number}
      description="Invoice penjualan dan riwayat pembayaran."
      onClose={onClose}
      wide
    >
      {order.loading || customers.loading || payments.loading ? (
        <Loading />
      ) : error ? (
        <ErrorState
          message={error}
          onRetry={() => {
            order.reload();
            customers.reload();
            payments.reload();
          }}
        />
      ) : (
        <>
          <div className="invoice-print">
            <div className="invoice-document-heading">
              <div>
                <span className="eyebrow">HARUFLOW COMMUNITY</span>
                <h2>{session.company.name}</h2>
                <p>Invoice penjualan</p>
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
                  {order.data?.items?.map((item, index) => (
                    <tr key={index}>
                      <td>
                        <strong>
                          {item.product_name ??
                            (item as typeof item & { name?: string }).name ??
                            item.sku}
                        </strong>
                        <small className="block muted">{item.sku}</small>
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
                <strong>{money(invoice.total)}</strong>
              </div>
              <div>
                <span>Sudah dibayar</span>
                <strong>{money(invoice.paid)}</strong>
              </div>
              <div className="invoice-due">
                <span>Sisa tagihan</span>
                <strong>
                  {money(
                    invoice.status === "void"
                      ? 0
                      : (invoice.outstanding ??
                          Number(invoice.total) - Number(invoice.paid)),
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
                          {canManage && !payment.reversed && (
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
          <div className="form-actions invoice-actions">
            {canManage && invoice.status !== "void" && (
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
            onChanged();
          }}
        />
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
