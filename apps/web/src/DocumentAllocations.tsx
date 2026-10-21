import { dateTime } from "./format";
import { exactMoney } from "./dashboard-charts";
import type { PartyReversal } from "./party-balances-model";

export interface DocumentAllocation {
  id: string;
  fund_id: string;
  fund_number: string;
  amount: string;
  created_at: string;
  journal_id: string;
  reversed: boolean;
  reversal: PartyReversal | null;
}

export function DocumentAllocations({
  items,
  onJournal,
}: {
  items?: DocumentAllocation[];
  onJournal: (id: string) => void;
}) {
  if (!items?.length) return null;
  return (
    <section className="form-body party-section">
      <h3>Riwayat pemakaian saldo pihak</h3>
      <p className="muted">
        Pemakaian uang muka atau kredit menyelesaikan tagihan tanpa perpindahan
        kas baru. Koreksi dapat ditelusuri melalui sumber dana.
      </p>
      <div className="table-scroll">
        <table>
          <thead>
            <tr>
              <th>Sumber dana / tanggal</th>
              <th className="align-right">Nominal</th>
              <th>Status</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {items.map((item) => (
              <tr key={item.id}>
                <td>
                  <a
                    className="text-button"
                    href={`?fund=${item.fund_id}#balances`}
                  >
                    {item.fund_number}
                  </a>
                  <small className="block muted">
                    {dateTime(item.created_at)}
                  </small>
                </td>
                <td className="align-right">{exactMoney(item.amount)}</td>
                <td>
                  {item.reversed
                    ? `Dibalik: ${item.reversal?.reason ?? "Koreksi tercatat"}`
                    : "Aktif"}
                </td>
                <td>
                  <button
                    type="button"
                    className="text-button"
                    onClick={() => onJournal(item.journal_id)}
                  >
                    Jurnal alokasi
                  </button>
                  {item.reversal?.journal_id && (
                    <button
                      type="button"
                      className="text-button"
                      onClick={() => onJournal(item.reversal!.journal_id)}
                    >
                      Jurnal pembalikan
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
