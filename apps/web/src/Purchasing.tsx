import { useRef, useState, type FormEvent } from "react";
import {
  ArrowRight,
  Check,
  ClipboardList,
  Info,
  Mail,
  MapPin,
  Package,
  Phone,
  Plus,
  RefreshCw,
  Store,
  Trash2,
  XCircle,
} from "lucide-react";
import { api, ApiError, errorMessage } from "./api";
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
import { calendarDate, exactMoney } from "./dashboard-charts";
import { downloadCsv } from "./export";
import { date, dateTime, statusNames } from "./format";
import { usePagination, useResource } from "./hooks";
import {
  exactCost,
  exactDecimal,
  purchaseDraft,
  purchaseLineCents,
  purchasePreview,
  purchaseProducts,
  purchaseSummary,
  requestIdentity,
  type PurchaseDraft,
  type PurchaseOrder,
  type RequestIdentity,
  type Supplier,
} from "./purchasing-model";
import type { Product, Role } from "./types";
import "./purchasing.css";

export default function Purchasing({
  role,
  changed,
}: {
  role: Role;
  changed: () => void;
}) {
  const [tab, setTab] = useState<"orders" | "suppliers">("orders");
  const [revision, setRevision] = useState(0);
  const orders = useResource<{ items: PurchaseOrder[] }>(
    "/purchase-orders",
    revision,
  );
  const suppliers = useResource<{ items: Supplier[] }>("/suppliers", revision);
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("");
  const [selectedOrder, setSelectedOrder] = useState<string | null>(null);
  const [editingOrder, setEditingOrder] = useState<PurchaseOrder | null>(null);
  const [createOrder, setCreateOrder] = useState(false);
  const [selectedSupplier, setSelectedSupplier] = useState<Supplier | null>(
    null,
  );
  const [editingSupplier, setEditingSupplier] = useState<Supplier | null>(null);
  const [createSupplier, setCreateSupplier] = useState(false);
  const canManage = ["admin", "warehouse"].includes(role);
  const orderItems = (orders.data?.items ?? []).filter(
    (item) =>
      (!status || item.status === status) &&
      `${item.number} ${item.supplier_code} ${item.supplier_name}`
        .toLowerCase()
        .includes(search.toLowerCase()),
  );
  const supplierItems = (suppliers.data?.items ?? []).filter(
    (item) =>
      (!status || (status === "active") === item.active) &&
      `${item.code} ${item.name} ${item.email ?? ""} ${item.phone ?? ""}`
        .toLowerCase()
        .includes(search.toLowerCase()),
  );
  const orderPagination = usePagination(orderItems, `${search}|${status}`);
  const supplierPagination = usePagination(
    supplierItems,
    `${search}|${status}`,
  );
  const summary = purchaseSummary(orders.data?.items ?? []);
  function saved() {
    setRevision((value) => value + 1);
    changed();
  }
  function switchTab(next: "orders" | "suppliers") {
    setTab(next);
    setSearch("");
    setStatus("");
  }
  return (
    <>
      <PageHeader
        eyebrow="RENCANAKAN BELANJA USAHA"
        title="Pembelian"
        description="Kelola supplier dan sepakati barang yang akan dibeli."
        actions={
          canManage && (
            <button
              className="button primary"
              onClick={() =>
                tab === "orders"
                  ? setCreateOrder(true)
                  : setCreateSupplier(true)
              }
            >
              <Plus size={17} />
              {tab === "orders" ? "Buat PO" : "Tambah supplier"}
            </button>
          )
        }
      />
      <div className="purchase-intro">
        <div className="purchase-intro-icon">
          <ClipboardList size={23} />
        </div>
        <div>
          <strong>Pesanan pembelian (PO) adalah rencana belanja.</strong>
          <p>
            Simpan draf, tinjau biaya, lalu konfirmasi kesepakatan dengan
            supplier. PO belum menambah stok, membuat utang, atau mengubah kas.
            Penerimaan barang dan tagihan supplier tersedia pada tahap
            berikutnya.
          </p>
        </div>
      </div>
      {tab === "orders" && orders.data && !orders.error && (
        <div
          className="purchase-summary"
          aria-label="Ringkasan pesanan pembelian"
        >
          {summary.map((item) => (
            <button
              key={item.status}
              className={`purchase-summary-card ${item.status} ${status === item.status ? "selected" : ""}`}
              aria-pressed={status === item.status}
              onClick={() =>
                setStatus(status === item.status ? "" : item.status)
              }
            >
              <span>
                {item.status === "draft"
                  ? "Rencana dalam draf"
                  : item.status === "confirmed"
                    ? "Komitmen dikonfirmasi"
                    : "Pesanan dibatalkan"}
                <b>{item.count} PO</b>
              </span>
              <strong>{exactMoney(item.total)}</strong>
              <small>
                {item.status === "confirmed"
                  ? "Nilai kesepakatan pembelian, belum menjadi utang"
                  : item.status === "draft"
                    ? "Masih dapat diubah sebelum konfirmasi"
                    : "Disimpan untuk riwayat, di luar komitmen aktif"}
              </small>
            </button>
          ))}
        </div>
      )}
      <section className="panel purchase-panel">
        <div className="table-toolbar">
          <div className="tabs" role="tablist" aria-label="Tampilan pembelian">
            <button
              role="tab"
              aria-selected={tab === "orders"}
              className={tab === "orders" ? "active" : ""}
              onClick={() => switchTab("orders")}
            >
              <ClipboardList size={16} />
              Pesanan pembelian
              <span className="count-pill">
                {orders.data?.items.length ?? 0}
              </span>
            </button>
            <button
              role="tab"
              aria-selected={tab === "suppliers"}
              className={tab === "suppliers" ? "active" : ""}
              onClick={() => switchTab("suppliers")}
            >
              <Store size={16} />
              Supplier
              <span className="count-pill">
                {suppliers.data?.items.length ?? 0}
              </span>
            </button>
          </div>
          <button
            className="text-button export-button"
            disabled={
              tab === "orders"
                ? orders.loading || !!orders.error
                : suppliers.loading || !!suppliers.error
            }
            onClick={() =>
              tab === "orders"
                ? downloadCsv(
                    "haruflow-pembelian",
                    [
                      "Nomor",
                      "Kode supplier",
                      "Supplier",
                      "Tanggal dibuat",
                      "Perkiraan tiba",
                      "Status",
                      "Total",
                      "Catatan",
                      "Alasan pembatalan",
                    ],
                    orderItems.map((item) => [
                      item.number,
                      item.supplier_code,
                      item.supplier_name,
                      item.created_at,
                      item.expected_date,
                      statusNames[item.status],
                      item.total,
                      item.notes,
                      item.cancellation_reason,
                    ]),
                  )
                : downloadCsv(
                    "haruflow-supplier",
                    ["Kode", "Nama", "Email", "Telepon", "Alamat", "Status"],
                    supplierItems.map((item) => [
                      item.code,
                      item.name,
                      item.email,
                      item.phone,
                      item.address,
                      item.active ? "Aktif" : "Nonaktif",
                    ]),
                  )
            }
          >
            Ekspor CSV
          </button>
        </div>
        <div className="purchase-filters">
          <SearchInput
            value={search}
            onChange={setSearch}
            placeholder={
              tab === "orders"
                ? "Cari nomor PO atau supplier…"
                : "Cari kode, nama, atau kontak supplier…"
            }
          />
          <label className="purchase-filter">
            <span>Status</span>
            <select
              value={status}
              onChange={(event) => setStatus(event.target.value)}
            >
              <option value="">Semua status</option>
              {tab === "orders" ? (
                <>
                  <option value="draft">Draf</option>
                  <option value="confirmed">Dikonfirmasi</option>
                  <option value="cancelled">Dibatalkan</option>
                </>
              ) : (
                <>
                  <option value="active">Aktif</option>
                  <option value="inactive">Nonaktif</option>
                </>
              )}
            </select>
          </label>
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
                    <th>Nomor PO</th>
                    <th>Supplier</th>
                    <th>Dibuat</th>
                    <th>Perkiraan tiba</th>
                    <th>Status</th>
                    <th className="align-right">Nilai pembelian</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {orderPagination.items.map((item) => (
                    <tr key={item.id}>
                      <td>
                        <button
                          className="table-link"
                          onClick={() => setSelectedOrder(item.id)}
                        >
                          {item.number}
                        </button>
                      </td>
                      <td>
                        <span className="purchase-cell">
                          <strong>{item.supplier_name}</strong>
                          <small>{item.supplier_code}</small>
                        </span>
                      </td>
                      <td className="muted">{date(item.created_at)}</td>
                      <td className="muted">
                        {item.expected_date
                          ? calendarDate(item.expected_date)
                          : "Belum ditentukan"}
                      </td>
                      <td>
                        <Status status={item.status} />
                      </td>
                      <td className="align-right font-medium">
                        {exactMoney(item.total)}
                      </td>
                      <td>
                        <button
                          className="text-button"
                          onClick={() => setSelectedOrder(item.id)}
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
                search || status
                  ? "Pesanan pembelian tidak ditemukan"
                  : "Mulai rencanakan pembelian pertama."
              }
              description={
                search || status
                  ? "Ubah kata kunci atau filter status untuk melihat pesanan lain."
                  : "Tambahkan supplier, pilih barang, dan isi biaya beli sesuai kesepakatan."
              }
              action={
                canManage &&
                !search &&
                !status && (
                  <button
                    className="button secondary"
                    onClick={() => setCreateOrder(true)}
                  >
                    <Plus size={16} />
                    Buat PO
                  </button>
                )
              }
            />
          )
        ) : suppliers.loading ? (
          <Loading />
        ) : suppliers.error ? (
          <ErrorState message={suppliers.error} onRetry={suppliers.reload} />
        ) : supplierItems.length ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Supplier</th>
                  <th>Email</th>
                  <th>Telepon</th>
                  <th>Status</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {supplierPagination.items.map((supplier) => (
                  <tr key={supplier.id}>
                    <td>
                      <button
                        className="table-link purchase-cell"
                        onClick={() => setSelectedSupplier(supplier)}
                      >
                        <strong>{supplier.name}</strong>
                        <small>{supplier.code}</small>
                      </button>
                    </td>
                    <td className="muted">{supplier.email || "—"}</td>
                    <td className="muted">{supplier.phone || "—"}</td>
                    <td>
                      <SupplierStatus active={supplier.active} />
                    </td>
                    <td>
                      <button
                        className="text-button"
                        onClick={() => setSelectedSupplier(supplier)}
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
              search || status
                ? "Supplier tidak ditemukan"
                : "Kenali pemasok usaha Anda."
            }
            description={
              search || status
                ? "Coba kata kunci atau status yang berbeda."
                : "Simpan kode, nama, dan kontak supplier untuk memulai pesanan pembelian."
            }
            action={
              canManage &&
              !search &&
              !status && (
                <button
                  className="button secondary"
                  onClick={() => setCreateSupplier(true)}
                >
                  <Plus size={16} />
                  Tambah supplier
                </button>
              )
            }
          />
        )}
        <div className="table-footer">
          <Pagination
            {...(tab === "orders" ? orderPagination : supplierPagination)}
          />
        </div>
      </section>
      {tab === "orders" && (
        <p className="purchase-history-note">
          Ringkasan, pencarian, dan ekspor mencakup hingga 500 PO terbaru. Nilai
          menggunakan biaya beli tersimpan dalam IDR.
        </p>
      )}
      {(createSupplier || editingSupplier) && (
        <SupplierForm
          supplier={editingSupplier ?? undefined}
          onClose={() => {
            setCreateSupplier(false);
            setEditingSupplier(null);
          }}
          onSaved={() => {
            setCreateSupplier(false);
            setEditingSupplier(null);
            saved();
          }}
        />
      )}
      {selectedSupplier && (
        <Modal
          title={selectedSupplier.name}
          description={`Supplier · ${selectedSupplier.code}`}
          onClose={() => setSelectedSupplier(null)}
        >
          <div className="detail-list">
            <div>
              <Store size={18} />
              <span>
                <small>Status</small>
                <SupplierStatus active={selectedSupplier.active} />
              </span>
            </div>
            <div>
              <Mail size={18} />
              <span>
                <small>Email</small>
                {selectedSupplier.email || "Belum ditambahkan"}
              </span>
            </div>
            <div>
              <Phone size={18} />
              <span>
                <small>Telepon</small>
                {selectedSupplier.phone || "Belum ditambahkan"}
              </span>
            </div>
            <div>
              <MapPin size={18} />
              <span>
                <small>Alamat</small>
                <span className="purchase-prewrap">
                  {selectedSupplier.address || "Belum ditambahkan"}
                </span>
              </span>
            </div>
          </div>
          <div className="form-actions">
            <button
              className="button secondary"
              onClick={() => setSelectedSupplier(null)}
            >
              Tutup
            </button>
            {canManage && (
              <button
                className="button primary"
                onClick={() => {
                  setEditingSupplier(selectedSupplier);
                  setSelectedSupplier(null);
                }}
              >
                Edit supplier
              </button>
            )}
          </div>
        </Modal>
      )}
      {(createOrder || editingOrder) && (
        <PurchaseOrderForm
          order={editingOrder ?? undefined}
          onClose={() => {
            if (editingOrder) setSelectedOrder(editingOrder.id);
            setCreateOrder(false);
            setEditingOrder(null);
          }}
          onSaved={(id) => {
            setCreateOrder(false);
            setEditingOrder(null);
            if (id) setSelectedOrder(id);
            saved();
          }}
        />
      )}
      {selectedOrder && (
        <PurchaseOrderDetail
          id={selectedOrder}
          canManage={canManage}
          revision={revision}
          onClose={() => setSelectedOrder(null)}
          onSaved={saved}
          onEdit={(order) => {
            setSelectedOrder(null);
            setEditingOrder(order);
          }}
        />
      )}
    </>
  );
}

function SupplierStatus({ active }: { active: boolean }) {
  return (
    <span
      className={`purchase-supplier-status ${active ? "active" : "inactive"}`}
    >
      <span />
      {active ? "Aktif" : "Nonaktif"}
    </span>
  );
}

function SupplierForm({
  supplier,
  onClose,
  onSaved,
}: {
  supplier?: Supplier;
  onClose: () => void;
  onSaved: () => void;
}) {
  const initial = (item?: Supplier) => ({
    code: item?.code ?? "",
    name: item?.name ?? "",
    email: item?.email ?? "",
    phone: item?.phone ?? "",
    address: item?.address ?? "",
    active: item?.active ?? true,
  });
  const [draft, setDraft] = useState(() => initial(supplier));
  const [version, setVersion] = useState(supplier?.version);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  const [error, setError] = useState("");
  const [conflict, setConflict] = useState(false);
  const update = (name: string, value: string | boolean) =>
    setDraft((old) => ({ ...old, [name]: value }));
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setError("");
    setConflict(false);
    const { active, ...contacts } = draft;
    const payload = supplier ? { ...contacts, active, version } : contacts;
    identity.current = requestIdentity(identity.current, payload);
    try {
      await api(supplier ? `/suppliers/${supplier.id}` : "/suppliers", {
        method: supplier ? "PUT" : "POST",
        body: { ...payload, idempotency_key: identity.current.key },
      });
      onSaved();
    } catch (error) {
      setError(errorMessage(error));
      setConflict(
        error instanceof ApiError && error.status === 409 && !!supplier,
      );
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  async function reloadLatest() {
    if (!supplier || busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    try {
      const { items } = await api<{ items: Supplier[] }>("/suppliers");
      const latest = items.find((item) => item.id === supplier.id);
      if (!latest)
        throw new Error(
          "Supplier sudah tidak ditemukan. Tutup dialog dan muat ulang data.",
        );
      setDraft(initial(latest));
      setVersion(latest.version);
      setConflict(false);
      setError(
        "Versi terbaru sudah dimuat. Isian diperbarui dari server; tinjau kembali sebelum menyimpan.",
      );
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  return (
    <Modal
      title={supplier ? "Edit supplier" : "Tambah supplier"}
      description="Kontak yang rapi memudahkan setiap pembelian."
      onClose={() => {
        if (!busyRef.current) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <fieldset disabled={busy} className="purchase-fieldset">
            <div className="form-grid">
              <FormField
                label="Kode supplier *"
                hint="Kode unik untuk mengenali pemasok."
              >
                <input
                  required
                  maxLength={64}
                  value={draft.code}
                  onChange={(event) => update("code", event.target.value)}
                  placeholder="SUP-KOPI"
                  autoFocus
                />
              </FormField>
              <FormField label="Nama supplier *">
                <input
                  required
                  maxLength={200}
                  value={draft.name}
                  onChange={(event) => update("name", event.target.value)}
                  placeholder="Nama usaha atau pemasok"
                />
              </FormField>
            </div>
            <div className="form-grid">
              <FormField label="Email">
                <input
                  type="email"
                  maxLength={254}
                  value={draft.email}
                  onChange={(event) => update("email", event.target.value)}
                  placeholder="supplier@email.com"
                />
              </FormField>
              <FormField label="Telepon">
                <input
                  type="tel"
                  maxLength={64}
                  value={draft.phone}
                  onChange={(event) => update("phone", event.target.value)}
                  placeholder="08xxxxxxxxxx"
                />
              </FormField>
            </div>
            <FormField label="Alamat">
              <textarea
                rows={3}
                maxLength={1000}
                value={draft.address}
                onChange={(event) => update("address", event.target.value)}
                placeholder="Alamat lengkap supplier"
              />
            </FormField>
            {supplier && (
              <label className="purchase-checkbox">
                <input
                  type="checkbox"
                  checked={draft.active}
                  onChange={(event) => update("active", event.target.checked)}
                />
                <span>
                  <strong>Supplier aktif</strong>
                  <small>
                    Supplier nonaktif tetap ada dalam riwayat dan tidak dapat
                    dipilih untuk PO baru.
                  </small>
                </span>
              </label>
            )}
          </fieldset>
          <FormError error={error} />
          {conflict && (
            <button
              className="text-button"
              type="button"
              disabled={busy}
              onClick={reloadLatest}
            >
              <RefreshCw size={15} />
              Muat versi terbaru (ganti isian)
            </button>
          )}
        </div>
        <FormActions onClose={onClose} busy={busy} label="Simpan supplier" />
      </form>
    </Modal>
  );
}

function PurchaseOrderForm({
  order,
  onClose,
  onSaved,
}: {
  order?: PurchaseOrder;
  onClose: () => void;
  onSaved: (id?: string) => void;
}) {
  const suppliers = useResource<{ items: Supplier[] }>("/suppliers");
  const products = useResource<{ items: Product[] }>("/products");
  const [draft, setDraft] = useState<PurchaseDraft>(() => purchaseDraft(order));
  const [version, setVersion] = useState(order?.version);
  const [readOnly, setReadOnly] = useState(false);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  const [error, setError] = useState("");
  const [conflict, setConflict] = useState(false);
  const stock = purchaseProducts(products.data?.items ?? []);
  const selectedSupplier = suppliers.data?.items.find(
    (item) => item.id === draft.supplier_id,
  );
  const preview = purchasePreview(draft);
  const update = (
    index: number,
    name: "product_id" | "quantity" | "unit_cost",
    value: string,
  ) =>
    setDraft((old) => ({
      ...old,
      items: old.items.map((item, i) =>
        i === index ? { ...item, [name]: value } : item,
      ),
    }));
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busyRef.current || readOnly) return;
    if (!preview.valid) {
      setError(preview.issue);
      return;
    }
    if (!selectedSupplier?.active) {
      setError("Pilih supplier aktif sebelum menyimpan pesanan.");
      return;
    }
    if (
      draft.items.some(
        (item) => !stock.some((product) => product.id === item.product_id),
      )
    ) {
      setError("Hanya barang stok yang dapat dipilih untuk pesanan pembelian.");
      return;
    }
    busyRef.current = true;
    setBusy(true);
    setError("");
    setConflict(false);
    const payload = {
      ...draft,
      expected_date: draft.expected_date || null,
      notes: draft.notes || null,
      ...(order ? { version } : {}),
    };
    identity.current = requestIdentity(identity.current, payload);
    try {
      const result = await api<{ id: string }>(
        order ? `/purchase-orders/${order.id}` : "/purchase-orders",
        {
          method: order ? "PUT" : "POST",
          body: { ...payload, idempotency_key: identity.current.key },
        },
      );
      onSaved(result.id ?? order?.id);
    } catch (error) {
      setError(errorMessage(error));
      setConflict(error instanceof ApiError && error.status === 409 && !!order);
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  async function reloadLatest() {
    if (!order || busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    try {
      const latest = await api<PurchaseOrder>(`/purchase-orders/${order.id}`);
      setDraft(purchaseDraft(latest));
      setVersion(latest.version);
      setReadOnly(latest.status !== "draft");
      setConflict(false);
      setError(
        latest.status === "draft"
          ? "Versi terbaru sudah dimuat. Isian diperbarui dari server; tinjau kembali sebelum menyimpan."
          : `Pesanan sudah ${statusNames[latest.status].toLowerCase()} dan tidak dapat diedit. Tutup dialog untuk melihat detail terbaru.`,
      );
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  const resourceError = suppliers.error || products.error;
  return (
    <Modal
      wide
      title={order ? `Edit ${order.number}` : "Buat pesanan pembelian"}
      description="Isi biaya beli sesuai kesepakatan supplier. Harga jual katalog tidak digunakan."
      onClose={() => {
        if (!busyRef.current) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          {suppliers.loading || products.loading ? (
            <Loading label="Memuat supplier dan barang…" />
          ) : resourceError ? (
            <ErrorState
              message={resourceError}
              onRetry={() => {
                suppliers.reload();
                products.reload();
              }}
            />
          ) : (
            <fieldset disabled={busy || readOnly} className="purchase-fieldset">
              <div className="form-grid">
                <FormField label="Supplier *">
                  <select
                    required
                    value={draft.supplier_id}
                    onChange={(event) =>
                      setDraft((old) => ({
                        ...old,
                        supplier_id: event.target.value,
                      }))
                    }
                  >
                    <option value="">Pilih supplier aktif</option>
                    {(suppliers.data?.items ?? [])
                      .filter(
                        (item) => item.active || item.id === draft.supplier_id,
                      )
                      .map((item) => (
                        <option
                          key={item.id}
                          value={item.id}
                          disabled={!item.active}
                        >
                          {item.code} · {item.name}
                          {item.active ? "" : " (nonaktif)"}
                        </option>
                      ))}
                  </select>
                </FormField>
                <FormField
                  label="Perkiraan barang tiba"
                  hint="Opsional; tidak mengubah tanggal pencatatan."
                >
                  <input
                    type="date"
                    value={draft.expected_date}
                    min="0001-01-01"
                    max="9999-12-31"
                    onChange={(event) =>
                      setDraft((old) => ({
                        ...old,
                        expected_date: event.target.value,
                      }))
                    }
                  />
                </FormField>
              </div>
              {!suppliers.data?.items.some((item) => item.active) && (
                <div className="info-box">
                  <Info size={18} />
                  <p>
                    Tambahkan supplier aktif pada tab Supplier terlebih dahulu.
                  </p>
                </div>
              )}
              {selectedSupplier && !selectedSupplier.active && (
                <FormError error="Supplier ini nonaktif. Pilih supplier aktif untuk menyimpan atau mengonfirmasi PO." />
              )}
              <div className="purchase-line-heading">
                <div>
                  <strong>Barang yang akan dibeli</strong>
                  <p>
                    Jumlah maksimal 3 desimal · biaya satuan maksimal 4 desimal.
                  </p>
                </div>
                <span className="count-pill">
                  {draft.items.length}/100 baris
                </span>
              </div>
              {!stock.length && (
                <div className="info-box">
                  <Package size={18} />
                  <p>
                    Tambahkan produk dengan jenis Barang pada Inventori terlebih
                    dahulu. Produk jasa tidak digunakan pada PO.
                  </p>
                </div>
              )}
              <div className="purchase-edit-lines">
                {draft.items.map((item, index) => {
                  const value = purchaseLineCents(
                    item.quantity,
                    item.unit_cost,
                  );
                  const selectedProduct = stock.find(
                    (product) => product.id === item.product_id,
                  );
                  return (
                    <div className="purchase-edit-line" key={index}>
                      <FormField
                        label={`Barang ${index + 1} *`}
                        className="purchase-product-field"
                      >
                        <select
                          required
                          value={item.product_id}
                          onChange={(event) =>
                            update(index, "product_id", event.target.value)
                          }
                        >
                          <option value="">Pilih barang</option>
                          {stock.map((product) => (
                            <option
                              key={product.id}
                              value={product.id}
                              disabled={draft.items.some(
                                (line, i) =>
                                  i !== index && line.product_id === product.id,
                              )}
                            >
                              {product.sku} · {product.name}
                            </option>
                          ))}
                        </select>
                      </FormField>
                      <FormField
                        label={
                          selectedProduct
                            ? `Jumlah (${selectedProduct.unit}) *`
                            : "Jumlah *"
                        }
                      >
                        <input
                          aria-label={`Jumlah barang ${index + 1}`}
                          required
                          inputMode="decimal"
                          value={item.quantity}
                          onChange={(event) =>
                            update(index, "quantity", event.target.value)
                          }
                          placeholder="1.000"
                        />
                      </FormField>
                      <FormField label="Biaya beli / satuan *">
                        <input
                          aria-label={`Biaya beli barang ${index + 1}`}
                          required
                          inputMode="decimal"
                          value={item.unit_cost}
                          onChange={(event) =>
                            update(index, "unit_cost", event.target.value)
                          }
                          placeholder="Contoh: 50000.1250"
                        />
                      </FormField>
                      <div className="purchase-line-value">
                        <small>Total baris</small>
                        <strong>
                          {value === null ? "—" : exactMoney(value)}
                        </strong>
                      </div>
                      <button
                        type="button"
                        className="icon-button danger-text"
                        aria-label={`Hapus barang ${index + 1}`}
                        disabled={draft.items.length === 1}
                        onClick={() =>
                          setDraft((old) => ({
                            ...old,
                            items: old.items.filter((_, i) => i !== index),
                          }))
                        }
                      >
                        <Trash2 size={17} />
                      </button>
                    </div>
                  );
                })}
              </div>
              <button
                type="button"
                className="text-button"
                disabled={
                  draft.items.length >= 100 ||
                  draft.items.length >= stock.length
                }
                onClick={() =>
                  setDraft((old) => ({
                    ...old,
                    items: [
                      ...old.items,
                      { product_id: "", quantity: "1", unit_cost: "" },
                    ],
                  }))
                }
              >
                <Plus size={16} />
                Tambah barang
              </button>
              <div className="purchase-draft-total">
                <span>
                  Total rencana pembelian
                  <small>
                    Setiap baris dibulatkan ke 2 desimal sebelum dijumlahkan.
                  </small>
                </span>
                <strong>{exactMoney(preview.total)}</strong>
              </div>
              <FormField
                label="Catatan"
                hint="Opsional, maksimal 2.000 karakter."
              >
                <textarea
                  maxLength={2000}
                  rows={3}
                  value={draft.notes}
                  onChange={(event) =>
                    setDraft((old) => ({ ...old, notes: event.target.value }))
                  }
                  placeholder="Kesepakatan pengiriman atau catatan untuk supplier"
                />
              </FormField>
              <div className="info-box">
                <Info size={18} />
                <p>
                  Simpan sebagai draf untuk ditinjau. Stok, utang, dan jurnal
                  belum berubah saat PO dibuat maupun dikonfirmasi.
                </p>
              </div>
            </fieldset>
          )}
          <FormError error={error} />
          {conflict && (
            <button
              className="text-button"
              type="button"
              disabled={busy}
              onClick={reloadLatest}
            >
              <RefreshCw size={15} />
              Muat versi terbaru (ganti isian)
            </button>
          )}
        </div>
        {readOnly ? (
          <div className="form-actions">
            <button
              type="button"
              className="button secondary"
              onClick={onClose}
            >
              Tutup dan lihat detail
            </button>
          </div>
        ) : (
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
              type="submit"
              className="button primary"
              disabled={
                busy ||
                suppliers.loading ||
                products.loading ||
                !!resourceError ||
                !stock.length ||
                !suppliers.data?.items.some((item) => item.active)
              }
            >
              {busy ? "Menyimpan…" : "Simpan draf PO"}
            </button>
          </div>
        )}
      </form>
    </Modal>
  );
}

function PurchaseOrderDetail({
  id,
  canManage,
  revision,
  onClose,
  onEdit,
  onSaved,
}: {
  id: string;
  canManage: boolean;
  revision: number;
  onClose: () => void;
  onEdit: (order: PurchaseOrder) => void;
  onSaved: () => void;
}) {
  const resource = useResource<PurchaseOrder>(
    `/purchase-orders/${id}`,
    revision,
  );
  const [action, setAction] = useState<"confirm" | "cancel" | null>(null);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  const [error, setError] = useState("");
  const [conflict, setConflict] = useState(false);
  const order = resource.data;
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busyRef.current || !order || !action) return;
    busyRef.current = true;
    setBusy(true);
    setError("");
    setConflict(false);
    const payload = {
      version: order.version,
      ...(action === "cancel" ? { reason } : {}),
    };
    identity.current = requestIdentity(identity.current, {
      action,
      ...payload,
    });
    try {
      await api(`/purchase-orders/${id}/${action}`, {
        method: "POST",
        body: { ...payload, idempotency_key: identity.current.key },
      });
      setAction(null);
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
  function reloadLatest() {
    setAction(null);
    setConflict(false);
    resource.reload();
    setError(
      "Data terbaru sedang dimuat. Tinjau status dan isi pesanan sebelum mencoba kembali.",
    );
  }
  return (
    <Modal
      wide
      title={order?.number ?? "Detail pesanan pembelian"}
      description="Kesepakatan pembelian yang tersimpan untuk riwayat usaha."
      onClose={() => {
        if (!busyRef.current) onClose();
      }}
    >
      {resource.loading ? (
        <Loading />
      ) : resource.error ? (
        <ErrorState message={resource.error} onRetry={resource.reload} />
      ) : (
        order && (
          <>
            <div className="purchase-detail-head">
              <div>
                <small>SUPPLIER</small>
                <h3>{order.supplier_name}</h3>
                <p>{order.supplier_code}</p>
              </div>
              <Status status={order.status} />
            </div>
            <div className="purchase-detail-meta">
              <div>
                <small>Dibuat</small>
                <span>{dateTime(order.created_at)}</span>
              </div>
              <div>
                <small>Perkiraan barang tiba</small>
                <span>
                  {order.expected_date
                    ? calendarDate(order.expected_date)
                    : "Belum ditentukan"}
                </span>
              </div>
              {order.confirmed_at && (
                <div>
                  <small>Dikonfirmasi</small>
                  <span>{dateTime(order.confirmed_at)}</span>
                </div>
              )}
              {order.cancelled_at && (
                <div>
                  <small>Dibatalkan</small>
                  <span>{dateTime(order.cancelled_at)}</span>
                </div>
              )}
            </div>
            <div className="table-scroll">
              <table className="purchase-detail-lines">
                <thead>
                  <tr>
                    <th>Barang</th>
                    <th className="align-right">Jumlah</th>
                    <th className="align-right">Biaya beli / satuan</th>
                    <th className="align-right">Total</th>
                  </tr>
                </thead>
                <tbody>
                  {order.items.map((item) => (
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
                      <td className="align-right">
                        {exactCost(item.unit_cost)}
                      </td>
                      <td className="align-right font-medium">
                        {exactMoney(item.total)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="purchase-detail-total">
              <span>Total pesanan</span>
              <strong>{exactMoney(order.total)}</strong>
            </div>
            <div className="purchase-detail-notes">
              {order.notes && (
                <div>
                  <small>Catatan</small>
                  <p className="purchase-prewrap">{order.notes}</p>
                </div>
              )}
              {order.cancellation_reason && (
                <div className="purchase-cancel-reason">
                  <small>Alasan pembatalan</small>
                  <p className="purchase-prewrap">
                    {order.cancellation_reason}
                  </p>
                </div>
              )}
              <p className="purchase-history-note">
                Nama supplier, barang, satuan, dan biaya pada dokumen ini
                menggunakan data tersimpan. PO belum mencatat stok, utang, atau
                jurnal.
              </p>
            </div>
            {action && (
              <form className="purchase-review" onSubmit={submit}>
                <div className="purchase-review-title">
                  {action === "confirm" ? (
                    <Check size={19} />
                  ) : (
                    <XCircle size={19} />
                  )}
                  <strong>
                    {action === "confirm"
                      ? "Konfirmasi kesepakatan pembelian"
                      : "Batalkan pesanan pembelian"}
                  </strong>
                </div>
                <p>
                  {action === "confirm"
                    ? `Pastikan supplier, barang, dan total ${exactMoney(order.total)} sudah benar. Setelah konfirmasi, isi PO tidak dapat diedit.`
                    : "Pesanan akan tetap tersimpan dengan status Dibatalkan beserta alasan di bawah."}
                </p>
                {action === "cancel" && (
                  <FormField
                    label="Alasan pembatalan *"
                    hint="3–1.000 karakter; tersimpan dalam riwayat."
                  >
                    <textarea
                      required
                      minLength={3}
                      maxLength={1000}
                      rows={3}
                      value={reason}
                      disabled={busy}
                      onChange={(event) => setReason(event.target.value)}
                      placeholder="Jelaskan mengapa pesanan dibatalkan"
                      autoFocus
                    />
                  </FormField>
                )}
                <FormError error={error} />
                <div className="purchase-review-actions">
                  <button
                    type="button"
                    className="button secondary"
                    disabled={busy}
                    onClick={() => {
                      setAction(null);
                      setError("");
                      setConflict(false);
                    }}
                  >
                    Kembali ke detail
                  </button>
                  <button
                    className={`button ${action === "cancel" ? "secondary danger-text" : "primary"}`}
                    type="submit"
                    disabled={busy}
                  >
                    {busy
                      ? "Memproses…"
                      : action === "confirm"
                        ? "Konfirmasi PO"
                        : "Batalkan PO"}
                  </button>
                </div>
              </form>
            )}
            {!action && error && (
              <div className="purchase-action-error">
                <FormError error={error} />
              </div>
            )}
            {conflict && (
              <div className="purchase-action-error">
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={reloadLatest}
                >
                  <RefreshCw size={15} />
                  Muat versi terbaru
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
          order &&
          !resource.loading &&
          !resource.error &&
          !action &&
          order.status !== "cancelled" && (
            <>
              <button
                className="text-button danger-text"
                disabled={busy}
                onClick={() => {
                  setAction("cancel");
                  setError("");
                }}
              >
                Batalkan PO
              </button>
              {order.status === "draft" && (
                <>
                  <button
                    className="button secondary"
                    disabled={busy}
                    onClick={() => onEdit(order)}
                  >
                    Edit draf
                  </button>
                  <button
                    className="button primary"
                    disabled={busy}
                    onClick={() => {
                      setAction("confirm");
                      setError("");
                    }}
                  >
                    Tinjau konfirmasi <ArrowRight size={15} />
                  </button>
                </>
              )}
            </>
          )}
      </div>
    </Modal>
  );
}
