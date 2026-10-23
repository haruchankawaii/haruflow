import { useEffect, useRef, useState, type FormEvent } from "react";
import { ArrowRight, Check, FileText, Plus, RefreshCw, Undo2 } from "lucide-react";
import { api, ApiError, errorMessage } from "./api";
import { EmptyState, ErrorState, FormError, FormField, Loading, Modal, Pagination, SearchInput } from "./components";
import { exactMoney } from "./dashboard-charts";
import { dateTime } from "./format";
import { usePagination, useResource } from "./hooks";
import { JournalPreview } from "./JournalPreview";
import { exactDecimal, requestIdentity, type RequestIdentity } from "./purchasing-model";
import { landedCostInput, retainedPercent, type LandedCostAllocation, type LandedCostDocument, type LandedCostDraft, type LandedCostOptions, type LandedCostPreview } from "./landed-costs-model";
import type { Role } from "./types";
import "./landed-costs.css";

const emptyDraft = (): LandedCostDraft => ({ source_line_id: "", amount: "", allocation_method: "value", receipt_line_ids: [], reference: "", reason: "" });

export function LandedCosts({ role, revision, onSaved }: { role: Role; revision: number; onSaved: () => void }) {
  const costs = useResource<{ items: LandedCostDocument[] }>("/landed-costs", revision);
  const [creating, setCreating] = useState(false);
  const [selected, setSelected] = useState<string | null>(() => {
    const id = new URLSearchParams(window.location.search).get("landed_cost");
    return id && /^[0-9a-f-]{36}$/i.test(id) ? id : null;
  });
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("");
  const canManage = ["admin", "finance"].includes(role);
  const items = (costs.data?.items ?? []).filter((c) => (!status || c.status === status) && `${c.number} ${c.source.number} ${c.reference ?? ""} ${c.reason}`.toLowerCase().includes(search.toLowerCase()));
  const pagination = usePagination(items, `${search}|${status}`);
  useEffect(() => {
    const url = new URL(window.location.href);
    if (url.searchParams.has("landed_cost")) { url.searchParams.delete("landed_cost"); window.history.replaceState(null, "", url); }
  }, []);
  return <>
    <section className="panel landed-workspace">
      <div className="table-toolbar"><div><strong>Biaya perolehan</strong><p className="muted">Alokasikan biaya pembelian yang sudah dicatat ke stok dan HPP.</p></div><div className="landed-actions"><button className="text-button" onClick={costs.reload} aria-label="Muat ulang biaya perolehan"><RefreshCw size={16} /></button>{canManage && <button className="button primary" onClick={() => setCreating(true)}><Plus size={16} /> Alokasikan biaya</button>}</div></div>
      <div className="landed-help">Ongkir, selisih harga positif, atau jasa final yang memenuhi syarat dapat dialokasikan. Bagian barang yang sudah keluar menjadi HPP; bagian yang masih ada menambah nilai persediaan. Tinjau hasil sebelum membukukan.</div>
      <div className="purchase-filters"><SearchInput value={search} onChange={setSearch} placeholder="Cari nomor alokasi, sumber, atau referensi…" /><label className="purchase-filter">Status<select value={status} onChange={(e) => setStatus(e.target.value)}><option value="">Semua status</option><option value="posted">Dibukukan</option><option value="reversed">Dibalik</option></select></label></div>
      {costs.loading ? <Loading /> : costs.error ? <ErrorState message={costs.error} onRetry={costs.reload} /> : !items.length ? <EmptyState title="Belum ada alokasi biaya yang cocok" description="Pilih biaya pembelian yang sudah dibukukan dan barang penerima biayanya." /> : <><div className="table-scroll"><table><thead><tr><th>Dokumen</th><th>Sumber biaya</th><th>Status</th><th className="align-right">Persediaan</th><th className="align-right">HPP</th><th className="align-right">Total</th><th /></tr></thead><tbody>{pagination.items.map((c) => <tr key={c.id}><td><button className="table-link" onClick={() => setSelected(c.id)}>{c.number}</button><small className="landed-cell-note">{dateTime(c.created_at)}</small></td><td>{c.source.number}<small className="landed-cell-note">{c.source.account_name}</small></td><td><span className={`status status-${c.status === "posted" ? "confirmed" : "cancelled"}`}><span />{c.status === "posted" ? "Dibukukan" : "Dibalik"}</span></td><td className="align-right">{exactMoney(c.inventory_amount)}</td><td className="align-right">{exactMoney(c.expense_amount)}</td><td className="align-right font-medium">{exactMoney(c.amount)}</td><td><button className="text-button" onClick={() => setSelected(c.id)}>Detail <ArrowRight size={14} /></button></td></tr>)}</tbody></table></div><Pagination {...pagination} /></>}
      <p className="landed-footnote">Menampilkan hingga 200 dokumen terbaru. Dokumen sumber dan posting lama tetap tersimpan.</p>
    </section>
    {creating && <LandedCostForm revision={revision} onClose={() => setCreating(false)} onCreated={(doc) => { setCreating(false); setSelected(doc.id); onSaved(); }} />}
    {selected && <LandedCostDetail id={selected} revision={revision} canManage={canManage} onClose={() => setSelected(null)} onSaved={onSaved} />}
  </>;
}

function CostTotals({ amount, inventory_amount, expense_amount }: Pick<LandedCostPreview, "amount" | "inventory_amount" | "expense_amount">) {
  return <div className="landed-totals"><div><span>Tambahan persediaan</span><strong>{exactMoney(inventory_amount)}</strong></div><div><span>Bagian HPP</span><strong>{exactMoney(expense_amount)}</strong></div><div><span>Biaya dialokasikan</span><strong>{exactMoney(amount)}</strong></div></div>;
}
function AllocationTable({ items }: { items: LandedCostAllocation[] }) {
  return <div className="table-scroll"><table><thead><tr><th>Barang / penerimaan</th><th className="align-right">Bobot</th><th className="align-right">Biaya tersisa</th><th className="align-right">Alokasi</th><th className="align-right">Persediaan</th><th className="align-right">HPP</th></tr></thead><tbody>{items.map((a) => <tr key={a.receipt_line_id}><td><strong>{a.product_name}</strong><small className="landed-cell-note">{a.product_sku} · {a.receipt_number} · {exactDecimal(a.quantity)} {a.unit}</small><small className="landed-cell-note">Nilai stok: {exactMoney(a.before_value)} → {exactMoney(a.after_value)}</small></td><td className="align-right">{exactDecimal(a.weight)}</td><td className="align-right">{retainedPercent(a.remaining_ratio)}</td><td className="align-right">{exactMoney(a.allocated)}</td><td className="align-right">{exactMoney(a.inventory_amount)}</td><td className="align-right">{exactMoney(a.expense_amount)}</td></tr>)}</tbody></table></div>;
}

function LandedCostForm({ revision, onClose, onCreated }: { revision: number; onClose: () => void; onCreated: (doc: LandedCostDocument) => void }) {
  const options = useResource<LandedCostOptions>("/landed-costs/options", revision);
  const [draft, setDraft] = useState(emptyDraft);
  const [review, setReview] = useState<LandedCostPreview | null>(null);
  const [reviewedInput, setReviewedInput] = useState<ReturnType<typeof landedCostInput> | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [search, setSearch] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  const [journal, setJournal] = useState<string | null>(null);
  function resetReview() { setReview(null); setReviewedInput(null); setConfirmed(false); }
  function update(patch: Partial<LandedCostDraft>) { setDraft((d) => ({ ...d, ...patch })); resetReview(); setError(""); }
  useEffect(() => { resetReview(); }, [options.data]);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busyRef.current || !options.data) return;
    busyRef.current = true; setBusy(true); setError("");
    try {
      if (review && reviewedInput) {
        if (!confirmed) throw new Error("Konfirmasikan pembagian biaya dan batas pembalikan sebelum membukukan.");
        const input = { ...reviewedInput, expected_preview: review.preview_token };
        identity.current = requestIdentity(identity.current, input);
        const doc = await api<LandedCostDocument>("/landed-costs", { method: "POST", body: { ...input, idempotency_key: identity.current.key } });
        onCreated(doc);
      } else {
        const input = landedCostInput(draft, options.data);
        identity.current = requestIdentity(identity.current, input);
        const preview = await api<LandedCostPreview>("/landed-costs/preview", { method: "POST", body: { ...input, idempotency_key: identity.current.key } });
        setReview(preview); setReviewedInput(input); setConfirmed(false);
      }
    } catch (cause) {
      setError(errorMessage(cause));
      if (cause instanceof ApiError && cause.status === 409) { resetReview(); options.reload(); }
    } finally { busyRef.current = false; setBusy(false); }
  }
  const selectedSource = options.data?.sources.find((s) => s.line_id === draft.source_line_id);
  const selectedProducts = new Set((options.data?.targets ?? []).filter((t) => draft.receipt_line_ids.includes(t.receipt_line_id)).map((t) => t.product_id));
  const targets = (options.data?.targets ?? []).filter((t) => `${t.product_name} ${t.product_sku} ${t.receipt_number}`.toLowerCase().includes(search.toLowerCase()));
  const unavailable = (options.data?.sources ?? []).filter((s) => !s.eligible);
  return <Modal wide title="Alokasikan biaya perolehan" description="Pindahkan biaya pembelian yang sudah dibukukan ke persediaan dan HPP sesuai bukti." onClose={() => { if (!busyRef.current) onClose(); }}>
    {options.loading ? <Loading /> : options.error ? <ErrorState message={options.error} onRetry={options.reload} /> : options.data && <form onSubmit={submit}>
      <fieldset disabled={busy} className="landed-fieldset"><div className="form-body">
        <div className="landed-help">Kas, utang, pajak masukan, dan dokumen lama tetap mengikuti posting asal. Biaya yang sudah keluar dialihkan ke HPP periode berjalan. Retur, penyesuaian, atau pembalikan setelah penerimaan membatasi tujuan yang dapat dipilih.</div>
        <FormField label="Sumber biaya yang sudah dibukukan" hint={selectedSource?.available ? `Sisa yang dapat dialokasikan: ${exactMoney(selectedSource.available)}` : "Pilih ongkir, selisih harga positif, atau jasa yang sudah ditagihkan final."}><select value={draft.source_line_id} onChange={(e) => update({ source_line_id: e.target.value })} required><option value="">Pilih sumber biaya…</option>{options.data.sources.filter((s) => s.eligible).map((s) => <option key={s.line_id} value={s.line_id}>{s.number} · {s.account_name} · {exactMoney(s.available ?? "0")}</option>)}</select></FormField>
        {selectedSource && <button type="button" className="text-button" onClick={() => setJournal(selectedSource.journal_id)}><FileText size={14} /> Lihat jurnal sumber</button>}
        {!!unavailable.length && <details className="landed-restrictions"><summary>{unavailable.length} sumber belum memenuhi syarat</summary><ul>{unavailable.map((s) => <li key={s.line_id}><strong>{s.number} · {s.account_name}</strong><span>{s.blocked_reason}</span></li>)}</ul></details>}
        <div className="landed-form-grid"><FormField label="Nominal yang dialokasikan"><input inputMode="decimal" placeholder="100000.00" value={draft.amount} onChange={(e) => update({ amount: e.target.value })} required /></FormField><FormField label="Dasar pembagian"><select value={draft.allocation_method} onChange={(e) => update({ allocation_method: e.target.value as "value" | "quantity" })}><option value="value">Nilai asli penerimaan</option><option value="quantity">Kuantitas satuan dasar yang sama</option></select></FormField></div>
        <div className="landed-target-title"><div><strong>Barang yang menerima biaya</strong><p className="muted">{draft.receipt_line_ids.length}/100 dipilih · satu penerimaan untuk setiap produk.</p></div><SearchInput value={search} onChange={setSearch} placeholder="Cari barang atau penerimaan…" /></div>
        <div className="landed-targets">{targets.length ? targets.map((t) => {
          const checked = draft.receipt_line_ids.includes(t.receipt_line_id);
          const duplicate = !checked && selectedProducts.has(t.product_id);
          return <label className={`landed-target ${!t.eligible || duplicate ? "unavailable" : ""}`} key={t.receipt_line_id}><input type="checkbox" checked={checked} disabled={!t.eligible || duplicate || (!checked && draft.receipt_line_ids.length >= 100)} onChange={(e) => update({ receipt_line_ids: e.target.checked ? [...draft.receipt_line_ids, t.receipt_line_id] : draft.receipt_line_ids.filter((id) => id !== t.receipt_line_id) })} /><span><strong>{t.product_name}</strong><small>{t.product_sku} · {t.receipt_number} · {exactDecimal(t.quantity)} {t.unit} · {exactMoney(t.receipt_value)}</small>{(!t.eligible || duplicate) && <small className="landed-blocked">{t.blocked_reason || "Produk ini sudah dipilih dari penerimaan lain."}</small>}</span></label>;
        }) : <p className="muted">Tidak ada barang penerimaan yang cocok.</p>}</div>
        <small className="muted">Pilihan memuat hingga 500 sumber biaya dan 1.000 baris penerimaan terbaru.</small>
        <FormField label="Referensi bukti (opsional)"><input maxLength={200} value={draft.reference} onChange={(e) => update({ reference: e.target.value })} /></FormField>
        <FormField label="Alasan dan dasar alokasi"><textarea minLength={3} maxLength={1000} rows={3} value={draft.reason} onChange={(e) => update({ reason: e.target.value })} required /></FormField>
      </div>
      {review && <section className="landed-review" aria-live="polite"><h3>Tinjau pembagian biaya</h3><CostTotals {...review} /><AllocationTable items={review.items} /><div className="landed-help"><ul>{review.warnings.map((w) => <li key={w}>{w}</li>)}</ul></div><label className="landed-confirm"><input type="checkbox" checked={confirmed} onChange={(e) => setConfirmed(e.target.checked)} />Saya telah memeriksa sumber, pembagian persediaan/HPP, dan batas pembalikannya.</label></section>}
      <div className="form-body"><FormError error={error} /></div><div className="form-actions"><button type="button" className="button secondary" onClick={onClose}>Batal</button>{review && <button type="button" className="button secondary" onClick={resetReview}>Tinjau ulang</button>}<button type="submit" className="button primary" disabled={busy || (!!review && !confirmed)}><Check size={16} />{busy ? "Memproses…" : review ? "Bukukan biaya perolehan" : "Tinjau alokasi"}</button></div></fieldset>
    </form>}
    {journal && <JournalPreview id={journal} onClose={() => setJournal(null)} />}
  </Modal>;
}

function LandedCostDetail({ id, revision, canManage, onClose, onSaved }: { id: string; revision: number; canManage: boolean; onClose: () => void; onSaved: () => void }) {
  const resource = useResource<LandedCostDocument>(`/landed-costs/${id}`, revision);
  const [reversing, setReversing] = useState(false);
  const [reason, setReason] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  const [error, setError] = useState("");
  const [journal, setJournal] = useState<string | null>(null);
  const cost = resource.data;
  useEffect(() => { setReversing(false); setReason(""); setConfirmed(false); }, [cost?.version]);
  async function reverse(event: FormEvent) {
    event.preventDefault();
    if (busyRef.current || !cost?.can_reverse || !confirmed) return;
    if (reason.trim().length < 3 || reason.trim().length > 1000) { setError("Alasan harus berisi 3–1.000 karakter."); return; }
    busyRef.current = true; setBusy(true); setError("");
    const input = { version: cost.version, reason: reason.trim() };
    identity.current = requestIdentity(identity.current, { id, ...input });
    try {
      await api(`/landed-costs/${id}/reverse`, { method: "POST", body: { ...input, idempotency_key: identity.current.key } });
      setReversing(false); resource.reload(); onSaved();
    } catch (cause) { setError(errorMessage(cause)); if (cause instanceof ApiError && cause.status === 409) { resource.reload(); setReversing(false); } }
    finally { busyRef.current = false; setBusy(false); }
  }
  return <Modal wide title={cost?.number ?? "Rincian biaya perolehan"} description="Pembagian biaya dan jejak koreksi tersimpan bersama dokumen asal." onClose={() => { if (!busyRef.current) onClose(); }}>
    {resource.loading ? <Loading /> : resource.error ? <ErrorState message={resource.error} onRetry={resource.reload} /> : cost && <>
      <div className="form-body"><div className="landed-detail-heading"><div><strong>{cost.status === "posted" ? "Dibukukan" : "Dibalik"}</strong><p className="muted">{dateTime(cost.created_at)} · versi {cost.version}</p></div><button className="text-button" onClick={() => setJournal(cost.journal_id)}><FileText size={15} /> Lihat jurnal</button></div>
        <div className="landed-meta"><div><span>Sumber biaya</span><strong>{cost.source.number}</strong><small>{cost.source.account_code} · {cost.source.account_name}</small><button className="text-button" onClick={() => setJournal(cost.source.journal_id)}>Jurnal sumber</button></div><div><span>Dasar pembagian</span><strong>{cost.allocation_method === "value" ? "Nilai asli penerimaan" : "Kuantitas satuan dasar"}</strong><small>{cost.reference ?? "Tanpa referensi tambahan"}</small></div></div>
        <p className="landed-reason">{cost.reason}</p><CostTotals {...cost} />
      </div><AllocationTable items={cost.items} />
      {!!cost.location_events.length && <details className="landed-location-history"><summary>Jejak penilaian lokasi ({cost.location_events.length})</summary><div className="table-scroll"><table><thead><tr><th>Waktu</th><th>Tahap</th><th>Gudang / lot</th><th className="align-right">Perubahan nilai</th></tr></thead><tbody>{cost.location_events.map((e) => <tr key={e.id}><td>{dateTime(e.created_at)}</td><td>{e.phase === "post" ? "Pembukuan" : "Pembalikan"}</td><td><small className="mono">{e.warehouse_id}</small>{e.lot_id && <small className="landed-cell-note mono">{e.lot_id}</small>}</td><td className="align-right">{exactMoney(e.value)}</td></tr>)}</tbody></table></div></details>}
      <div className="form-body"><FormError error={error} />{cost.reversal ? <div className="landed-help"><strong>Dibalik pada {dateTime(cost.reversal.created_at)}</strong><p>{cost.reversal.reason}</p><button className="text-button" onClick={() => setJournal(cost.reversal!.journal_id)}>Lihat jurnal pembalikan</button></div> : !cost.can_reverse && <div className="landed-help">{cost.reversal_blocked_reason}</div>}
        {canManage && cost.can_reverse && !reversing && <button className="button secondary" onClick={() => { setReversing(true); setError(""); }}><Undo2 size={16} /> Balikkan alokasi</button>}
        {canManage && reversing && <form onSubmit={reverse} className="landed-reverse"><p>Jurnal lawan akan mengembalikan nilai stok dan akun biaya sumber sesuai snapshot. Posting asli tetap tersimpan.</p><FormField label="Alasan pembalikan"><textarea value={reason} onChange={(e) => { setReason(e.target.value); setConfirmed(false); }} minLength={3} maxLength={1000} rows={3} required disabled={busy} /></FormField><label className="landed-confirm"><input type="checkbox" checked={confirmed} disabled={busy} onChange={(e) => setConfirmed(e.target.checked)} />Saya menyetujui pembalikan {exactMoney(cost.amount)} sesuai rincian dokumen ini.</label><div className="landed-actions"><button className="button secondary" type="button" disabled={busy} onClick={() => setReversing(false)}>Batal</button><button className="button danger" type="submit" disabled={busy || !confirmed}>{busy ? "Memproses…" : "Bukukan pembalikan"}</button></div></form>}
      </div>
    </>}
    {journal && <JournalPreview id={journal} onClose={() => setJournal(null)} />}
  </Modal>;
}
