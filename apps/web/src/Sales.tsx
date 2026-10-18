import { useState, type FormEvent } from "react";
import {
  ArrowRight,
  Check,
  CreditCard,
  FilePlus2,
  FileText,
  PackageCheck,
  Plus,
  ShoppingBag,
  Trash2,
  Truck,
} from "lucide-react";
import { api, errorMessage } from "./api";
import {
  EmptyState,
  ErrorState,
  FormActions,
  FormError,
  FormField,
  Loading,
  Modal,
  PageHeader,
  Pagination,
  SearchInput,
  Status,
} from "./components";
import { date, lineTotal, money, number, orderTotal } from "./format";
import { usePagination, useResource } from "./hooks";
import type { Customer, Invoice, Order, Product, Role, Session } from "./types";
import InvoiceDetail, { ReasonForm } from "./InvoiceDetail";
import { downloadCsv } from "./export";

export default function Sales({
  role,
  changed,
  session,
}: {
  role: Role;
  changed: () => void;
  session: Session;
}) {
  const [tab, setTab] = useState("orders");
  const [revision, setRevision] = useState(0);
  const orders = useResource<{ items: Order[] }>("/orders", revision);
  const invoices = useResource<{ items: Invoice[] }>("/invoices", revision);
  const [search, setSearch] = useState("");
  const [create, setCreate] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [payment, setPayment] = useState<Invoice | null>(null);
  const [invoiceDetailId, setInvoiceDetailId] = useState<string | null>(null);
  const invoiceDetail = invoices.data?.items.find(
    (invoice) => invoice.id === invoiceDetailId,
  );
  const canSell = ["admin", "sales"].includes(role);
  const canPay = ["admin", "finance"].includes(role);
  const orderItems = (orders.data?.items ?? []).filter((item) =>
    `${item.number} ${item.customer_name}`
      .toLowerCase()
      .includes(search.toLowerCase()),
  );
  const invoiceItems = (invoices.data?.items ?? []).filter((item) =>
    `${item.number} ${item.customer_name}`
      .toLowerCase()
      .includes(search.toLowerCase()),
  );
  const orderPagination = usePagination(orderItems, search);
  const invoicePagination = usePagination(invoiceItems, search);
  function saved() {
    setCreate(false);
    setPayment(null);
    setRevision((value) => value + 1);
    changed();
  }
  return (
    <>
      <PageHeader
        eyebrow="DARI PESANAN HINGGA PEMBAYARAN"
        title="Penjualan"
        description="Setiap transaksi, mengalir dalam satu alur yang jelas."
        actions={
          canSell && (
            <button className="button primary" onClick={() => setCreate(true)}>
              <Plus size={17} /> Buat pesanan
            </button>
          )
        }
      />
      <div className="sales-flow">
        <span>
          <ShoppingBag size={17} /> Buat pesanan
        </span>
        <ArrowRight size={14} />
        <span>
          <Check size={17} /> Konfirmasi
        </span>
        <ArrowRight size={14} />
        <span>
          <Truck size={17} /> Kirim produk
        </span>
        <ArrowRight size={14} />
        <span>
          <FileText size={17} /> Terbitkan invoice
        </span>
        <ArrowRight size={14} />
        <span>
          <CreditCard size={17} /> Terima pembayaran
        </span>
      </div>
      <section className="panel">
        <div className="table-toolbar">
          <div className="tabs" role="tablist" aria-label="Tampilan penjualan">
            <button
              role="tab"
              aria-selected={tab === "orders"}
              className={tab === "orders" ? "active" : ""}
              onClick={() => {
                setTab("orders");
                setSearch("");
              }}
            >
              Pesanan{" "}
              <span className="count-pill">
                {orders.data?.items.length ?? 0}
              </span>
            </button>
            <button
              role="tab"
              aria-selected={tab === "invoices"}
              className={tab === "invoices" ? "active" : ""}
              onClick={() => {
                setTab("invoices");
                setSearch("");
              }}
            >
              Invoice{" "}
              <span className="count-pill">
                {invoices.data?.items.length ?? 0}
              </span>
            </button>
          </div>
          <button
            className="text-button export-button"
            onClick={() =>
              downloadCsv(
                tab === "orders" ? "haruflow-pesanan" : "haruflow-invoice",
                [
                  "Nomor",
                  "Pelanggan",
                  "Tanggal",
                  "Status",
                  "Total",
                  "Terbayar",
                ],
                (tab === "orders" ? orderItems : invoiceItems).map((o) => [
                  o.number,
                  o.customer_name,
                  o.created_at,
                  o.status,
                  o.total,
                  o.paid,
                ]),
              )
            }
          >
            Ekspor CSV
          </button>
          <SearchInput
            value={search}
            onChange={setSearch}
            placeholder="Cari nomor atau pelanggan…"
          />
        </div>
        {tab === "orders" ? (
          orders.loading ? (
            <Loading />
          ) : orders.error ? (
            <ErrorState message={orders.error} onRetry={orders.reload} />
          ) : orderItems.length ? (
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Nomor pesanan</th>
                    <th>Pelanggan</th>
                    <th>Tanggal</th>
                    <th>Status</th>
                    <th className="align-right">Total</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {orderPagination.items.map((order) => (
                    <tr key={order.id}>
                      <td>
                        <button
                          className="table-link"
                          onClick={() => setSelected(order.id)}
                        >
                          {order.number}
                        </button>
                      </td>
                      <td>
                        <span className="customer-cell">
                          <span className="initial-avatar">
                            {order.customer_name.slice(0, 1).toUpperCase()}
                          </span>
                          {order.customer_name}
                        </span>
                      </td>
                      <td className="muted">{date(order.created_at)}</td>
                      <td>
                        <Status status={order.status} />
                      </td>
                      <td className="align-right font-medium">
                        {money(order.total)}
                      </td>
                      <td>
                        <button
                          className="text-button"
                          onClick={() => setSelected(order.id)}
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
                search
                  ? "Pesanan tidak ditemukan"
                  : "Pesanan pertama, langkah berikutnya."
              }
              description={
                search
                  ? "Coba nomor atau nama pelanggan yang berbeda."
                  : "Pilih pelanggan dan produk untuk memulai alur penjualan Anda."
              }
              action={
                canSell &&
                !search && (
                  <button
                    className="button secondary"
                    onClick={() => setCreate(true)}
                  >
                    <Plus size={16} /> Buat pesanan
                  </button>
                )
              }
            />
          )
        ) : invoices.loading ? (
          <Loading />
        ) : invoices.error ? (
          <ErrorState message={invoices.error} onRetry={invoices.reload} />
        ) : invoiceItems.length ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Nomor invoice</th>
                  <th>Pelanggan</th>
                  <th>Tanggal</th>
                  <th>Status</th>
                  <th className="align-right">Total</th>
                  <th className="align-right">Terbayar</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {invoicePagination.items.map((invoice) => (
                  <tr key={invoice.id}>
                    <td>
                      <button
                        className="table-link"
                        onClick={() => setInvoiceDetailId(invoice.id)}
                      >
                        {invoice.number}
                      </button>
                    </td>
                    <td>{invoice.customer_name}</td>
                    <td className="muted">{date(invoice.created_at)}</td>
                    <td>
                      <Status status={invoice.status} />
                    </td>
                    <td className="align-right">{money(invoice.total)}</td>
                    <td className="align-right font-medium">
                      {money(invoice.paid)}
                    </td>
                    <td>
                      {canPay && !["paid", "void"].includes(invoice.status) && (
                        <button
                          className="text-button"
                          onClick={() => setPayment(invoice)}
                        >
                          <CreditCard size={15} /> Bayar
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
            title="Invoice akan muncul di sini"
            description="Terbitkan invoice dari pesanan yang sudah dikirim untuk mencatat piutang pelanggan."
          />
        )}
        <div className="table-footer">
          <Pagination
            {...(tab === "orders" ? orderPagination : invoicePagination)}
          />
        </div>
      </section>
      {create && <OrderForm onClose={() => setCreate(false)} onSaved={saved} />}
      {selected && (
        <OrderDetail
          id={selected}
          role={role}
          onClose={() => setSelected(null)}
          onChanged={saved}
        />
      )}
      {invoiceDetail && (
        <InvoiceDetail
          invoice={invoiceDetail}
          session={session}
          onClose={() => setInvoiceDetailId(null)}
          onChanged={saved}
          onPay={() => {
            setPayment(invoiceDetail);
            setInvoiceDetailId(null);
          }}
        />
      )}
      {payment && (
        <PaymentForm
          invoice={payment}
          onClose={() => setPayment(null)}
          onSaved={saved}
        />
      )}
    </>
  );
}

interface DraftLine {
  key: string;
  product_id: string;
  quantity: string;
  unit_price: string;
}
function blankLine(): DraftLine {
  return {
    key: crypto.randomUUID(),
    product_id: "",
    quantity: "1",
    unit_price: "",
  };
}

function OrderForm({
  onClose,
  onSaved,
}: {
  onClose: () => void;
  onSaved: () => void;
}) {
  const customers = useResource<{ items: Customer[] }>("/customers");
  const products = useResource<{ items: Product[] }>("/products");
  const [lines, setLines] = useState<DraftLine[]>([blankLine()]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [key] = useState(() => crypto.randomUUID());
  function updateLine(index: number, fields: Partial<DraftLine>) {
    setLines((current) =>
      current.map((line, i) => (i === index ? { ...line, ...fields } : line)),
    );
  }
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    const customer_id = new FormData(event.currentTarget).get("customer_id");
    try {
      await api("/orders", {
        method: "POST",
        body: {
          customer_id,
          items: lines.map(({ product_id, quantity, unit_price }) => ({
            product_id,
            quantity,
            unit_price,
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
  const prerequisiteError = customers.error || products.error;
  return (
    <Modal
      title="Buat pesanan"
      description="Mulai dengan pelanggan, lalu tambahkan produk yang dipesan."
      onClose={() => {
        if (!busy) onClose();
      }}
      wide
    >
      {customers.loading || products.loading ? (
        <Loading />
      ) : prerequisiteError ? (
        <ErrorState
          message={prerequisiteError}
          onRetry={() => {
            customers.reload();
            products.reload();
          }}
        />
      ) : !customers.data?.items.length || !products.data?.items.length ? (
        <EmptyState
          title="Lengkapi data awal terlebih dahulu"
          description="Tambahkan minimal satu pelanggan dan satu produk sebelum membuat pesanan."
          action={
            <button className="button secondary" onClick={onClose}>
              Kembali ke ruang kerja
            </button>
          }
        />
      ) : (
        <form onSubmit={submit}>
          <div className="form-body">
            <div className="form-grid">
              <FormField label="Pelanggan *">
                <select name="customer_id" required defaultValue="" autoFocus>
                  <option value="" disabled>
                    Pilih pelanggan
                  </option>
                  {customers.data.items.map((customer) => (
                    <option key={customer.id} value={customer.id}>
                      {customer.name}
                    </option>
                  ))}
                </select>
              </FormField>
              <div className="order-note">
                <FilePlus2 size={20} />
                <div>
                  <strong>Pesanan disimpan sebagai draf</strong>
                  <p>Konfirmasi setelah detailnya sudah benar.</p>
                </div>
              </div>
            </div>
            <div className="order-lines-heading">
              <strong>Produk pesanan</strong>
              <span>{lines.length} baris produk</span>
            </div>
            <div className="order-line-labels">
              <span>Produk</span>
              <span>Jumlah</span>
              <span>Harga satuan (Rp)</span>
              <span>Subtotal</span>
              <span />
            </div>
            {lines.map((line, index) => (
              <div className="order-line" key={line.key}>
                <select
                  required
                  aria-label={`Produk baris ${index + 1}`}
                  value={line.product_id}
                  onChange={(event) => {
                    const product = products.data?.items.find(
                      (item) => item.id === event.target.value,
                    );
                    updateLine(index, {
                      product_id: event.target.value,
                      unit_price: product?.price ?? "",
                    });
                  }}
                >
                  <option value="" disabled>
                    Pilih produk
                  </option>
                  {products.data!.items.map((product) => (
                    <option
                      key={product.id}
                      value={product.id}
                      disabled={lines.some(
                        (other, i) =>
                          i !== index && other.product_id === product.id,
                      )}
                    >
                      {product.sku} · {product.name}
                    </option>
                  ))}
                </select>
                <input
                  aria-label={`Jumlah baris ${index + 1}`}
                  type="number"
                  min="0.001"
                  step="0.001"
                  required
                  value={line.quantity}
                  onChange={(event) =>
                    updateLine(index, { quantity: event.target.value })
                  }
                />
                <input
                  aria-label={`Harga satuan baris ${index + 1}`}
                  type="number"
                  min="0.01"
                  step="0.01"
                  required
                  value={line.unit_price}
                  onChange={(event) =>
                    updateLine(index, { unit_price: event.target.value })
                  }
                />
                <span className="line-subtotal">
                  {money(lineTotal(line.quantity, line.unit_price))}
                </span>
                <button
                  className="icon-button danger-text"
                  type="button"
                  disabled={lines.length === 1}
                  aria-label={`Hapus produk baris ${index + 1}`}
                  onClick={() =>
                    setLines((current) => current.filter((_, i) => i !== index))
                  }
                >
                  <Trash2 size={17} />
                </button>
              </div>
            ))}
            <button
              className="text-button add-line"
              type="button"
              disabled={
                lines.length >= products.data.items.length ||
                lines.length >= 100
              }
              onClick={() => setLines((current) => [...current, blankLine()])}
            >
              <Plus size={16} /> Tambah baris produk
            </button>
            <div className="order-total">
              <span>Total pesanan</span>
              <strong>{money(orderTotal(lines))}</strong>
              <small>Nilai akhir dihitung saat pesanan disimpan.</small>
            </div>
            <FormError error={error} />
          </div>
          <FormActions onClose={onClose} busy={busy} label="Simpan pesanan" />
        </form>
      )}
    </Modal>
  );
}

function OrderDetail({
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
  const { data, loading, error, reload } = useResource<Order>(`/orders/${id}`);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState("");
  const [reversal, setReversal] = useState(false);
  const [keys] = useState(() => ({
    confirm: crypto.randomUUID(),
    ship: crypto.randomUUID(),
    invoice: crypto.randomUUID(),
  }));
  const action =
    data?.status === "draft" && ["admin", "sales"].includes(role)
      ? {
          path: "confirm" as const,
          label: "Konfirmasi pesanan",
          Icon: Check,
          note: "Konfirmasi menyetujui detail pesanan untuk diproses oleh gudang.",
        }
      : data?.status === "confirmed" && ["admin", "warehouse"].includes(role)
        ? {
            path: "ship" as const,
            label: "Kirim pesanan",
            Icon: Truck,
            note: "Pengiriman mengurangi stok dan mencatat harga pokok secara otomatis.",
          }
        : data?.status === "shipped" && ["admin", "finance"].includes(role)
          ? {
              path: "invoice" as const,
              label: "Terbitkan invoice",
              Icon: FileText,
              note: "Invoice mencatat pendapatan dan piutang pelanggan dalam jurnal.",
            }
          : null;
  async function mutate() {
    if (!action) return;
    setBusy(true);
    setActionError("");
    try {
      await api(`/orders/${id}/${action.path}`, {
        method: "POST",
        body: { idempotency_key: keys[action.path] },
      });
      reload();
      onChanged();
    } catch (error) {
      setActionError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={data?.number ?? "Detail pesanan"}
      description="Semua detail dan langkah selanjutnya dalam satu tempat."
      onClose={() => {
        if (!busy) onClose();
      }}
      wide
    >
      {loading ? (
        <Loading />
      ) : error || !data ? (
        <ErrorState message={error} onRetry={reload} />
      ) : (
        <>
          <div className="form-body">
            <div className="order-detail-meta">
              <div>
                <small>Pelanggan</small>
                <strong>{data.customer_name}</strong>
              </div>
              <div>
                <small>Tanggal pesanan</small>
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
                    <th className="align-right">Jumlah</th>
                    <th className="align-right">Harga satuan</th>
                    <th className="align-right">Subtotal</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items?.map((item, index) => (
                    <tr key={`${item.product_id}-${index}`}>
                      <td>
                        <strong>
                          {item.product_name ??
                            (item as typeof item & { name?: string }).name ??
                            item.sku ??
                            item.product_id}
                        </strong>
                        <small className="block muted">{item.sku}</small>
                      </td>
                      <td className="align-right">{number(item.quantity)}</td>
                      <td className="align-right">{money(item.unit_price)}</td>
                      <td className="align-right font-medium">
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
            <div className="order-total">
              <span>Total pesanan</span>
              <strong>{money(data.total)}</strong>
            </div>
            {action ? (
              <div className="info-box">
                <action.Icon size={18} />
                <p>{action.note} Perubahan ini akan langsung tersimpan.</p>
              </div>
            ) : (
              <div className="info-box">
                <PackageCheck size={18} />
                <p>
                  {data.status === "invoiced"
                    ? "Invoice telah diterbitkan. Catat pembayaran pelanggan pada tab Invoice."
                    : "Pesanan siap dilanjutkan oleh pengguna dengan hak akses yang sesuai."}
                </p>
              </div>
            )}
            <FormError error={actionError} />
          </div>
          <div className="form-actions">
            {((["draft", "confirmed"].includes(data.status) &&
              ["admin", "sales"].includes(role)) ||
              (data.status === "shipped" &&
                ["admin", "warehouse"].includes(role))) && (
              <button
                className="text-button danger-text"
                disabled={busy}
                onClick={() => setReversal(true)}
              >
                {data.status === "shipped"
                  ? "Batalkan pengiriman"
                  : "Batalkan pesanan"}
              </button>
            )}
            <button
              className="button secondary"
              onClick={onClose}
              disabled={busy}
            >
              Tutup
            </button>
            {action && (
              <button
                className="button primary"
                disabled={busy}
                onClick={mutate}
              >
                <action.Icon size={17} />
                {busy ? "Memproses…" : action.label}
              </button>
            )}
          </div>
        </>
      )}
      {reversal && data && (
        <ReasonForm
          path={`/orders/${id}/${data.status === "shipped" ? "reverse-shipment" : "cancel"}`}
          title={
            data.status === "shipped"
              ? "Batalkan pengiriman"
              : "Batalkan pesanan"
          }
          onClose={() => setReversal(false)}
          onSaved={() => {
            setReversal(false);
            onChanged();
          }}
        />
      )}
    </Modal>
  );
}

function PaymentForm({
  invoice,
  onClose,
  onSaved,
}: {
  invoice: Invoice;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [key] = useState(() => crypto.randomUUID());
  const outstanding = (Number(invoice.total) - Number(invoice.paid)).toFixed(2);
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api(`/invoices/${invoice.id}/payments`, {
        method: "POST",
        body: {
          ...Object.fromEntries(new FormData(event.currentTarget)),
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
      title="Catat pembayaran"
      description={`${invoice.number} · ${invoice.customer_name}`}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <div className="payment-outstanding">
            <span>Sisa tagihan</span>
            <strong>{money(outstanding)}</strong>
          </div>
          <FormField label="Jumlah pembayaran (Rp) *">
            <input
              name="amount"
              type="number"
              required
              min="0.01"
              max={outstanding}
              step="0.01"
              defaultValue={outstanding}
              autoFocus
            />
          </FormField>
          <FormField label="Referensi pembayaran">
            <input
              name="reference"
              maxLength={150}
              placeholder="Contoh: Transfer bank 18/10"
            />
          </FormField>
          <div className="info-box">
            <CreditCard size={18} />
            <p>
              Pembayaran akan menambah saldo kas dan mengurangi piutang
              pelanggan.
            </p>
          </div>
          <FormError error={error} />
        </div>
        <FormActions onClose={onClose} busy={busy} label="Simpan pembayaran" />
      </form>
    </Modal>
  );
}
