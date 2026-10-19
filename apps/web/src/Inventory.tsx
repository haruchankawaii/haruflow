import { useState, type FormEvent } from "react";
import {
  ArrowDownToLine,
  ArrowUpRight,
  Boxes,
  Package,
  Plus,
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
  MovementIcon,
  PageHeader,
  Pagination,
  SearchInput,
} from "./components";
import { costMoney, date, money, number, statusNames } from "./format";
import { AdjustmentForm } from "./Corrections";
import { downloadCsv } from "./export";
import { DeleteMaster } from "./MasterActions";
import { usePagination, useResource } from "./hooks";
import { isStockProduct } from "./commerce";
import { DocumentReversal, ReceiptHistory } from "./DocumentReversal";
import type { Movement, Product, Role } from "./types";

export default function Inventory({
  role,
  changed,
}: {
  role: Role;
  changed: () => void;
}) {
  const [tab, setTab] = useState("products");
  const [revision, setRevision] = useState(0);
  const products = useResource<{ items: Product[] }>("/products", revision);
  const movements = useResource<{ items: Movement[] }>(
    "/inventory/movements",
    revision,
  );
  const [search, setSearch] = useState("");
  const [form, setForm] = useState<"product" | "receipt" | "adjustment" | null>(
    null,
  );
  const [editing, setEditing] = useState<Product | null>(null);
  const [deleting, setDeleting] = useState<Product | null>(null);
  const [reversing, setReversing] = useState<Movement | null>(null);
  const [receiptHistory, setReceiptHistory] = useState<Movement | null>(null);
  const canEdit = ["admin", "warehouse"].includes(role);
  const stockProducts = (products.data?.items ?? []).filter(isStockProduct);
  const items = (products.data?.items ?? []).filter((item) =>
    `${item.sku} ${item.name}`.toLowerCase().includes(search.toLowerCase()),
  );
  const movementsList = (movements.data?.items ?? []).filter((item) =>
    `${item.product_name} ${item.reference ?? ""} ${item.purchase_order_number ?? ""}`
      .toLowerCase()
      .includes(search.toLowerCase()),
  );
  const inventoryValue = stockProducts.reduce(
    (value, item) =>
      value +
      Number(
        item.inventory_value ??
          Number(item.quantity) * Number(item.average_cost),
      ),
    0,
  );
  const productPagination = usePagination(items, search);
  const movementPagination = usePagination(movementsList, search);
  function saved() {
    setForm(null);
    setRevision((value) => value + 1);
    changed();
  }
  return (
    <>
      <PageHeader
        eyebrow="SETIAP PRODUK, TERPANTAU"
        title="Inventori"
        description="Kelola katalog barang dan jasa, serta persediaan barang Anda."
        actions={
          canEdit && (
            <>
              <button
                className="button secondary"
                disabled={!stockProducts.length}
                onClick={() => setForm("adjustment")}
              >
                Sesuaikan stok
              </button>
              <button
                className="button secondary"
                disabled={!stockProducts.length}
                onClick={() => setForm("receipt")}
              >
                <ArrowDownToLine size={17} /> Terima stok
              </button>
              <button
                className="button primary"
                onClick={() => setForm("product")}
              >
                <Plus size={17} /> Tambah produk
              </button>
            </>
          )
        }
      />
      <div className="compact-metrics">
        <div>
          <span className="metric-icon green">
            <Package size={20} />
          </span>
          <div>
            <small>Barang & jasa terdaftar</small>
            <strong>
              {number(products.data?.items.length ?? 0)} <em>produk</em>
            </strong>
          </div>
        </div>
        <div>
          <span className="metric-icon violet">
            <Boxes size={20} />
          </span>
          <div>
            <small>Nilai persediaan</small>
            <strong>{money(inventoryValue)}</strong>
          </div>
        </div>
        <div>
          <span className="metric-icon amber">
            <ArrowUpRight size={20} />
          </span>
          <div>
            <small>Barang tanpa stok</small>
            <strong>
              {number(
                stockProducts.filter((item) => Number(item.quantity) <= 0)
                  .length,
              )}{" "}
              <em>produk</em>
            </strong>
          </div>
        </div>
      </div>
      <section className="panel">
        <div className="table-toolbar">
          <div className="tabs" role="tablist" aria-label="Tampilan inventori">
            <button
              role="tab"
              aria-selected={tab === "products"}
              className={tab === "products" ? "active" : ""}
              onClick={() => {
                setTab("products");
                setSearch("");
              }}
            >
              Katalog produk
            </button>
            <button
              role="tab"
              aria-selected={tab === "movements"}
              className={tab === "movements" ? "active" : ""}
              onClick={() => {
                setTab("movements");
                setSearch("");
              }}
            >
              Pergerakan stok
            </button>
          </div>
          <button
            className="text-button export-button"
            onClick={() =>
              downloadCsv(
                tab === "products" ? "haruflow-produk" : "haruflow-pergerakan",
                tab === "products"
                  ? [
                      "SKU",
                      "Nama",
                      "Jenis",
                      "Satuan",
                      "Harga",
                      "Stok",
                      "Biaya rata-rata",
                    ]
                  : [
                      "Tanggal",
                      "Jenis",
                      "Produk",
                      "Jumlah",
                      "Biaya satuan",
                      "Nilai transaksi",
                      "Referensi",
                      "Pesanan pembelian",
                      "Status koreksi",
                      "Alasan koreksi",
                    ],
                tab === "products"
                  ? items.map((p) => [
                      p.sku,
                      p.name,
                      p.kind ?? "stock",
                      p.unit,
                      p.price,
                      isStockProduct(p) ? p.quantity : "",
                      isStockProduct(p) ? p.average_cost : "",
                    ])
                  : movementsList.map((m) => [
                      m.created_at,
                      m.kind,
                      m.product_name,
                      m.quantity,
                      m.unit_cost,
                      m.value,
                      m.reference,
                      m.purchase_order_number,
                      m.reversal ? "Dibatalkan" : "",
                      m.reversal?.reason,
                    ]),
              )
            }
          >
            Ekspor CSV
          </button>
          <SearchInput
            value={search}
            onChange={setSearch}
            placeholder={
              tab === "products"
                ? "Cari produk atau SKU…"
                : "Cari produk atau referensi…"
            }
          />
        </div>
        {tab === "products" ? (
          products.loading ? (
            <Loading />
          ) : products.error ? (
            <ErrorState message={products.error} onRetry={products.reload} />
          ) : items.length ? (
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Produk</th>
                    <th>SKU</th>
                    <th className="align-right">Harga jual</th>
                    <th className="align-right">Stok tersedia</th>
                    <th className="align-right">Biaya rata-rata</th>
                    <th className="align-right">Nilai stok</th>
                  </tr>
                </thead>
                <tbody>
                  {productPagination.items.map((product) => (
                    <tr key={product.id}>
                      <td>
                        <span className="customer-cell">
                          <span className="product-avatar">
                            <Package size={19} strokeWidth={1.5} />
                          </span>
                          {canEdit ? (
                            <button
                              className="table-link"
                              onClick={() => setEditing(product)}
                            >
                              {product.name}
                            </button>
                          ) : (
                            <strong>{product.name}</strong>
                          )}
                          <span className="subtle-chip">
                            {isStockProduct(product) ? "Barang" : "Jasa"}
                          </span>
                        </span>
                      </td>
                      <td className="mono muted">{product.sku}</td>
                      <td className="align-right">{money(product.price)}</td>
                      <td className="align-right">
                        {isStockProduct(product) ? (
                          <>
                            <span
                              className={
                                Number(product.quantity) <= 0
                                  ? "zero-stock"
                                  : "font-medium"
                              }
                            >
                              {number(product.quantity)}
                            </span>{" "}
                            <span className="muted">{product.unit}</span>
                          </>
                        ) : (
                          <span className="muted">Tanpa stok</span>
                        )}
                      </td>
                      <td className="align-right muted">
                        {isStockProduct(product)
                          ? costMoney(product.average_cost)
                          : "—"}
                      </td>
                      <td className="align-right font-medium">
                        {isStockProduct(product)
                          ? money(
                              product.inventory_value ??
                                Number(product.quantity) *
                                  Number(product.average_cost),
                            )
                          : "—"}
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
                  ? "Produk tidak ditemukan"
                  : "Katalog Anda dimulai di sini."
              }
              description={
                search
                  ? "Coba nama atau SKU yang berbeda."
                  : "Tambahkan barang atau jasa dan tentukan harganya. Terima stok awal untuk barang."
              }
              action={
                canEdit &&
                !search && (
                  <button
                    className="button secondary"
                    onClick={() => setForm("product")}
                  >
                    <Plus size={16} /> Tambah produk
                  </button>
                )
              }
            />
          )
        ) : movements.loading ? (
          <Loading />
        ) : movements.error ? (
          <ErrorState message={movements.error} onRetry={movements.reload} />
        ) : movementsList.length ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Pergerakan</th>
                  <th>Produk</th>
                  <th>Referensi</th>
                  <th>Tanggal</th>
                  <th className="align-right">Jumlah</th>
                  <th className="align-right">Biaya satuan</th>
                  <th className="align-right">Nilai transaksi</th>
                  <th>Koreksi</th>
                </tr>
              </thead>
              <tbody>
                {movementPagination.items.map((item) => (
                  <tr key={item.id} id={`movement-${item.id}`}>
                    <td>
                      <span className="customer-cell">
                        <MovementIcon incoming={Number(item.quantity) > 0} />
                        {statusNames[item.kind] ?? item.kind}
                      </span>
                    </td>
                    <td className="font-medium">{item.product_name}</td>
                    <td className="mono muted">
                      {item.reference || "—"}
                      {item.purchase_order_id && (
                        <div>
                          <a
                            className="text-button"
                            href={`?purchase=${encodeURIComponent(item.purchase_order_id)}#purchasing`}
                          >
                            {item.purchase_order_number ?? "Lihat PO"}
                          </a>
                        </div>
                      )}
                      {item.sales_return_invoice_id && (
                        <div>
                          <a
                            className="text-button"
                            href={`?invoice=${encodeURIComponent(item.sales_return_invoice_id)}#sales`}
                          >
                            {item.sales_return_number ??
                              "Lihat retur pelanggan"}{" "}
                            · Invoice asal
                          </a>
                        </div>
                      )}
                      {item.supplier_return_bill_id && (
                        <div>
                          <a
                            className="text-button"
                            href={`?bill=${encodeURIComponent(item.supplier_return_bill_id)}#purchasing`}
                          >
                            {item.supplier_return_number ??
                              "Lihat retur supplier"}{" "}
                            · Tagihan asal
                          </a>
                        </div>
                      )}
                      {item.shipment_id && item.order_id && (
                        <div>
                          <a
                            className="text-button"
                            href={`?order=${encodeURIComponent(item.order_id)}#sales`}
                          >
                            Lihat pengiriman pesanan
                          </a>
                        </div>
                      )}
                    </td>
                    <td className="muted">{date(item.created_at)}</td>
                    <td className="align-right font-medium">
                      {number(item.quantity)}
                    </td>
                    <td className="align-right">{costMoney(item.unit_cost)}</td>
                    <td className="align-right">{money(item.value)}</td>
                    <td>
                      <div className="movement-correction">
                        {item.reversal ? (
                          <>
                            <strong>
                              Dibatalkan · {date(item.reversal.created_at)}
                            </strong>
                            <p>{item.reversal.reason}</p>
                          </>
                        ) : item.kind === "receipt" && canEdit ? (
                          item.can_reverse ? (
                            <button
                              className="text-button danger-text"
                              onClick={() => setReversing(item)}
                            >
                              Batalkan penerimaan
                            </button>
                          ) : (
                            <p className="muted">
                              {item.reversal_blocked_reason ??
                                "Penerimaan ini belum dapat dibatalkan."}
                            </p>
                          )
                        ) : item.kind === "receipt_reversal" &&
                          item.reverses_movement_id ? (
                          movements.data?.items.some(
                            (source) => source.id === item.reverses_movement_id,
                          ) ? (
                            <button
                              className="text-button"
                              onClick={() =>
                                setReceiptHistory(
                                  movements.data!.items.find(
                                    (source) =>
                                      source.id === item.reverses_movement_id,
                                  )!,
                                )
                              }
                            >
                              Lihat penerimaan asal
                            </button>
                          ) : (
                            <span className="muted">
                              Penerimaan asal tidak ada dalam daftar terbaru.
                            </span>
                          )
                        ) : (
                          !item.purchase_order_id && (
                            <span className="muted">—</span>
                          )
                        )}
                        {item.purchase_order_id && (
                          <a
                            className="text-button"
                            href={`?purchase=${encodeURIComponent(item.purchase_order_id)}#purchasing`}
                          >
                            Lihat penerimaan PO
                          </a>
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <EmptyState
            title="Belum ada pergerakan stok"
            description="Penerimaan dan pengiriman produk akan tercatat otomatis di sini."
          />
        )}
        <div className="table-footer">
          <Pagination
            {...(tab === "products" ? productPagination : movementPagination)}
          />
        </div>
      </section>
      {form === "product" && (
        <ProductForm onClose={() => setForm(null)} onSaved={saved} />
      )}
      {form === "adjustment" && (
        <AdjustmentForm
          products={stockProducts}
          onClose={() => setForm(null)}
          onSaved={saved}
        />
      )}
      {editing && (
        <ProductForm
          product={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            saved();
          }}
          onDelete={() => {
            setDeleting(editing);
            setEditing(null);
          }}
        />
      )}
      {deleting && (
        <DeleteMaster
          path={`/products/${deleting.id}`}
          name={deleting.name}
          onClose={() => setDeleting(null)}
          onSaved={() => {
            setDeleting(null);
            saved();
          }}
        />
      )}
      {form === "receipt" && (
        <ReceiptForm
          products={stockProducts}
          onClose={() => setForm(null)}
          onSaved={saved}
        />
      )}
      {reversing && (
        <DocumentReversal
          receipt={reversing}
          onClose={() => setReversing(null)}
          onSaved={() => {
            setReversing(null);
            saved();
          }}
        />
      )}
      {receiptHistory && (
        <ReceiptHistory
          receipt={receiptHistory}
          onClose={() => setReceiptHistory(null)}
        />
      )}
    </>
  );
}

function ProductForm({
  product,
  onClose,
  onSaved,
  onDelete,
}: {
  product?: Product;
  onClose: () => void;
  onSaved: () => void;
  onDelete?: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [kind, setKind] = useState(product?.kind ?? "stock");
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api(product ? `/products/${product.id}` : "/products", {
        method: product ? "PATCH" : "POST",
        body: Object.fromEntries(new FormData(event.currentTarget)),
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
      title={product ? "Edit produk" : "Tambah produk"}
      description="Simpan detail barang atau jasa ke dalam katalog Anda."
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <FormField
            label="Jenis produk *"
            hint={
              product
                ? "Jenis hanya dapat diubah sebelum produk digunakan pada pesanan atau pergerakan stok."
                : "Barang memiliki persediaan. Jasa dapat ditagihkan tanpa pengiriman stok."
            }
          >
            <select
              name="kind"
              value={kind}
              onChange={(event) =>
                setKind(event.target.value as "stock" | "service")
              }
            >
              <option value="stock">Barang — kelola stok</option>
              <option value="service">Jasa — tanpa stok</option>
            </select>
          </FormField>
          <FormField label="Nama produk *">
            <input
              name="name"
              defaultValue={product?.name}
              required
              maxLength={200}
              autoFocus
              placeholder="Contoh: Kopi Arabika 250 g"
            />
          </FormField>
          <div className="form-grid">
            <FormField label="SKU / kode produk *">
              <input
                name="sku"
                defaultValue={product?.sku}
                required
                maxLength={60}
                placeholder="KOP-ARA-250"
              />
            </FormField>
            <FormField label="Satuan *">
              <input
                name="unit"
                required
                maxLength={30}
                defaultValue={product?.unit ?? "pcs"}
                placeholder="pcs, kg, jam, sesi…"
              />
            </FormField>
          </div>
          <FormField
            label="Harga jual (Rp) *"
            hint="Gunakan titik untuk desimal. Contoh: 45000.50"
          >
            <input
              name="price"
              defaultValue={product?.price}
              type="number"
              required
              min="0.01"
              step="0.01"
              placeholder="0"
            />
          </FormField>
          <div className="info-box">
            <Package size={18} />
            <p>
              {kind === "service" ? (
                "Jasa tidak memiliki persediaan atau biaya rata-rata. Pesanan jasa dapat langsung ditagihkan setelah dikonfirmasi."
              ) : product ? (
                "Stok dan biaya persediaan dikelola melalui penerimaan atau penyesuaian, sehingga jejak transaksi tetap tercatat."
              ) : (
                <>
                  Produk baru dimulai dengan stok kosong. Gunakan{" "}
                  <strong>Terima stok</strong> untuk mencatat persediaan awal.
                </>
              )}
            </p>
          </div>
          <FormError error={error} />
        </div>
        <div className="master-form-actions">
          {onDelete && (
            <button
              className="text-button danger-text"
              type="button"
              disabled={busy}
              onClick={onDelete}
            >
              Hapus produk
            </button>
          )}
          <FormActions onClose={onClose} busy={busy} label="Simpan produk" />
        </div>
      </form>
    </Modal>
  );
}

function ReceiptForm({
  products,
  onClose,
  onSaved,
}: {
  products: Product[];
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
      await api("/inventory/receipts", {
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
      title="Terima stok"
      description="Persediaan dan jurnal akan diperbarui secara otomatis."
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <FormField label="Produk *">
            <select name="product_id" required autoFocus defaultValue="">
              <option value="" disabled>
                Pilih produk
              </option>
              {products.filter(isStockProduct).map((product) => (
                <option key={product.id} value={product.id}>
                  {product.sku} · {product.name} ({product.unit})
                </option>
              ))}
            </select>
          </FormField>
          <div className="form-grid">
            <FormField label="Jumlah diterima *">
              <input
                name="quantity"
                required
                type="number"
                min="0.001"
                step="0.001"
                placeholder="0"
              />
            </FormField>
            <FormField label="Biaya per satuan (Rp) *">
              <input
                name="unit_cost"
                required
                type="number"
                min="0.0001"
                step="0.0001"
                placeholder="0"
              />
            </FormField>
          </div>
          <FormField
            label="Referensi"
            hint="Nomor dokumen atau catatan asal stok."
          >
            <input
              name="reference"
              maxLength={150}
              placeholder="Contoh: Stok awal / PO-001"
            />
          </FormField>
          <div className="info-box">
            <ArrowDownToLine size={18} />
            <p>
              Stok masuk menambah persediaan. Pastikan jumlah dan biaya sudah
              benar sebelum disimpan.
            </p>
          </div>
          <FormError error={error} />
        </div>
        <FormActions onClose={onClose} busy={busy} label="Catat penerimaan" />
      </form>
    </Modal>
  );
}
