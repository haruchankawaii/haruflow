import { useState, type FormEvent } from "react";
import { BookOpen, Download, Filter, Scale } from "lucide-react";
import { api, errorMessage } from "./api";
import {
  EmptyState,
  ErrorState,
  FormActions,
  FormError,
  FormField,
  Loading,
  Modal,
  Pagination,
} from "./components";
import { downloadCsv } from "./export";
import { date, money } from "./format";
import { usePagination, useResource } from "./hooks";
import type { Account } from "./types";

interface LedgerEntry {
  journal_id: string;
  description: string;
  created_at: string;
  source_type: string;
  debit: string;
  credit: string;
  balance: string;
}
interface LedgerData {
  account: Account;
  items: LedgerEntry[];
  opening_balance: string;
  closing_balance: string;
  truncated: boolean;
}

export function AccountForm({
  onClose,
  onSaved,
}: {
  onClose: () => void;
  onSaved: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api("/accounting/accounts", {
        method: "POST",
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
      title="Tambah akun"
      description="Buat akun pembukuan sesuai kebutuhan operasional Anda."
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <FormField
            label="Kode akun *"
            hint="Gunakan 3–10 digit angka yang belum digunakan."
          >
            <input
              name="code"
              required
              pattern="[0-9]{3,10}"
              inputMode="numeric"
              minLength={3}
              maxLength={10}
              autoFocus
              placeholder="Contoh: 5100"
            />
          </FormField>
          <FormField label="Nama akun *">
            <input
              name="name"
              required
              maxLength={150}
              placeholder="Contoh: Beban internet"
            />
          </FormField>
          <FormField label="Kelompok akun *">
            <select name="kind" required defaultValue="expense">
              <option value="asset">Aset</option>
              <option value="liability">Liabilitas</option>
              <option value="equity">Ekuitas</option>
              <option value="revenue">Pendapatan</option>
              <option value="expense">Beban</option>
            </select>
          </FormField>
          <div className="info-box">
            <Scale size={18} />
            <p>
              Akun baru dapat digunakan untuk jurnal manual. Gunakan kelompok
              yang sesuai dengan sifat transaksinya.
            </p>
          </div>
          <FormError error={error} />
        </div>
        <FormActions onClose={onClose} busy={busy} label="Simpan akun" />
      </form>
    </Modal>
  );
}

export function Ledger({
  account,
  onClose,
}: {
  account: Account;
  onClose: () => void;
}) {
  const [filter, setFilter] = useState({ from: "", to: "" });
  const [filterError, setFilterError] = useState("");
  const query = new URLSearchParams(
    Object.entries(filter).filter(([, value]) => value),
  ).toString();
  const { data, loading, error, reload } = useResource<LedgerData>(
    `/accounting/accounts/${account.code}/ledger${query ? `?${query}` : ""}`,
  );
  const pagination = usePagination(data?.items ?? [], query);
  function apply(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const fields = new FormData(event.currentTarget);
    const from = String(fields.get("from"));
    const to = String(fields.get("to"));
    if (from && to && from > to) {
      setFilterError(
        "Tanggal awal harus sebelum atau sama dengan tanggal akhir.",
      );
      return;
    }
    setFilterError("");
    setFilter({ from, to });
  }
  return (
    <Modal
      title={`Buku besar · ${account.code}`}
      description={account.name}
      onClose={onClose}
      wide
    >
      <div className="form-body">
        <form className="ledger-filter" onSubmit={apply}>
          <FormField label="Dari tanggal">
            <input name="from" type="date" />
          </FormField>
          <FormField label="Sampai tanggal">
            <input name="to" type="date" />
          </FormField>
          <button className="button secondary" type="submit">
            <Filter size={15} /> Terapkan
          </button>
        </form>
        <FormError error={filterError} />
        {loading ? (
          <Loading />
        ) : error || !data ? (
          <ErrorState message={error} onRetry={reload} />
        ) : (
          <>
            <div className="ledger-balances">
              <div>
                <small>Saldo awal (debit − kredit)</small>
                <strong>{money(data.opening_balance)}</strong>
              </div>
              <div>
                <small>Saldo akhir (debit − kredit)</small>
                <strong>{money(data.closing_balance)}</strong>
              </div>
            </div>
            {data.items.length ? (
              <div className="table-scroll detail-table">
                <table>
                  <thead>
                    <tr>
                      <th>Tanggal</th>
                      <th>Keterangan</th>
                      <th className="align-right">Debit</th>
                      <th className="align-right">Kredit</th>
                      <th className="align-right">Saldo</th>
                    </tr>
                  </thead>
                  <tbody>
                    {pagination.items.map((entry, index) => (
                      <tr key={`${entry.journal_id}-${index}`}>
                        <td className="muted">{date(entry.created_at)}</td>
                        <td>
                          <strong>{entry.description}</strong>
                          <small className="block muted">
                            {entry.source_type}
                          </small>
                        </td>
                        <td className="align-right">{money(entry.debit)}</td>
                        <td className="align-right">{money(entry.credit)}</td>
                        <td className="align-right font-medium">
                          {money(entry.balance)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <EmptyState
                title="Belum ada transaksi pada akun ini"
                description="Pilih rentang tanggal lain atau posting jurnal pertama."
              />
            )}
            <Pagination {...pagination} />
            {data.truncated && (
              <div className="info-box">
                <BookOpen size={18} />
                <p>
                  Tampilan dibatasi 5.000 baris. Persempit tanggal untuk melihat
                  seluruh rincian. Saldo akhir mencakup seluruh transaksi pada
                  rentang ini.
                </p>
              </div>
            )}
          </>
        )}
      </div>
      <div className="form-actions">
        <button className="button secondary" onClick={onClose}>
          Tutup
        </button>
        <button
          className="button primary"
          disabled={!data || loading}
          onClick={() =>
            data &&
            downloadCsv(
              `haruflow-buku-besar-${account.code}`,
              ["Tanggal", "Keterangan", "Sumber", "Debit", "Kredit", "Saldo"],
              data.items.map((item) => [
                item.created_at,
                item.description,
                item.source_type,
                item.debit,
                item.credit,
                item.balance,
              ]),
            )
          }
        >
          <Download size={16} /> Ekspor CSV
        </button>
      </div>
    </Modal>
  );
}
