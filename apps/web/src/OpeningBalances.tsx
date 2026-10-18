import { useContext, useEffect, useRef, useState, type FormEvent } from "react";
import {
  CheckCheck,
  CircleAlert,
  History,
  LockKeyhole,
  PackagePlus,
  Plus,
  RotateCcw,
  Save,
  Scale,
  Trash2,
} from "lucide-react";
import { api, ApiError, errorMessage } from "./api";
import {
  EmptyState,
  ErrorState,
  FormError,
  FormField,
  Loading,
  Modal,
  Status,
} from "./components";
import { companyToday, costMoney, dateTime, money, number } from "./format";
import { ResourceRevisionContext, useResource } from "./hooks";
import {
  openingAccounts,
  openingDifference,
  openingDraft,
  openingIsBalanced,
  openingItemValue,
  openingPreview,
  openingProducts,
} from "./opening";
import type { OpeningDraft, OpeningSnapshot, OpeningState } from "./opening";
import type { Account, Product, Role } from "./types";

export default function OpeningBalances({
  role,
  changed,
}: {
  role: Role;
  changed: () => void;
}) {
  const [state, setState] = useState<OpeningState | null>(null);
  const [draft, setDraft] = useState<OpeningDraft>(() => openingDraft(null));
  const [baseline, setBaseline] = useState(() =>
    JSON.stringify(openingDraft(null)),
  );
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  const [error, setError] = useState("");
  const [stale, setStale] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [modal, setModal] = useState<"review" | "reverse" | "reload" | null>(
    null,
  );
  const actionKeys = useRef(new Map<string, string>());
  const workspaceRevision = useContext(ResourceRevisionContext);
  const products = useResource<{ items: Product[] }>("/products");
  const accounts = useResource<{ items: Account[] }>("/accounting/accounts");
  const period = useResource<{ closed_through: string | null }>(
    "/accounting/period",
  );
  const periodClosed =
    !!period.data?.closed_through &&
    period.data.closed_through >= companyToday();
  const canEdit = ["admin", "finance"].includes(role);
  const document = state?.document;
  const posted = document?.status === "posted";
  const editable = canEdit && !!state?.eligible && !posted;
  const dirty = JSON.stringify(draft) !== baseline;
  const latest = useRef({ state, dirty });
  latest.current = { state, dirty };
  const preview = openingPreview(draft, accounts.data?.items ?? []);
  const stockProducts = openingProducts(products.data?.items ?? []);
  const selectableAccounts = openingAccounts(accounts.data?.items ?? []);

  function accept(next: OpeningState) {
    const nextDraft = openingDraft(next.document);
    setState(next);
    setDraft(nextDraft);
    setBaseline(JSON.stringify(nextDraft));
    setStale(false);
    setError("");
  }
  async function load() {
    setLoading(true);
    setLoadError("");
    try {
      accept(await api<OpeningState>("/accounting/opening"));
      setModal(null);
    } catch (error) {
      setLoadError(errorMessage(error));
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => {
    const controller = new AbortController();
    api<OpeningState>("/accounting/opening", { signal: controller.signal })
      .then((result) => {
        if (controller.signal.aborted) return;
        // Header actions may change eligibility. Keep unsaved input while
        // checking the latest state, and require an explicit reload on conflict.
        if (latest.current.dirty) {
          if (JSON.stringify(result) !== JSON.stringify(latest.current.state)) {
            setStale(true);
            setError(
              "Status saldo awal berubah setelah transaksi lain. Isian Anda tetap ada; muat versi server sebelum melanjutkan.",
            );
          }
        } else accept(result);
      })
      .catch((error) => {
        if (controller.signal.aborted) return;
        if (latest.current.state) {
          setError(errorMessage(error));
          setStale(true);
        } else setLoadError(errorMessage(error));
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [workspaceRevision]);
  useEffect(() => {
    if (!dirty) return;
    const preventLeave = (event: BeforeUnloadEvent) => event.preventDefault();
    window.addEventListener("beforeunload", preventLeave);
    return () => window.removeEventListener("beforeunload", preventLeave);
  }, [dirty]);
  function failure(error: unknown) {
    setError(
      error instanceof ApiError &&
        error.message === "The current accounting period is closed"
        ? "Periode pembukuan saat ini sudah ditutup. Saldo awal belum dapat diposting atau dibalik."
        : errorMessage(error),
    );
    if (error instanceof ApiError && error.status === 409) setStale(true);
  }
  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!editable || stale) return;
    if (!preview.valid) {
      setError(
        "Periksa baris barang dan akun. Gunakan angka sesuai batas desimal, nilai stok lebih dari nol, dan hanya satu sisi debit atau kredit untuk setiap akun.",
      );
      return;
    }
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const next = await api<OpeningState>("/accounting/opening", {
        method: "PUT",
        body: {
          ...draft,
          version: document?.version ?? 0,
          lines: draft.lines.map((line) => ({
            ...line,
            debit: line.debit || "0",
            credit: line.credit || "0",
          })),
        },
      });
      accept(next);
      setNotice("Draf tersimpan. Stok dan pembukuan belum berubah.");
    } catch (error) {
      failure(error);
    } finally {
      setBusy(false);
    }
  }
  async function perform(action: "post" | "reverse", reason?: string) {
    if (!document) return;
    setBusy(true);
    setError("");
    setNotice("");
    const keyId = `${action}:${document.version}`;
    if (!actionKeys.current.has(keyId))
      actionKeys.current.set(keyId, crypto.randomUUID());
    try {
      const next = await api<OpeningState>(`/accounting/opening/${action}`, {
        method: "POST",
        body: {
          version: document.version,
          idempotency_key: actionKeys.current.get(keyId),
          ...(reason ? { reason } : {}),
        },
      });
      accept(next);
      setModal(null);
      setNotice(
        action === "post"
          ? "Saldo awal diposting. Stok dan neraca saldo telah diperbarui."
          : "Saldo awal dibalik. Perbaiki draf lalu periksa kembali sebelum memposting ulang.",
      );
      changed();
    } catch (error) {
      failure(error);
    } finally {
      setBusy(false);
    }
  }
  function edit(next: OpeningDraft) {
    setDraft(next);
    setNotice("");
  }

  if (loading) return <Loading label="Memuat saldo awal…" />;
  if (loadError || !state)
    return <ErrorState message={loadError} onRetry={() => void load()} />;
  return (
    <div className="opening-workspace">
      <div className="opening-heading">
        <div>
          <h2>Mulai dari posisi usaha Anda.</h2>
          <p>
            Catat kas, persediaan, aset, kewajiban, dan modal yang sudah
            dimiliki sebelum memakai Haruflow. Saldo awal dibuat sekali pada
            awal pembukuan, bukan untuk mengulang saldo setiap tahun.
          </p>
        </div>
        {document && (
          <Status status={document.status === "posted" ? "posted" : "draft"} />
        )}
      </div>
      <ol className="opening-steps" aria-label="Tahapan saldo awal">
        <li className={!posted ? "current" : "complete"}>
          <span>1</span> Siapkan draf
        </li>
        <li className={modal === "review" ? "current" : ""}>
          <span>2</span> Periksa keseimbangan
        </li>
        <li className={posted ? "current" : ""}>
          <span>3</span> Posting saldo awal
        </li>
      </ol>
      {!state.eligible && !posted && (
        <div className="opening-notice" role="status">
          <LockKeyhole size={20} />
          <div>
            <strong>
              Saldo awal tidak dapat ditambahkan ke pembukuan ini.
            </strong>
            <p>
              Sudah ada jurnal, pergerakan, atau saldo stok. Saldo awal perlu
              disiapkan sebelum transaksi pertama agar saldo tidak tercatat dua
              kali. Data dan saldo yang sudah ada tetap tersimpan.
            </p>
          </div>
        </div>
      )}
      <div className="info-box">
        <Scale size={18} />
        <p>
          Nilai persediaan otomatis masuk debit akun 1300. Masukkan saldo kas
          atau aset lain pada debit dan modal atau kewajiban pada kredit sesuai
          posisi usaha. Contoh: kas Rp100.000 + stok Rp500.000 = modal
          Rp600.000. Saldo awal piutang per pelanggan belum tersedia.
        </p>
      </div>
      {!canEdit && (
        <p className="opening-help">
          Hanya administrator dan keuangan yang dapat menyiapkan atau memposting
          saldo awal.
        </p>
      )}
      {periodClosed && (
        <div className="opening-notice">
          <LockKeyhole size={18} />
          <div>
            <strong>Periode pembukuan saat ini sudah ditutup.</strong>
            <p>
              Draf tetap dapat disiapkan. Posting dan pembalikan belum dapat
              dilakukan pada periode yang ditutup.
            </p>
          </div>
        </div>
      )}
      {period.error && (
        <ErrorState message={period.error} onRetry={period.reload} />
      )}
      {notice && (
        <div className="opening-success" role="status">
          <CheckCheck size={18} />
          {notice}
        </div>
      )}
      <FormError error={error} />
      {stale && (
        <div className="opening-conflict">
          <p>
            Isian Anda tetap ada. Muat versi server untuk memeriksa perubahan
            atau status pembukuan terbaru; isian yang belum tersimpan akan
            diganti.
          </p>
          <button
            type="button"
            className="button secondary"
            disabled={busy}
            onClick={() => setModal("reload")}
          >
            Muat versi server
          </button>
        </div>
      )}

      {posted ? (
        <>
          <div className="opening-section-heading">
            <div>
              <h3>Saldo awal yang sudah diposting</h3>
              <p>
                {document.posted_at ? dateTime(document.posted_at) : ""} ·{" "}
                {document.description}
              </p>
            </div>
            <CheckCheck size={22} />
          </div>
          <Snapshot snapshot={document} />
          {canEdit && (
            <div className="opening-actions">
              <p>
                {periodClosed
                  ? "Periode pembukuan saat ini sudah ditutup. Saldo awal belum dapat dibalik."
                  : state.can_reverse
                    ? "Kesalahan masih dapat diperbaiki dengan pembalikan karena belum ada transaksi lain. Riwayat lama tetap disimpan."
                    : "Saldo awal sudah terkunci karena ada transaksi lain. Riwayat dan angka awal tetap dapat diperiksa di sini."}
              </p>
              <button
                type="button"
                className="button secondary"
                disabled={
                  !state.can_reverse ||
                  busy ||
                  periodClosed ||
                  period.loading ||
                  !!period.error
                }
                onClick={() => {
                  setError("");
                  setModal("reverse");
                }}
              >
                <RotateCcw size={16} /> Balikkan untuk koreksi
              </button>
            </div>
          )}
        </>
      ) : editable ? (
        products.loading || accounts.loading ? (
          <Loading label="Memuat produk dan akun…" />
        ) : products.error || accounts.error ? (
          <ErrorState
            message={products.error || accounts.error}
            onRetry={() => {
              products.reload();
              accounts.reload();
            }}
          />
        ) : (
          <form onSubmit={save}>
            <fieldset className="opening-form" disabled={busy}>
              <FormField label="Keterangan saldo awal *">
                <textarea
                  rows={2}
                  minLength={1}
                  maxLength={500}
                  required
                  value={draft.description}
                  onChange={(e) =>
                    edit({ ...draft, description: e.target.value })
                  }
                />
              </FormField>
              <div className="opening-section-heading">
                <div>
                  <h3>Persediaan barang</h3>
                  <p>
                    Gunakan jumlah fisik dan biaya per unit, bukan harga jual.
                    Produk jasa tidak memiliki saldo persediaan.
                  </p>
                </div>
                <PackagePlus size={21} />
              </div>
              {!stockProducts.length && (
                <p className="opening-help">
                  Tambahkan produk barang di menu Inventori terlebih dahulu bila
                  usaha Anda memiliki stok awal.
                </p>
              )}
              {draft.items.map((item, index) => {
                const product = stockProducts.find(
                  (product) => product.id === item.product_id,
                );
                return (
                  <div className="opening-stock-row" key={index}>
                    <FormField label={`Barang ${index + 1}`}>
                      <select
                        required
                        value={item.product_id}
                        onChange={(e) =>
                          edit({
                            ...draft,
                            items: draft.items.map((row, i) =>
                              i === index
                                ? { ...row, product_id: e.target.value }
                                : row,
                            ),
                          })
                        }
                      >
                        <option value="" disabled>
                          Pilih barang
                        </option>
                        {stockProducts.map((product) => (
                          <option
                            key={product.id}
                            value={product.id}
                            disabled={draft.items.some(
                              (other, i) =>
                                i !== index && other.product_id === product.id,
                            )}
                          >
                            {product.sku} · {product.name}
                          </option>
                        ))}
                      </select>
                    </FormField>
                    <FormField
                      label={`Jumlah ${product?.unit ? `(${product.unit})` : ""}`}
                    >
                      <input
                        aria-label={`Jumlah barang ${index + 1}`}
                        type="number"
                        min="0.001"
                        step="0.001"
                        max="1000000000"
                        required
                        value={item.quantity}
                        onChange={(e) =>
                          edit({
                            ...draft,
                            items: draft.items.map((row, i) =>
                              i === index
                                ? { ...row, quantity: e.target.value }
                                : row,
                            ),
                          })
                        }
                      />
                    </FormField>
                    <FormField label="Biaya per unit (Rp)">
                      <input
                        aria-label={`Biaya barang ${index + 1}`}
                        type="number"
                        min="0.0001"
                        step="0.0001"
                        max="1000000000"
                        required
                        value={item.unit_cost}
                        onChange={(e) =>
                          edit({
                            ...draft,
                            items: draft.items.map((row, i) =>
                              i === index
                                ? { ...row, unit_cost: e.target.value }
                                : row,
                            ),
                          })
                        }
                      />
                    </FormField>
                    <div className="opening-row-value">
                      <small>Nilai stok</small>
                      <strong>
                        {money(openingItemValue(item.quantity, item.unit_cost))}
                      </strong>
                    </div>
                    <button
                      type="button"
                      className="icon-button danger-text"
                      aria-label={`Hapus barang ${index + 1}`}
                      onClick={() =>
                        edit({
                          ...draft,
                          items: draft.items.filter((_, i) => i !== index),
                        })
                      }
                    >
                      <Trash2 size={16} />
                    </button>
                  </div>
                );
              })}
              <button
                type="button"
                className="text-button"
                disabled={
                  !stockProducts.length ||
                  draft.items.length >= Math.min(100, stockProducts.length)
                }
                onClick={() =>
                  edit({
                    ...draft,
                    items: [
                      ...draft.items,
                      { product_id: "", quantity: "", unit_cost: "" },
                    ],
                  })
                }
              >
                <Plus size={16} /> Tambah barang
              </button>
              <div className="opening-auto-line">
                <span>
                  1300 · Persediaan barang <small>Debit otomatis</small>
                </span>
                <strong>{money(preview.inventory)}</strong>
              </div>
              <div className="opening-section-heading">
                <div>
                  <h3>Saldo akun lainnya</h3>
                  <p>
                    Isi satu sisi pada setiap akun: debit atau kredit. Akun
                    pendapatan, beban, dan akun kontrol operasional tidak
                    dipilih di sini, termasuk barang diterima belum ditagih.
                  </p>
                </div>
              </div>
              {draft.lines.map((line, index) => (
                <div className="opening-account-row" key={index}>
                  <FormField label={`Akun ${index + 1}`}>
                    <select
                      required
                      value={line.account_code}
                      onChange={(e) =>
                        edit({
                          ...draft,
                          lines: draft.lines.map((row, i) =>
                            i === index
                              ? { ...row, account_code: e.target.value }
                              : row,
                          ),
                        })
                      }
                    >
                      <option value="" disabled>
                        Pilih akun
                      </option>
                      {selectableAccounts.map((account) => (
                        <option
                          key={account.code}
                          value={account.code}
                          disabled={draft.lines.some(
                            (other, i) =>
                              i !== index &&
                              other.account_code === account.code,
                          )}
                        >
                          {account.code} · {account.name}
                        </option>
                      ))}
                    </select>
                  </FormField>
                  <FormField label="Debit (Rp)">
                    <input
                      aria-label={`Debit saldo awal ${index + 1}`}
                      type="number"
                      min="0"
                      max="1000000000000"
                      step="0.01"
                      placeholder="0"
                      value={line.debit}
                      onChange={(e) =>
                        edit({
                          ...draft,
                          lines: draft.lines.map((row, i) =>
                            i === index
                              ? { ...row, debit: e.target.value }
                              : row,
                          ),
                        })
                      }
                    />
                  </FormField>
                  <FormField label="Kredit (Rp)">
                    <input
                      aria-label={`Kredit saldo awal ${index + 1}`}
                      type="number"
                      min="0"
                      max="1000000000000"
                      step="0.01"
                      placeholder="0"
                      value={line.credit}
                      onChange={(e) =>
                        edit({
                          ...draft,
                          lines: draft.lines.map((row, i) =>
                            i === index
                              ? { ...row, credit: e.target.value }
                              : row,
                          ),
                        })
                      }
                    />
                  </FormField>
                  <button
                    type="button"
                    className="icon-button danger-text"
                    aria-label={`Hapus akun ${index + 1}`}
                    onClick={() =>
                      edit({
                        ...draft,
                        lines: draft.lines.filter((_, i) => i !== index),
                      })
                    }
                  >
                    <Trash2 size={16} />
                  </button>
                </div>
              ))}
              <button
                type="button"
                className="text-button"
                disabled={
                  draft.lines.length >= Math.min(100, selectableAccounts.length)
                }
                onClick={() =>
                  edit({
                    ...draft,
                    lines: [
                      ...draft.lines,
                      { account_code: "", debit: "", credit: "" },
                    ],
                  })
                }
              >
                <Plus size={16} /> Tambah akun
              </button>
              <Totals
                debit={preview.debit}
                credit={preview.credit}
                difference={preview.difference}
                balanced={preview.balanced}
              />
              {!preview.valid && (
                <p className="opening-help">
                  Lengkapi setiap baris dengan angka yang valid. Jumlah barang
                  maksimal 3 desimal, biaya 4 desimal, dan saldo akun 2 desimal.
                  Nilai stok per barang harus lebih dari nol; isi hanya satu
                  sisi setiap akun.
                </p>
              )}
              <div className="opening-actions">
                <p>
                  {dirty
                    ? "Ada perubahan yang belum disimpan. Simpan draf sebelum pemeriksaan akhir."
                    : document
                      ? `Draf versi ${document.version} tersimpan. Draf belum mengubah stok atau jurnal.`
                      : "Siapkan dan simpan draf terlebih dahulu. Draf boleh belum seimbang."}
                </p>
                <div>
                  <button
                    type="submit"
                    className="button secondary"
                    disabled={stale || (!dirty && !!document)}
                  >
                    <Save size={16} />
                    {busy ? "Menyimpan…" : "Simpan draf"}
                  </button>
                  <button
                    type="button"
                    className="button primary"
                    disabled={
                      !document ||
                      dirty ||
                      !preview.balanced ||
                      busy ||
                      stale ||
                      periodClosed ||
                      period.loading ||
                      !!period.error
                    }
                    onClick={() => {
                      setError("");
                      setModal("review");
                    }}
                  >
                    <Scale size={16} /> Periksa & posting
                  </button>
                </div>
              </div>
            </fieldset>
          </form>
        )
      ) : document ? (
        <Snapshot snapshot={document} />
      ) : state.eligible ? (
        <EmptyState
          title="Belum ada draf saldo awal"
          description="Administrator atau keuangan dapat menyiapkan posisi awal usaha di sini sebelum transaksi pertama."
        />
      ) : null}

      {!!document?.history.length && (
        <section className="opening-history">
          <div className="opening-section-heading">
            <div>
              <h3>
                <History size={18} /> Riwayat posting dan pembalikan
              </h3>
              <p>
                Angka pada setiap posting tetap tersimpan, termasuk setelah
                koreksi.
              </p>
            </div>
          </div>
          {document.history.map((entry, index) => (
            <details key={entry.id}>
              <summary>
                <span>
                  <strong>Posting {index + 1}</strong>
                  <small>
                    {dateTime(entry.posted_at)} · {entry.description}
                  </small>
                </span>
                <span
                  className={`opening-history-status ${entry.reversed_at ? "reversed" : ""}`}
                >
                  {entry.reversed_at ? "Sudah dibalik" : "Aktif"}
                </span>
              </summary>
              <div className="opening-history-body">
                {entry.reversed_at && (
                  <div className="opening-notice">
                    <RotateCcw size={18} />
                    <div>
                      <strong>
                        Dibalik pada {dateTime(entry.reversed_at)}
                      </strong>
                      <p>Alasan: {entry.reason}</p>
                      <p>
                        Jurnal pembalik meniadakan nilai stok dan saldo akun
                        pada posting ini.
                      </p>
                    </div>
                  </div>
                )}
                <Snapshot snapshot={entry} />
              </div>
            </details>
          ))}
        </section>
      )}

      {modal === "reload" && (
        <Modal
          title="Muat versi server?"
          description="Isian lokal yang belum tersimpan akan diganti dengan draf terakhir dari server."
          onClose={() => {
            if (!busy) setModal(null);
          }}
        >
          <div className="form-body">
            <p className="opening-help">
              Periksa atau salin isian Anda sebelum melanjutkan. Pembukuan yang
              sudah diposting tidak berubah karena tindakan ini.
            </p>
          </div>
          <div className="form-actions">
            <button
              type="button"
              className="button secondary"
              onClick={() => setModal(null)}
            >
              Kembali ke isian
            </button>
            <button
              type="button"
              className="button primary"
              onClick={() => void load()}
            >
              Muat versi server
            </button>
          </div>
        </Modal>
      )}
      {modal === "review" && document && (
        <Modal
          title="Periksa saldo awal"
          description={`Posting draf versi ${document.version} setelah seluruh nilai benar.`}
          onClose={() => {
            if (!busy) setModal(null);
          }}
          wide
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void perform("post");
            }}
          >
            <div className="form-body">
              <p className="opening-help">{document.description}</p>
              <Snapshot snapshot={document} />
              <div className="info-box">
                <CircleAlert size={18} />
                <p>
                  Posting akan mencatat stok awal dan jurnal pembukaan secara
                  bersamaan. Setelah ada transaksi lain, saldo awal tidak dapat
                  dibalik melalui halaman ini.
                </p>
              </div>
              <label className="checkbox-field">
                <input type="checkbox" required disabled={busy} />
                Saya sudah memeriksa jumlah stok, biaya, dan saldo semua akun.
                Debit dan kredit sudah sesuai posisi usaha.
              </label>
              <FormError error={error} />
            </div>
            <div className="form-actions">
              <button
                type="button"
                className="button secondary"
                disabled={busy}
                onClick={() => setModal(null)}
              >
                Kembali ke draf
              </button>
              <button
                type="submit"
                className="button primary"
                disabled={busy || stale}
              >
                {busy ? "Memposting…" : "Posting saldo awal"}
              </button>
            </div>
          </form>
        </Modal>
      )}
      {modal === "reverse" && document && (
        <Modal
          title="Balikkan saldo awal untuk koreksi"
          description="Stok dan saldo akun dari posting awal akan diniadakan. Riwayat tetap tersimpan dan draf dapat diperbaiki."
          onClose={() => {
            if (!busy) setModal(null);
          }}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              const reason = String(
                new FormData(event.currentTarget).get("reason") ?? "",
              );
              void perform("reverse", reason);
            }}
          >
            <div className="form-body">
              <div className="info-box">
                <RotateCcw size={18} />
                <p>
                  Pembalikan hanya diizinkan selama belum ada transaksi atau
                  pergerakan stok lain setelah saldo awal.
                </p>
              </div>
              <FormField label="Alasan koreksi *">
                <textarea
                  name="reason"
                  rows={3}
                  required
                  minLength={3}
                  maxLength={1000}
                  autoFocus
                  disabled={busy}
                  placeholder="Contoh: Biaya stok awal perlu dikoreksi"
                />
              </FormField>
              <label className="checkbox-field">
                <input type="checkbox" required disabled={busy} />
                Saya memahami stok dan jurnal awal akan dibalik sebelum draf
                diperbaiki.
              </label>
              <FormError error={error} />
            </div>
            <div className="form-actions">
              <button
                type="button"
                className="button secondary"
                disabled={busy}
                onClick={() => setModal(null)}
              >
                Batal
              </button>
              <button
                type="submit"
                className="button primary"
                disabled={busy || stale}
              >
                {busy ? "Membalikkan…" : "Balikkan saldo awal"}
              </button>
            </div>
          </form>
        </Modal>
      )}
    </div>
  );
}

function Totals({
  debit,
  credit,
  difference,
  balanced,
}: {
  debit: string;
  credit: string;
  difference?: string;
  balanced: boolean;
}) {
  return (
    <div className={`opening-totals ${balanced ? "balanced" : ""}`}>
      <div>
        <small>Total debit</small>
        <strong>{money(debit)}</strong>
      </div>
      <div>
        <small>Total kredit</small>
        <strong>{money(credit)}</strong>
      </div>
      <div>
        {balanced ? <CheckCheck size={18} /> : <Scale size={18} />}
        <span>
          {balanced ? "Seimbang" : `Selisih ${money(difference ?? "0")}`}
        </span>
      </div>
    </div>
  );
}
function Snapshot({ snapshot }: { snapshot: OpeningSnapshot }) {
  return (
    <div className="opening-snapshot">
      {!!snapshot.items.length && (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>Barang</th>
                <th className="align-right">Jumlah</th>
                <th className="align-right">Biaya / unit</th>
                <th className="align-right">Nilai stok</th>
              </tr>
            </thead>
            <tbody>
              {snapshot.items.map((item) => (
                <tr key={item.product_id}>
                  <td>
                    <strong>{item.name}</strong>
                    <small className="opening-table-note">{item.sku}</small>
                  </td>
                  <td className="align-right">
                    {number(item.quantity)} {item.unit}
                  </td>
                  <td className="align-right">{costMoney(item.unit_cost)}</td>
                  <td className="align-right">{money(item.value)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {!!snapshot.lines.length && (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>Akun</th>
                <th className="align-right">Debit</th>
                <th className="align-right">Kredit</th>
              </tr>
            </thead>
            <tbody>
              {snapshot.lines.map((line, index) => (
                <tr key={`${line.account_code}-${index}`}>
                  <td>
                    <span className="mono muted">{line.account_code}</span>{" "}
                    {line.account_name}
                    {line.account_code === "1300" && (
                      <small className="opening-table-note">
                        Dihitung otomatis dari stok awal
                      </small>
                    )}
                  </td>
                  <td className="align-right">{money(line.debit)}</td>
                  <td className="align-right">{money(line.credit)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <Totals
        debit={snapshot.total_debit}
        credit={snapshot.total_credit}
        difference={openingDifference(
          snapshot.total_debit,
          snapshot.total_credit,
        )}
        balanced={openingIsBalanced(
          snapshot.total_debit,
          snapshot.total_credit,
        )}
      />
    </div>
  );
}
