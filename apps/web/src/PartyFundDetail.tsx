import { useEffect, useRef, useState, type FormEvent } from "react";
import { CreditCard, HandCoins, RotateCcw } from "lucide-react";
import { api, ApiError, errorMessage } from "./api";
import {
  EmptyState,
  ErrorState,
  FormError,
  FormField,
  Loading,
  Modal,
} from "./components";
import { CashAccountSelect } from "./CashAccountSelect";
import { AttachmentsPanel } from "./AttachmentsPanel";
import { exactMoney, moneyCents } from "./dashboard-charts";
import { dateTime } from "./format";
import { useResource } from "./hooks";
import { JournalPreview } from "./JournalPreview";
import { requestIdentity, type RequestIdentity } from "./purchasing-model";
import {
  allocationPreview,
  fundKindNames,
  positivePartyAmount,
  partyFundActionInput,
  targetHref,
  type PartyAllocation,
  type PartyFund,
  type PartyRefund,
  type PartyRefundRequest,
  type PartySide,
  type PartyTarget,
} from "./party-balances-model";
import { openingPartyNames, type OpeningParty } from "./opening-parties";
import { centsAmount } from "./returns-model";

type FundAction =
  | { type: "allocate"; target_id?: string }
  | { type: "request" | "refund" | "reverse" }
  | { type: "allocation_reverse"; item: PartyAllocation }
  | { type: "request_cancel" | "request_pay"; item: PartyRefundRequest }
  | { type: "refund_reverse"; item: PartyRefund };

export function PartyFundDetail({
  id,
  canManage,
  onClose,
  onChanged,
  initialTargetId,
}: {
  id: string;
  canManage: boolean;
  onClose: () => void;
  onChanged: () => void;
  initialTargetId?: string;
}) {
  const resource = useResource<PartyFund>(`/party-funds/${id}`);
  const fund = resource.data;
  const [action, setAction] = useState<{
    fund: PartyFund;
    action: FundAction;
  } | null>(null);
  const [journal, setJournal] = useState<string | null>(null);
  const [openingId, setOpeningId] = useState<string | null>(null);
  const prompted = useRef(false);
  useEffect(() => {
    if (
      fund &&
      initialTargetId &&
      !prompted.current &&
      canManage &&
      !fund.reversed
    ) {
      prompted.current = true;
      setAction({
        fund,
        action: { type: "allocate", target_id: initialTargetId },
      });
    }
  }, [fund, initialTargetId, canManage]);
  const available = !!fund && !fund.reversed && moneyCents(fund.available) > 0n;
  function saved() {
    setAction(null);
    resource.reload();
    onChanged();
  }
  return (
    <Modal
      wide
      title={fund?.number ?? "Rincian saldo dana"}
      description="Sumber, pemakaian, dan perpindahan uang tetap tersimpan."
      onClose={onClose}
    >
      {resource.loading ? (
        <Loading />
      ) : resource.error ? (
        <ErrorState message={resource.error} onRetry={resource.reload} />
      ) : (
        fund && (
          <>
            <div className="form-body">
              <div className="party-fund-title">
                <div>
                  <span className="eyebrow">
                    {fund.side === "customer"
                      ? "DANA PELANGGAN"
                      : "DANA PADA SUPPLIER"}
                  </span>
                  <h3>{fund.party_name}</h3>
                  <a
                    className="text-button"
                    href={`?side=${fund.side}&party=${fund.party_id}#balances`}
                  >
                    Buka buku pembantu pihak
                  </a>
                  <p>
                    {fundKindNames[fund.kind] ?? fund.kind} ·{" "}
                    {dateTime(fund.created_at)} ·{" "}
                    {fund.reversed ? "Sudah dibalik" : "Aktif"}
                  </p>
                </div>
                <button
                  className="text-button"
                  onClick={() => setJournal(fund.journal_id)}
                >
                  Jurnal sumber
                </button>
              </div>
              {fund.reference && <p>Referensi: {fund.reference}</p>}
              {fund.description && (
                <p className="purchase-prewrap">{fund.description}</p>
              )}
              <div className="party-metrics">
                <div>
                  <span>Nilai asal</span>
                  <strong>{exactMoney(fund.amount)}</strong>
                </div>
                <div>
                  <span>Sudah dialokasikan</span>
                  <strong>{exactMoney(fund.allocated)}</strong>
                </div>
                <div>
                  <span>Refund aktual</span>
                  <strong>{exactMoney(fund.refunded)}</strong>
                </div>
                <div>
                  <span>Disisihkan refund</span>
                  <strong>{exactMoney(fund.reserved)}</strong>
                </div>
                <div>
                  <span>Tersedia</span>
                  <strong>{exactMoney(fund.available)}</strong>
                </div>
              </div>
              <p className="party-help">
                Tersedia = nilai asal − alokasi − refund aktual − dana yang
                disisihkan. Pemakaian saldo tidak memindahkan kas.{" "}
                {fund.kind !== "cash_advance" &&
                  "Kredit ini berasal dari retur atau saldo awal; pembalikan sumber dilakukan melalui dokumen asal setelah semua pemakaian dan refund dibereskan."}
              </p>
              {fund.source_url && (
                <a className="text-button" href={fund.source_url}>
                  Lihat dokumen asal
                </a>
              )}
              {canManage && available && (
                <div className="data-file-actions">
                  <button
                    className="button primary"
                    onClick={() =>
                      setAction({ fund, action: { type: "allocate" } })
                    }
                  >
                    <HandCoins size={16} /> Alokasikan ke tagihan
                  </button>
                  <button
                    className="button secondary"
                    onClick={() =>
                      setAction({ fund, action: { type: "request" } })
                    }
                  >
                    Sisihkan untuk refund
                  </button>
                  <button
                    className="button secondary"
                    onClick={() =>
                      setAction({ fund, action: { type: "refund" } })
                    }
                  >
                    <CreditCard size={16} /> Catat refund aktual
                  </button>
                </div>
              )}
            </div>
            <AttachmentsPanel
              documentType="party_fund"
              documentId={fund.id}
              title="Bukti sumber dana"
            />
            <section className="party-section form-body">
              <h3>Pemakaian saldo</h3>
              {fund.allocations.length ? (
                <div className="table-scroll">
                  <table>
                    <thead>
                      <tr>
                        <th>Tagihan / waktu</th>
                        <th className="align-right">Nominal</th>
                        <th>Status dan sumber</th>
                      </tr>
                    </thead>
                    <tbody>
                      {fund.allocations.map((item) => (
                        <tr key={item.id}>
                          <td>
                            {targetHref({
                              target_type: item.target_type,
                              id: item.target_id,
                            }) ? (
                              <a
                                className="text-button"
                                href={targetHref({
                                  target_type: item.target_type,
                                  id: item.target_id,
                                })!}
                              >
                                {item.number}
                              </a>
                            ) : (
                              <button
                                className="text-button"
                                onClick={() => setOpeningId(item.target_id)}
                              >
                                {item.number}
                              </button>
                            )}
                            <small className="block muted">
                              {dateTime(item.created_at)}
                            </small>
                          </td>
                          <td className="align-right">
                            {exactMoney(item.amount)}
                          </td>
                          <td>
                            <button
                              className="text-button"
                              onClick={() => setJournal(item.journal_id)}
                            >
                              Jurnal alokasi
                            </button>
                            {item.reversed ? (
                              <>
                                <p className="muted">
                                  Dibalik: {item.reversal?.reason}
                                </p>
                                {item.reversal && (
                                  <button
                                    className="text-button"
                                    onClick={() =>
                                      setJournal(item.reversal!.journal_id)
                                    }
                                  >
                                    Jurnal pembalikan
                                  </button>
                                )}
                              </>
                            ) : (
                              canManage &&
                              !fund.reversed && (
                                <button
                                  className="text-button danger-text"
                                  onClick={() =>
                                    setAction({
                                      fund,
                                      action: {
                                        type: "allocation_reverse",
                                        item,
                                      },
                                    })
                                  }
                                >
                                  Balikkan alokasi
                                </button>
                              )
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              ) : (
                <p className="muted">
                  Belum ada saldo yang dipakai untuk tagihan.
                </p>
              )}
            </section>
            <section className="party-section form-body">
              <h3>Permintaan refund</h3>
              <p className="muted">
                Permintaan hanya menyisihkan saldo. Kas berubah setelah refund
                aktual dicatat.
              </p>
              {fund.refund_requests.length ? (
                fund.refund_requests.map((item) => (
                  <article className="party-refund-request" key={item.id}>
                    <div className="data-file-section-head">
                      <div>
                        <strong>{dateTime(item.created_at)}</strong>
                        <p>{item.reason}</p>
                      </div>
                      <span>
                        {item.cancelled
                          ? "Dibatalkan"
                          : moneyCents(item.remaining) === 0n
                            ? "Selesai"
                            : "Menunggu refund"}
                      </span>
                    </div>
                    <div className="party-inline-summary">
                      <span>
                        Diminta <strong>{exactMoney(item.amount)}</strong>
                      </span>
                      <span>
                        Sudah berpindah <strong>{exactMoney(item.paid)}</strong>
                      </span>
                      <span>
                        Sisa disisihkan{" "}
                        <strong>{exactMoney(item.remaining)}</strong>
                      </span>
                    </div>
                    {canManage &&
                      !fund.reversed &&
                      !item.cancelled &&
                      moneyCents(item.remaining) > 0n && (
                        <div className="data-file-actions">
                          <button
                            className="button secondary"
                            onClick={() =>
                              setAction({
                                fund,
                                action: { type: "request_pay", item },
                              })
                            }
                          >
                            Catat refund sebagian / penuh
                          </button>
                          <button
                            className="text-button danger-text"
                            onClick={() =>
                              setAction({
                                fund,
                                action: { type: "request_cancel", item },
                              })
                            }
                          >
                            Batalkan sisa permintaan
                          </button>
                        </div>
                      )}
                  </article>
                ))
              ) : (
                <p className="muted">Belum ada permintaan refund.</p>
              )}
            </section>
            <section className="party-section form-body">
              <h3>Riwayat refund aktual</h3>
              {fund.refunds.length ? (
                fund.refunds.map((item) => (
                  <article className="party-refund-request" key={item.id}>
                    <div className="data-file-section-head">
                      <strong>
                        {exactMoney(item.amount)} ·{" "}
                        {fund.side === "customer"
                          ? "Kas keluar ke pelanggan"
                          : "Kas masuk dari supplier"}
                      </strong>
                      <span>{item.reversed ? "Dibalik" : "Tercatat"}</span>
                    </div>
                    <p>
                      {dateTime(item.created_at)} ·{" "}
                      {item.reference || "Tanpa referensi"}
                    </p>
                    <p className="purchase-prewrap">{item.reason}</p>
                    <div className="data-file-actions">
                      <button
                        className="text-button"
                        onClick={() => setJournal(item.journal_id)}
                      >
                        Jurnal refund
                      </button>
                      {item.reversal ? (
                        <>
                          <span>Dibalik: {item.reversal.reason}</span>
                          <button
                            className="text-button"
                            onClick={() =>
                              setJournal(item.reversal!.journal_id)
                            }
                          >
                            Jurnal pembalikan
                          </button>
                        </>
                      ) : (
                        canManage &&
                        !fund.reversed && (
                          <button
                            className="text-button danger-text"
                            onClick={() =>
                              setAction({
                                fund,
                                action: { type: "refund_reverse", item },
                              })
                            }
                          >
                            Koreksi refund
                          </button>
                        )
                      )}
                    </div>
                    <AttachmentsPanel
                      documentType="party_refund"
                      documentId={item.id}
                      title="Bukti refund"
                    />
                  </article>
                ))
              ) : (
                <p className="muted">Belum ada perpindahan kas untuk refund.</p>
              )}
            </section>
            {canManage && fund.kind === "cash_advance" && !fund.reversed && (
              <div className="form-body">
                <button
                  className="text-button danger-text"
                  disabled={
                    moneyCents(fund.allocated) > 0n ||
                    moneyCents(fund.refunded) > 0n ||
                    moneyCents(fund.reserved) > 0n
                  }
                  onClick={() =>
                    setAction({ fund, action: { type: "reverse" } })
                  }
                >
                  <RotateCcw size={15} /> Balikkan uang muka
                </button>
                <p className="muted">
                  Selesaikan pembalikan alokasi dan refund serta batalkan
                  permintaan aktif sebelum membalik sumber dana.
                </p>
              </div>
            )}
          </>
        )
      )}
      <div className="form-actions">
        <button className="button secondary" onClick={onClose}>
          Tutup
        </button>
      </div>
      {action &&
        (action.action.type === "allocate" ? (
          <AllocationForm
            fund={action.fund}
            initialTargetId={action.action.target_id}
            onClose={() => setAction(null)}
            onSaved={saved}
          />
        ) : (
          <FundActionForm
            fund={action.fund}
            action={action.action}
            onClose={() => setAction(null)}
            onSaved={saved}
            onRefresh={() => {
              setAction(null);
              resource.reload();
            }}
          />
        ))}
      {journal && (
        <JournalPreview id={journal} onClose={() => setJournal(null)} />
      )}
      {openingId && (
        <OpeningItemDetail
          id={openingId}
          canManage={canManage}
          onClose={() => setOpeningId(null)}
          onChanged={saved}
        />
      )}
    </Modal>
  );
}

export function PartyFundingForm({
  side,
  party,
  onClose,
  onSaved,
}: {
  side: PartySide;
  party: { id: string; name: string };
  onClose: () => void;
  onSaved: (fund: PartyFund) => void;
}) {
  const orders = useResource<{
    items: {
      id: string;
      number: string;
      customer_id?: string;
      supplier_id?: string;
    }[];
  }>(side === "customer" ? "/orders" : "/purchase-orders");
  const choices = (orders.data?.items ?? []).filter(
    (order) =>
      (side === "customer" ? order.customer_id : order.supplier_id) ===
      party.id,
  );
  const [amount, setAmount] = useState("");
  const [cashAccount, setCashAccount] = useState("1100");
  const [reference, setReference] = useState("");
  const [description, setDescription] = useState("");
  const [orderId, setOrderId] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [review, setReview] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busyRef.current) return;
    if (!positivePartyAmount(amount) || !confirmed) {
      setError(
        "Isi nominal positif maksimal Rp1 triliun dan konfirmasikan perpindahan uang sebenarnya.",
      );
      return;
    }
    if (!review) {
      setReview(true);
      setError("");
      return;
    }
    const payload = {
      side,
      party_id: party.id,
      amount,
      cash_account_code: cashAccount,
      reference: reference.trim() || null,
      description: description.trim() || null,
      order_id: orderId || null,
    };
    identity.current = requestIdentity(identity.current, payload);
    busyRef.current = true;
    setBusy(true);
    setError("");
    try {
      const fund = await api<PartyFund>("/party-funds", {
        method: "POST",
        body: { ...payload, idempotency_key: identity.current.key },
      });
      onSaved(fund);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  return (
    <Modal
      title={
        side === "customer"
          ? "Catat uang muka pelanggan"
          : "Catat uang muka ke supplier"
      }
      description={party.name}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <p>
            {side === "customer"
              ? "Kas bertambah. Dana dicatat sebagai kewajiban kepada pelanggan sampai dialokasikan atau dikembalikan."
              : "Kas berkurang. Dana menjadi uang muka pada supplier sampai dialokasikan atau dikembalikan."}{" "}
            Pencatatan ini belum menjadi pendapatan, beban, atau stok.
          </p>
          <p className="party-help">
            Jika uang melebihi satu tagihan, catat seluruh dana di sini lalu
            alokasikan bagian pelunasannya. Sisanya tetap menjadi saldo pihak,
            tanpa menyamarkannya sebagai pembayaran tagihan.
          </p>
          <CashAccountSelect value={cashAccount} onChange={setCashAccount} disabled={busy || review} label={side === "customer" ? "Rekening penerimaan uang muka" : "Rekening pembayaran uang muka"} />
          <FormField label="Dana yang benar-benar berpindah (Rp) *">
            <input
              required
              inputMode="decimal"
              value={amount}
              disabled={busy || review}
              onChange={(e) => setAmount(e.target.value)}
            />
          </FormField>
          <FormField label="Referensi bukti">
            <input
              maxLength={200}
              value={reference}
              disabled={busy || review}
              onChange={(e) => setReference(e.target.value)}
            />
          </FormField>
          <FormField label="Pesanan sumber (opsional)">
            <select
              value={orderId}
              disabled={busy || review || orders.loading}
              onChange={(e) => setOrderId(e.target.value)}
            >
              <option value="">Tanpa pesanan sumber</option>
              {choices.map((order) => (
                <option key={order.id} value={order.id}>
                  {order.number}
                </option>
              ))}
            </select>
          </FormField>
          {orders.error && (
            <p className="muted">
              Pesanan belum berhasil dimuat. Dana tetap dapat dicatat tanpa
              pesanan sumber.
            </p>
          )}
          <FormField label="Keterangan">
            <textarea
              maxLength={1000}
              rows={3}
              value={description}
              disabled={busy || review}
              onChange={(e) => setDescription(e.target.value)}
            />
          </FormField>
          <label className="checkbox-field">
            <input
              type="checkbox"
              checked={confirmed}
              disabled={busy || review}
              onChange={(e) => setConfirmed(e.target.checked)}
            />{" "}
            Saya memastikan{" "}
            {side === "customer"
              ? "uang pelanggan sudah diterima"
              : "uang sudah dibayarkan ke supplier"}
            .
          </label>
          {review && (
            <div className="info-box">
              <HandCoins size={18} />
              <p>
                {side === "customer" ? "Kas masuk" : "Kas keluar"}{" "}
                {exactMoney(amount)} · {party.name}. Seluruh dana tersedia untuk
                alokasi atau refund setelah disimpan.
              </p>
            </div>
          )}
          <FormError error={error} />
        </div>
        <div className="form-actions">
          <button
            type="button"
            className="button secondary"
            disabled={busy}
            onClick={() => (review ? setReview(false) : onClose())}
          >
            {review ? "Ubah rincian" : "Batal"}
          </button>
          <button className="button primary" disabled={busy}>
            {busy
              ? "Menyimpan…"
              : review
                ? "Catat dana aktual"
                : "Tinjau uang muka"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function AllocationForm({
  fund,
  initialTargetId,
  onClose,
  onSaved,
}: {
  fund: PartyFund;
  initialTargetId?: string;
  onClose: () => void;
  onSaved: () => void;
}) {
  const resource = useResource<{ items: PartyTarget[] }>(
    `/party-targets?${new URLSearchParams({ side: fund.side, party_id: fund.party_id })}`,
  );
  const targets = (resource.data?.items ?? []).filter(
    (target) => moneyCents(target.outstanding) > 0n,
  );
  const [amounts, setAmounts] = useState<Record<string, string>>({});
  const [review, setReview] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  const preview = allocationPreview(fund, targets, amounts);
  const prepared = useRef(false);
  useEffect(() => {
    if (initialTargetId && resource.data && !prepared.current) {
      prepared.current = true;
      const target = resource.data.items.find(
        (item) => item.id === initialTargetId,
      );
      if (target)
        setAmounts({
          [`${target.target_type}:${target.id}`]: centsAmount(
            moneyCents(fund.available) < moneyCents(target.outstanding)
              ? moneyCents(fund.available)
              : moneyCents(target.outstanding),
          ),
        });
    }
  }, [initialTargetId, resource.data, fund.available]);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busyRef.current) return;
    if (preview.error) {
      setError(preview.error);
      return;
    }
    if (!review) {
      setReview(true);
      setError("");
      return;
    }
    const payload = { version: fund.version, items: preview.items };
    identity.current = requestIdentity(identity.current, {
      fund_id: fund.id,
      ...payload,
    });
    busyRef.current = true;
    setBusy(true);
    setError("");
    try {
      await api(`/party-funds/${fund.id}/allocate`, {
        method: "POST",
        body: { ...payload, idempotency_key: identity.current.key },
      });
      onSaved();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  return (
    <Modal
      wide
      title="Alokasikan saldo ke tagihan"
      description={`${fund.number} · ${fund.party_name}`}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <p>
            Saldo tersedia {exactMoney(fund.available)}. Pilih satu atau
            beberapa tagihan pihak yang sama. Alokasi tidak mencatat kas kedua
            kali.
          </p>
          {initialTargetId && (
            <p className="info-box">
              Dana aktual sudah tercatat. Periksa alokasi ke tagihan awal
              berikut ini. Menutup tinjauan menyisakan dana di saldo pihak dan
              tidak membatalkan perpindahan kas yang sudah dicatat.
            </p>
          )}
          {resource.loading ? (
            <Loading />
          ) : resource.error ? (
            <ErrorState message={resource.error} onRetry={resource.reload} />
          ) : targets.length ? (
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Tagihan</th>
                    <th>Jatuh tempo</th>
                    <th className="align-right">Sisa</th>
                    <th>Pakai saldo (Rp)</th>
                  </tr>
                </thead>
                <tbody>
                  {targets.map((target) => {
                    const key = `${target.target_type}:${target.id}`;
                    return (
                      <tr key={key}>
                        <td>
                          {target.number}
                          <small className="block muted">
                            {target.target_type.startsWith("opening")
                              ? "Tagihan pembukaan"
                              : target.target_type.startsWith("historical_") ? "Tagihan historis" : "Tagihan operasional"}
                          </small>
                        </td>
                        <td>{target.due_date ?? "Belum diketahui"}</td>
                        <td className="align-right">
                          {exactMoney(target.outstanding)}
                        </td>
                        <td>
                          <input
                            aria-label={`Alokasi ${target.number}`}
                            inputMode="decimal"
                            value={amounts[key] ?? ""}
                            disabled={busy || review}
                            onChange={(e) =>
                              setAmounts((previous) => ({
                                ...previous,
                                [key]: e.target.value,
                              }))
                            }
                            placeholder="0.00"
                          />
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          ) : (
            <EmptyState
              title="Tidak ada sisa tagihan"
              description="Saldo tetap tersedia sampai ada tagihan pihak ini atau refund dicatat."
            />
          )}
          <div className="party-inline-summary">
            <span>
              Dipakai <strong>{exactMoney(preview.total)}</strong>
            </span>
            <span>
              Sisa tersedia{" "}
              <strong>
                {exactMoney(moneyCents(fund.available) - preview.total)}
              </strong>
            </span>
            <span>
              Kas berubah <strong>Rp0</strong>
            </span>
          </div>
          {review && (
            <p className="info-box">
              Periksa {preview.items.length} tagihan dengan total alokasi{" "}
              {exactMoney(preview.total)}. Saldo sumber dan sisa tagihan
              diperiksa lagi saat posting.
            </p>
          )}
          <FormError error={error} />
        </div>
        <div className="form-actions">
          <button
            type="button"
            className="button secondary"
            disabled={busy}
            onClick={() => (review ? setReview(false) : onClose())}
          >
            {review ? "Ubah alokasi" : "Batal"}
          </button>
          <button
            className="button primary"
            disabled={
              busy || resource.loading || !!resource.error || !targets.length
            }
          >
            {busy
              ? "Memproses…"
              : review
                ? "Posting alokasi tanpa kas"
                : "Tinjau alokasi"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function FundActionForm({
  fund,
  action,
  onClose,
  onSaved,
  onRefresh,
}: {
  fund: PartyFund;
  action: Exclude<FundAction, { type: "allocate" }>;
  onClose: () => void;
  onSaved: () => void;
  onRefresh: () => void;
}) {
  const [amount, setAmount] = useState(
    action.type === "request_pay" ? action.item.remaining : fund.available,
  );
  const [reason, setReason] = useState("");
  const [reference, setReference] = useState("");
  const [cashAccount, setCashAccount] = useState("1100");
  const newCash = action.type === "refund" || action.type === "request_pay";
  const [confirmed, setConfirmed] = useState(false);
  const [review, setReview] = useState(false);
  const [error, setError] = useState("");
  const [conflict, setConflict] = useState(false);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const identity = useRef<RequestIdentity | null>(null);
  const hasAmount =
    action.type === "request" ||
    action.type === "refund" ||
    action.type === "request_pay";
  const cash =
    action.type === "refund" ||
    action.type === "request_pay" ||
    action.type === "refund_reverse" ||
    action.type === "reverse";
  const maximum =
    action.type === "request_pay" ? action.item.remaining : fund.available;
  const title =
    action.type === "request"
      ? "Sisihkan saldo untuk refund"
      : action.type === "refund" || action.type === "request_pay"
        ? "Catat refund aktual"
        : action.type === "allocation_reverse"
          ? "Balikkan pemakaian saldo"
          : action.type === "request_cancel"
            ? "Batalkan sisa permintaan refund"
            : action.type === "refund_reverse"
              ? "Koreksi refund aktual"
              : "Balikkan uang muka";
  const actualAmount = hasAmount
    ? amount
    : action.type === "refund_reverse" || action.type === "allocation_reverse"
      ? action.item.amount
      : action.type === "request_cancel"
        ? action.item.remaining
        : fund.amount;
  const outgoing =
    action.type === "refund_reverse"
      ? fund.side === "supplier"
      : action.type === "reverse"
        ? fund.side === "customer"
        : fund.side === "customer";
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busyRef.current) return;
    if (
      reason.trim().length < 3 ||
      (hasAmount && !positivePartyAmount(amount, maximum)) ||
      (cash && !confirmed)
    ) {
      setError(
        "Periksa nominal, alasan minimal tiga karakter, dan konfirmasi perpindahan kas bila diperlukan.",
      );
      return;
    }
    if (!review) {
      setReview(true);
      setError("");
      return;
    }
    const path =
      action.type === "request"
        ? `/party-funds/${fund.id}/refund-requests`
        : action.type === "refund"
          ? `/party-funds/${fund.id}/refunds`
          : action.type === "request_pay"
            ? `/party-refund-requests/${action.item.id}/pay`
            : action.type === "request_cancel"
              ? `/party-refund-requests/${action.item.id}/cancel`
              : action.type === "allocation_reverse"
                ? `/party-allocations/${action.item.id}/reverse`
                : action.type === "refund_reverse"
                  ? `/party-refunds/${action.item.id}/reverse`
                  : `/party-funds/${fund.id}/reverse`;
    const payload = partyFundActionInput(
      action.type,
      fund.version,
      reason,
      amount,
      reference,
      cashAccount,
    );
    identity.current = requestIdentity(identity.current, { path, ...payload });
    busyRef.current = true;
    setBusy(true);
    setError("");
    try {
      await api(path, {
        method: "POST",
        body: { ...payload, idempotency_key: identity.current.key },
      });
      onSaved();
    } catch (e) {
      setError(errorMessage(e));
      setConflict(e instanceof ApiError && e.status === 409);
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  return (
    <Modal
      title={title}
      description={`${fund.number} · ${fund.party_name}`}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <p>
            {action.type === "request"
              ? "Dana disisihkan sehingga tidak dapat dipakai untuk alokasi lain. Permintaan ini belum mengubah kas."
              : action.type === "request_cancel"
                ? "Hanya sisa yang belum dibayar dilepaskan kembali ke saldo tersedia. Riwayat refund aktual tetap tersimpan; kas tidak berubah."
                : action.type === "allocation_reverse"
                  ? "Saldo sumber dan sisa tagihan dipulihkan. Pembalikan ini tidak memindahkan kas."
                  : cash
                    ? `${outgoing ? "Kas berkurang" : "Kas bertambah"} sesuai perpindahan uang sebenarnya. Gunakan hanya setelah dana benar-benar ${outgoing ? "dibayarkan" : "diterima"}.`
                    : "Pembalikan mempertahankan jurnal dan riwayat sumber."}
          </p>
          {newCash && <CashAccountSelect value={cashAccount} onChange={setCashAccount} disabled={busy || review} label={fund.side === "customer" ? "Rekening pembayaran refund" : "Rekening penerimaan refund"} />}
          {cash && !newCash && <p className="muted">Pembalikan memakai rekening pada jurnal asli.</p>}
          {hasAmount ? (
            <FormField
              label="Nominal (Rp) *"
              hint={`Maksimal ${exactMoney(maximum)}. Sebagian atau seluruhnya dapat diproses.`}
            >
              <input
                required
                inputMode="decimal"
                value={amount}
                disabled={busy || review}
                onChange={(e) => setAmount(e.target.value)}
              />
            </FormField>
          ) : (
            <p>
              <strong>{exactMoney(actualAmount)}</strong>
            </p>
          )}
          {(action.type === "refund" || action.type === "request_pay") && (
            <FormField label="Referensi bukti">
              <input
                maxLength={200}
                value={reference}
                disabled={busy || review}
                onChange={(e) => setReference(e.target.value)}
              />
            </FormField>
          )}
          <FormField label="Alasan / keterangan *">
            <textarea
              required
              minLength={3}
              maxLength={1000}
              rows={3}
              value={reason}
              disabled={busy || review}
              onChange={(e) => setReason(e.target.value)}
            />
          </FormField>
          {cash && (
            <label className="checkbox-field">
              <input
                type="checkbox"
                checked={confirmed}
                disabled={busy || review}
                onChange={(e) => setConfirmed(e.target.checked)}
              />{" "}
              Saya memastikan dana{" "}
              {outgoing ? "sudah dibayarkan" : "sudah diterima"} sesuai nominal
              tindakan ini.
            </label>
          )}
          {review && (
            <div className="info-box">
              <HandCoins size={18} />
              <p>
                {title}: {exactMoney(actualAmount)}.{" "}
                {cash
                  ? `${outgoing ? "Kas keluar" : "Kas masuk"} aktual ${newCash ? `pada akun ${cashAccount}` : "pada rekening jurnal asal"}.`
                  : "Kas tidak berubah."}{" "}
                Alasan: {reason}
              </p>
            </div>
          )}
          <FormError error={error} />
          {conflict && (
            <button
              type="button"
              className="text-button"
              disabled={busy}
              onClick={onRefresh}
            >
              Tutup tinjauan dan muat saldo terbaru
            </button>
          )}
        </div>
        <div className="form-actions">
          <button
            type="button"
            className="button secondary"
            disabled={busy}
            onClick={() => (review ? setReview(false) : onClose())}
          >
            {review ? "Ubah rincian" : "Batal"}
          </button>
          <button className="button primary" disabled={busy}>
            {busy ? "Memproses…" : review ? title : "Tinjau perubahan"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

interface OpeningItem extends OpeningParty {
  id: string;
  side?: PartySide;
  party_name: string;
  posting_id: string;
  journal_id?: string;
  paid?: string;
  outstanding: string;
  version: number;
  allocations?: (PartyAllocation & { fund_id: string })[];
  credit_fund_id?: string | null;
  status?: string;
}
export function OpeningItemDetail({
  id,
  canManage,
  onClose,
  onChanged,
}: {
  id: string;
  canManage: boolean;
  onClose: () => void;
  onChanged: () => void;
}) {
  const resource = useResource<OpeningItem>(`/opening-items/${id}`);
  const item = resource.data;
  const [creating, setCreating] = useState(false);
  const [fundId, setFundId] = useState<string | null>(null);
  const [journal, setJournal] = useState<string | null>(null);
  const [preferredTarget, setPreferredTarget] = useState<string | undefined>();
  const side =
    item?.side ??
    (item?.kind === "receivable" || item?.kind === "customer_credit"
      ? "customer"
      : "supplier");
  return (
    <Modal
      wide
      title={item?.reference ?? "Rincian saldo awal pihak"}
      description="Tagihan atau kredit lama yang dibawa ke pembukaan Haruflow."
      onClose={onClose}
    >
      {resource.loading ? (
        <Loading />
      ) : resource.error ? (
        <ErrorState message={resource.error} onRetry={resource.reload} />
      ) : (
        item && (
          <>
            <div className="form-body">
              <h3>{item.party_name}</h3>
              <p>
                {openingPartyNames[item.kind]} · Tanggal dokumen{" "}
                {item.document_date} · Jatuh tempo{" "}
                {item.due_date ?? "Belum diketahui"}
              </p>
              <div className="party-inline-summary">
                <span>
                  Saldo awal <strong>{exactMoney(item.amount)}</strong>
                </span>
                <span>
                  Sudah diselesaikan{" "}
                  <strong>{exactMoney(item.paid ?? "0")}</strong>
                </span>
                <span>
                  Sisa tagihan <strong>{exactMoney(item.outstanding)}</strong>
                </span>
              </div>
              <p className="party-help">
                Saldo ini diposting melalui pembukaan, tanpa mencatat ulang
                penjualan, stok, atau kas historis. Pembayaran baru dicatat
                sebagai dana aktual kemudian dialokasikan ke tagihan awal.
              </p>
              {item.journal_id && (
                <button
                  className="text-button"
                  onClick={() => setJournal(item.journal_id!)}
                >
                  Jurnal pembukaan
                </button>
              )}
              {item.credit_fund_id && (
                <button
                  className="button secondary"
                  onClick={() => {
                    setPreferredTarget(undefined);
                    setFundId(item.credit_fund_id!);
                  }}
                >
                  Lihat kredit dan penyelesaiannya
                </button>
              )}
              {canManage &&
                item.status !== "void" &&
                moneyCents(item.outstanding) > 0n &&
                (item.kind === "receivable" || item.kind === "payable") && (
                  <button
                    className="button primary"
                    onClick={() => setCreating(true)}
                  >
                    {side === "customer"
                      ? "Catat dana pembayaran pelanggan"
                      : "Catat dana pembayaran ke supplier"}
                  </button>
                )}
            </div>
            <AttachmentsPanel
              documentType="opening_item"
              documentId={item.id}
              title="Bukti tagihan/saldo lama"
            />
            <div className="form-body">
              <h3>Riwayat penyelesaian saldo</h3>
              {item.allocations?.length ? (
                <div className="table-scroll">
                  <table>
                    <thead>
                      <tr>
                        <th>Waktu</th>
                        <th className="align-right">Nominal</th>
                        <th>Status</th>
                        <th />
                      </tr>
                    </thead>
                    <tbody>
                      {item.allocations.map((allocation) => (
                        <tr key={allocation.id}>
                          <td>{dateTime(allocation.created_at)}</td>
                          <td className="align-right">
                            {exactMoney(allocation.amount)}
                          </td>
                          <td>
                            {allocation.reversed || allocation.reversal
                              ? "Dibalik"
                              : "Aktif"}
                          </td>
                          <td>
                            <button
                              className="text-button"
                              onClick={() => setFundId(allocation.fund_id)}
                            >
                              Sumber dana dan koreksi
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              ) : (
                <p className="muted">Belum ada alokasi dana pada saldo ini.</p>
              )}
            </div>
          </>
        )
      )}
      <div className="form-actions">
        <button className="button secondary" onClick={onClose}>
          Tutup
        </button>
      </div>
      {creating && item && (
        <PartyFundingForm
          side={side}
          party={{ id: item.party_id, name: item.party_name }}
          onClose={() => setCreating(false)}
          onSaved={(fund) => {
            setCreating(false);
            setPreferredTarget(id);
            setFundId(fund.id);
            onChanged();
          }}
        />
      )}
      {fundId && (
        <PartyFundDetail
          id={fundId}
          initialTargetId={preferredTarget}
          canManage={canManage}
          onClose={() => {
            setFundId(null);
            resource.reload();
          }}
          onChanged={() => {
            resource.reload();
            onChanged();
          }}
        />
      )}
      {journal && (
        <JournalPreview id={journal} onClose={() => setJournal(null)} />
      )}
    </Modal>
  );
}
