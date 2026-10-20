import { useState, type FormEvent } from "react";
import { Check, FileText, Truck } from "lucide-react";
import { api, errorMessage } from "./api";
import {
  ErrorState,
  FormActions,
  FormError,
  FormField,
  Loading,
  Modal,
  Status,
} from "./components";
import { date, money, number, toMinorUnits } from "./format";
import { useResource } from "./hooks";
import { ReasonForm } from "./InvoiceDetail";
import type { Order, OrderLine, Role } from "./types";
import { AttachmentsPanel } from "./AttachmentsPanel";

export default function OrderDetail({
  id,
  role,
  onClose,
  onChanged,
}: {
  id: string;
  role: Role;
  onClose: () => void;
  onChanged: () => void;
}) {
  const order = useResource<Order>(`/orders/${id}`);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [key] = useState(() => crypto.randomUUID());
  const [action, setAction] = useState<"ship" | "invoice" | null>(null);
  const [reversal, setReversal] = useState<{
    path: string;
    title: string;
  } | null>(null);
  const data = order.data;
  const canShip = ["admin", "warehouse"].includes(role);
  const canInvoice = ["admin", "finance"].includes(role);
  const canSell = ["admin", "sales"].includes(role);
  function saved() {
    setAction(null);
    setReversal(null);
    order.reload();
    onChanged();
  }
  async function confirm() {
    setBusy(true);
    setError("");
    try {
      await api(`/orders/${id}/confirm`, {
        method: "POST",
        body: { idempotency_key: key },
      });
      saved();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={data?.number ?? "Detail pesanan"}
      description="Kirim dan tagihkan bertahap; sisa pesanan tetap terlacak."
      wide
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      {order.loading ? (
        <Loading />
      ) : order.error || !data ? (
        <ErrorState message={order.error} onRetry={order.reload} />
      ) : (
        <>
          <div className="form-body">
            <div className="order-detail-meta">
              <div>
                <small>Pelanggan</small>
                <strong>{data.customer_name}</strong>
              </div>
              <div>
                <small>Tanggal</small>
                <strong>{date(data.created_at)}</strong>
              </div>
              <div>
                <small>Status</small>
                <Status status={data.status} />
              </div>
            </div>
            <div className="table-scroll detail-table">
              <table>
                <thead>
                  <tr>
                    <th>Produk</th>
                    <th className="align-right">Dipesan</th>
                    <th className="align-right">Dikirim</th>
                    <th className="align-right">Sisa kirim</th>
                    <th className="align-right">Ditagihkan</th>
                    <th className="align-right">Siap ditagih</th>
                    <th className="align-right">Subtotal</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items?.map((item) => (
                    <tr key={item.id}>
                      <td>
                        <strong>
                          {item.product_name ??
                            (item as OrderLine & { name?: string }).name ??
                            item.sku}
                        </strong>
                        <small className="block muted">
                          {item.sku}
                          {item.product_kind === "service" ? " · Jasa" : ""}
                        </small>
                      </td>
                      <td className="align-right">{number(item.quantity)}</td>
                      <td className="align-right">
                        {item.product_kind === "service"
                          ? "—"
                          : number(item.shipped_quantity)}
                      </td>
                      <td className="align-right">
                        {item.product_kind === "service"
                          ? "—"
                          : number(item.unshipped_quantity)}
                      </td>
                      <td className="align-right">
                        {number(item.invoiced_quantity)}
                      </td>
                      <td className="align-right">
                        {number(item.invoiceable_quantity)}
                      </td>
                      <td className="align-right">{money(item.total)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="order-total">
              <span>Total pesanan</span>
              <strong>{money(data.total)}</strong>
            </div>
            <AttachmentsPanel
              documentType="sales_order"
              documentId={data.id}
              title="Lampiran pesanan penjualan"
            />
            <p className="muted">
              Sisa kirim adalah jumlah barang yang masih harus dikirim. Barang
              siap ditagih setelah dikirim; jasa dapat ditagih setelah
              konfirmasi. Status Ditagihkan muncul setelah seluruh jumlah
              pesanan ditagih.
            </p>
            {!!data.shipments?.length && (
              <section>
                <h3>Riwayat pengiriman</h3>
                <div className="table-scroll">
                  <table>
                    <thead>
                      <tr>
                        <th>Tanggal</th>
                        <th>Barang</th>
                        <th>Status</th>
                        <th />
                      </tr>
                    </thead>
                    <tbody>
                      {data.shipments.map((shipment, index) => (
                        <tr key={shipment.id}>
                          <td>
                            <strong>Pengiriman {index + 1}</strong>
                            <small className="block muted">
                              {date(shipment.created_at)}
                            </small>
                          </td>
                          <td>
                            {shipment.items
                              .map(
                                (item) =>
                                  `${item.sku}: ${number(item.quantity)}`,
                              )
                              .join(" · ")}
                            {shipment.reversal_reason && (
                              <small className="block muted">
                                {shipment.reversal_reason}
                              </small>
                            )}
                          </td>
                          <td>
                            <Status
                              status={shipment.reversed ? "void" : "shipped"}
                            />
                          </td>
                          <td>
                            {canShip && shipment.can_reverse && (
                              <button
                                className="text-button danger-text"
                                onClick={() =>
                                  setReversal({
                                    path: `/orders/${id}/shipments/${shipment.id}/reverse`,
                                    title: `Batalkan pengiriman ${index + 1}`,
                                  })
                                }
                              >
                                Batalkan
                              </button>
                            )}
                            {!shipment.reversed && !shipment.can_reverse && (
                              <small className="muted">
                                Masih terkait invoice aktif
                              </small>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </section>
            )}
            {!!data.invoices?.length && (
              <section>
                <h3>Riwayat invoice</h3>
                <div className="table-scroll">
                  <table>
                    <thead>
                      <tr>
                        <th>Invoice</th>
                        <th>Tanggal</th>
                        <th className="align-right">Jumlah</th>
                        <th>Status</th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.invoices.map((invoice) => (
                        <tr key={invoice.id}>
                          <td>{invoice.number}</td>
                          <td>{date(invoice.created_at)}</td>
                          <td className="align-right">
                            {money(invoice.total)}
                          </td>
                          <td>
                            <Status status={invoice.status} />
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                <p className="muted">
                  Buka tab Invoice untuk mencetak, membayar, atau membatalkan
                  invoice tertentu.
                </p>
              </section>
            )}
            <FormError error={error} />
          </div>
          <div className="form-actions">
            {canSell &&
              ["draft", "confirmed"].includes(data.status) &&
              !data.shipments?.some((s) => !s.reversed) &&
              !data.invoices?.some((i) => i.status !== "void") && (
                <button
                  className="text-button danger-text"
                  disabled={busy}
                  onClick={() =>
                    setReversal({
                      path: `/orders/${id}/cancel`,
                      title: "Batalkan pesanan",
                    })
                  }
                >
                  Batalkan pesanan
                </button>
              )}
            <button
              className="button secondary"
              disabled={busy}
              onClick={onClose}
            >
              Tutup
            </button>
            {canSell && data.status === "draft" && (
              <button
                className="button primary"
                disabled={busy}
                onClick={confirm}
              >
                <Check size={17} />
                Konfirmasi pesanan
              </button>
            )}
            {canShip && data.can_ship && (
              <button
                className="button primary"
                onClick={() => setAction("ship")}
              >
                <Truck size={17} />
                Kirim sebagian / sisa
              </button>
            )}
            {canInvoice && data.can_invoice && (
              <button
                className="button primary"
                onClick={() => setAction("invoice")}
              >
                <FileText size={17} />
                Tagihkan sebagian / sisa
              </button>
            )}
          </div>
          {action && (
            <AllocationForm
              order={data}
              action={action}
              onClose={() => setAction(null)}
              onSaved={saved}
              onReload={order.reload}
            />
          )}
          {reversal && (
            <ReasonForm
              {...reversal}
              onClose={() => setReversal(null)}
              onSaved={saved}
            />
          )}
        </>
      )}
    </Modal>
  );
}

function AllocationForm({
  order,
  action,
  onClose,
  onSaved,
  onReload,
}: {
  order: Order;
  action: "ship" | "invoice";
  onClose: () => void;
  onSaved: () => void;
  onReload: () => void;
}) {
  const lines =
    order.items?.filter(
      (item) =>
        Number(
          action === "ship"
            ? item.unshipped_quantity
            : item.invoiceable_quantity,
        ) > 0,
    ) ?? [];
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      lines.map((line) => [
        line.id!,
        action === "ship"
          ? line.unshipped_quantity!
          : line.invoiceable_quantity!,
      ]),
    ),
  );
  const [key, setKey] = useState(() => crypto.randomUUID());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const selected = lines.filter((line) => Number(values[line.id!] ?? 0) > 0);
  const invoiceMinor = selected.reduce((sum, line) => {
    const quantity = toMinorUnits(line.quantity);
    const cumulative =
      toMinorUnits(line.invoiced_quantity ?? "0") +
      toMinorUnits(values[line.id!]);
    const lineMinor = toMinorUnits(line.total ?? "0") / 100n;
    const rounded =
      quantity > 0n
        ? (lineMinor * cumulative * 2n + quantity) / (quantity * 2n)
        : 0n;
    const invoicedMinor = toMinorUnits(line.invoiced_total ?? "0") / 100n;
    const value = rounded - invoicedMinor;
    const cap = lineMinor - invoicedMinor - 1n;
    return (
      sum +
      (cumulative < quantity
        ? value < cap
          ? value
          : cap > 0n
            ? cap
            : 0n
        : value)
    );
  }, 0n);
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api(`/orders/${order.id}/${action}`, {
        method: "POST",
        body: {
          version: order.version,
          items: selected.map((line) => ({
            line_id: line.id,
            quantity: values[line.id!],
          })),
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
      title={
        action === "ship"
          ? "Kirim barang bertahap"
          : "Terbitkan invoice bertahap"
      }
      description={order.number}
      wide
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <p className="muted">
            Isi jumlah yang diproses sekarang. Isi 0 untuk melewati produk; sisa
            tetap tersedia pada pesanan.
          </p>
          {lines.map((line) => (
            <FormField
              key={line.id}
              label={`${line.sku} · ${(line as OrderLine & { name?: string }).name ?? line.product_name ?? ""} (maks. ${number(action === "ship" ? line.unshipped_quantity : line.invoiceable_quantity)})`}
            >
              <input
                aria-label={`Jumlah ${line.sku}`}
                type="number"
                min="0"
                step="0.001"
                max={
                  action === "ship"
                    ? line.unshipped_quantity
                    : line.invoiceable_quantity
                }
                required
                disabled={busy}
                value={values[line.id!]}
                onChange={(event) => {
                  setValues((current) => ({
                    ...current,
                    [line.id!]: event.target.value,
                  }));
                  setKey(crypto.randomUUID());
                }}
              />
            </FormField>
          ))}
          {action === "invoice" && (
            <div className="order-total">
              <span>Pratinjau invoice</span>
              <strong>
                {money(
                  `${invoiceMinor / 100n}.${(invoiceMinor % 100n).toString().padStart(2, "0")}`,
                )}
              </strong>
            </div>
          )}
          <div className="info-box">
            <p>
              {action === "ship"
                ? "Stok dan harga pokok berkurang hanya untuk jumlah yang dipilih."
                : "Piutang dan pendapatan dicatat untuk jumlah yang dipilih. Pembulatan mengikuti total pesanan agar penagihan terakhir tepat."}
            </p>
          </div>
          <FormError error={error} />
          {error && (
            <button
              type="button"
              className="text-button"
              disabled={busy}
              onClick={() => {
                onClose();
                onReload();
              }}
            >
              Muat ulang pesanan sebelum mencoba kembali
            </button>
          )}
        </div>
        <FormActions
          onClose={onClose}
          busy={busy || !selected.length}
          label={action === "ship" ? "Catat pengiriman" : "Terbitkan invoice"}
        />
      </form>
    </Modal>
  );
}
