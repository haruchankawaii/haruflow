import { useState, type FormEvent } from "react";
import { CalendarCheck2, Plus, Scale, Trash2 } from "lucide-react";
import { api, errorMessage } from "./api";
import {
  ErrorState,
  FormActions,
  FormError,
  FormField,
  Loading,
  Modal,
} from "./components";
import { useResource } from "./hooks";
import { isControlAccount } from "./account-controls";
import { companyToday, money } from "./format";
import { isStockProduct } from "./commerce";
import type { Account, Product } from "./types";
import { StockContextFields } from "./StockContextFields";
import type { StockContextInput } from "./inventory-model";

export function AdjustmentForm({
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
  const [quantity, setQuantity] = useState("");
  const [key, setKey] = useState(() => crypto.randomUUID());
  const [productId, setProductId] = useState("");
  const [stockContext, setStockContext] = useState<StockContextInput>({});
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    const fields = Object.fromEntries(new FormData(event.currentTarget));
    if (Number(quantity) === 0) {
      setError(
        "Jumlah penyesuaian harus lebih besar atau lebih kecil dari nol.",
      );
      setBusy(false);
      return;
    }
    try {
      await api("/inventory/adjustments", {
        method: "POST",
        body: {
          ...fields,
          quantity,
          stock_context: stockContext,
          ...(Number(quantity) > 0 ? {} : { unit_cost: undefined }),
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
      title="Penyesuaian stok"
      description="Catat selisih stok fisik dengan alasan yang jelas."
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit} onChange={() => setKey(crypto.randomUUID())}>
        <div className="form-body">
          <FormField label="Produk *">
            <select name="product_id" required value={productId} autoFocus onChange={e => {setProductId(e.target.value);setStockContext({});}}>
              <option disabled value="">
                Pilih produk
              </option>
              {products.filter(isStockProduct).map((product) => (
                <option key={product.id} value={product.id}>
                  {product.sku} · {product.name}
                </option>
              ))}
            </select>
          </FormField>
          <FormField
            label="Selisih jumlah *"
            hint="Positif untuk menambah, negatif untuk mengurangi. Contoh: -2"
          >
            <input
              type="number"
              required
              step="0.001"
              value={quantity}
              onChange={(event) => setQuantity(event.target.value)}
              placeholder="0"
            />
          </FormField>
          {Number(quantity) > 0 && (
            <FormField label="Biaya satuan tambahan (Rp) *">
              <input
                name="unit_cost"
                type="number"
                required
                min="0.0001"
                step="0.0001"
              />
            </FormField>
          )}
          {productId && <details><summary>Gudang dan lot / seri</summary><StockContextFields productId={productId} value={stockContext} incoming={Number(quantity)>0} disabled={busy} onChange={value => {setStockContext(value);setKey(crypto.randomUUID());}} /></details>}
          <FormField label="Alasan penyesuaian *">
            <textarea
              name="reason"
              required
              minLength={3}
              maxLength={1000}
              rows={3}
              placeholder="Contoh: Selisih hasil stock opname"
            />
          </FormField>
          <div className="info-box">
            <Scale size={18} />
            <p>
              Penyesuaian langsung mengubah stok dan membuat jurnal penyesuaian
              persediaan. Periksa kembali sebelum menyimpan.
            </p>
          </div>
          <FormError error={error} />
        </div>
        <FormActions onClose={onClose} busy={busy} label="Simpan penyesuaian" />
      </form>
    </Modal>
  );
}

export function ManualJournalForm({
  onClose,
  onSaved,
}: {
  onClose: () => void;
  onSaved: () => void;
}) {
  const accounts = useResource<{ items: Account[] }>("/accounting/accounts");
  const [lines, setLines] = useState(() => [
    { key: crypto.randomUUID(), account_code: "", debit: "", credit: "" },
    { key: crypto.randomUUID(), account_code: "", debit: "", credit: "" },
  ]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [key] = useState(() => crypto.randomUUID());
  const debit = lines.reduce((sum, line) => sum + Number(line.debit), 0);
  const credit = lines.reduce((sum, line) => sum + Number(line.credit), 0);
  function update(index: number, field: string, value: string) {
    setLines((current) =>
      current.map((line, i) =>
        i === index ? { ...line, [field]: value } : line,
      ),
    );
  }
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api("/accounting/journals", {
        method: "POST",
        body: {
          description: new FormData(event.currentTarget).get("description"),
          lines: lines.map(({ account_code, debit, credit }) => ({
            account_code,
            debit: debit || "0",
            credit: credit || "0",
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
      title="Buat jurnal manual"
      description="Catat transaksi di luar alur penjualan dan persediaan."
      onClose={() => {
        if (!busy) onClose();
      }}
      wide
    >
      {accounts.loading ? (
        <Loading />
      ) : accounts.error ? (
        <ErrorState message={accounts.error} onRetry={accounts.reload} />
      ) : (
        <form onSubmit={submit}>
          <div className="form-body">
            <FormField label="Keterangan jurnal *">
              <textarea
                name="description"
                required
                minLength={3}
                maxLength={1000}
                rows={2}
                autoFocus
                placeholder="Contoh: Setoran modal awal"
              />
            </FormField>
            <div className="manual-labels">
              <span>Akun</span>
              <span>Debit (Rp)</span>
              <span>Kredit (Rp)</span>
              <span />
            </div>
            {lines.map((line, index) => (
              <div className="manual-line" key={line.key}>
                <select
                  required
                  aria-label={`Akun baris ${index + 1}`}
                  value={line.account_code}
                  onChange={(event) =>
                    update(index, "account_code", event.target.value)
                  }
                >
                  <option value="" disabled>
                    Pilih akun
                  </option>
                  {accounts.data?.items
                    .filter((account) => !isControlAccount(account))
                    .map((account) => (
                      <option key={account.code} value={account.code}>
                        {account.code} · {account.name}
                      </option>
                    ))}
                </select>
                <input
                  aria-label={`Debit baris ${index + 1}`}
                  type="number"
                  min="0"
                  step="0.01"
                  placeholder="0"
                  value={line.debit}
                  onChange={(event) =>
                    update(index, "debit", event.target.value)
                  }
                />
                <input
                  aria-label={`Kredit baris ${index + 1}`}
                  type="number"
                  min="0"
                  step="0.01"
                  placeholder="0"
                  value={line.credit}
                  onChange={(event) =>
                    update(index, "credit", event.target.value)
                  }
                />
                <button
                  className="icon-button danger-text"
                  type="button"
                  aria-label={`Hapus jurnal baris ${index + 1}`}
                  disabled={lines.length <= 2}
                  onClick={() =>
                    setLines((current) => current.filter((_, i) => i !== index))
                  }
                >
                  <Trash2 size={16} />
                </button>
              </div>
            ))}
            <button
              className="text-button"
              type="button"
              disabled={lines.length >= 100}
              onClick={() =>
                setLines((current) => [
                  ...current,
                  {
                    key: crypto.randomUUID(),
                    account_code: "",
                    debit: "",
                    credit: "",
                  },
                ])
              }
            >
              <Plus size={16} /> Tambah baris
            </button>
            <div className="manual-totals">
              <span>
                Total debit <strong>{money(debit)}</strong>
              </span>
              <span>
                Total kredit <strong>{money(credit)}</strong>
              </span>
            </div>
            <div className="info-box">
              <Scale size={18} />
              <p>
                Total debit dan kredit harus seimbang. Akun kontrol piutang,
                persediaan, barang diterima belum ditagih, dan utang supplier
                dikelola melalui alur operasional.
              </p>
            </div>
            <FormError error={error} />
          </div>
          <FormActions onClose={onClose} busy={busy} label="Posting jurnal" />
        </form>
      )}
    </Modal>
  );
}

export function PeriodForm({
  onClose,
  onSaved,
}: {
  onClose: () => void;
  onSaved: () => void;
}) {
  const {
    data,
    loading,
    error: resourceError,
    reload,
  } = useResource<{ closed_through: string | null }>("/accounting/period");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [key] = useState(() => crypto.randomUUID());
  const today = companyToday();
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api("/accounting/period/close", {
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
      title="Tutup periode pembukuan"
      description="Kunci transaksi periode yang telah selesai diperiksa."
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      {loading ? (
        <Loading />
      ) : resourceError ? (
        <ErrorState message={resourceError} onRetry={reload} />
      ) : (
        <form onSubmit={submit}>
          <div className="form-body">
            <div className="info-box">
              <CalendarCheck2 size={18} />
              <p>
                {data?.closed_through
                  ? `Periode telah ditutup sampai ${data.closed_through}.`
                  : "Belum ada periode yang ditutup."}{" "}
                Transaksi baru tidak dapat diposting pada tanggal yang sudah
                ditutup.
              </p>
            </div>
            <FormField label="Tutup sampai tanggal *">
              <input
                name="through"
                type="date"
                required
                max={today}
                min={data?.closed_through ?? undefined}
              />
            </FormField>
            <FormField label="Alasan penutupan *">
              <textarea
                name="reason"
                required
                minLength={3}
                maxLength={1000}
                rows={3}
                placeholder="Contoh: Pembukuan bulan sebelumnya selesai diperiksa"
              />
            </FormField>
            <FormField label="Konfirmasi">
              <span className="checkbox-field">
                <input type="checkbox" required /> Saya sudah memeriksa
                transaksi pada periode ini.
              </span>
            </FormField>
            <FormError error={error} />
          </div>
          <FormActions onClose={onClose} busy={busy} label="Tutup periode" />
        </form>
      )}
    </Modal>
  );
}
