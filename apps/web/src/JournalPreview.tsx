import { ErrorState, Loading, Modal } from "./components";
import { dateTime } from "./format";
import { exactMoney, moneyCents } from "./dashboard-charts";
import { useResource } from "./hooks";
import type { Journal } from "./types";

export function JournalPreview({
  id,
  onClose,
}: {
  id: string;
  onClose: () => void;
}) {
  const resource = useResource<Journal>(`/accounting/journals/${id}`);
  const journal = resource.data;
  return (
    <Modal
      wide
      title="Rincian jurnal"
      description="Posting tersimpan dari dokumen penerimaan."
      onClose={onClose}
    >
      {resource.loading ? (
        <Loading />
      ) : resource.error ? (
        <ErrorState message={resource.error} onRetry={resource.reload} />
      ) : (
        journal && (
          <>
            <div className="form-body">
              <strong>{journal.description}</strong>
              <p>{dateTime(journal.created_at)}</p>
            </div>
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
                  {journal.lines.map((line, index) => (
                    <tr key={`${line.account_code}-${index}`}>
                      <td>
                        <span className="mono muted">{line.account_code}</span>{" "}
                        {line.account_name}
                      </td>
                      <td className="align-right">{exactMoney(line.debit)}</td>
                      <td className="align-right">{exactMoney(line.credit)}</td>
                    </tr>
                  ))}
                </tbody>
                <tfoot>
                  <tr>
                    <th>Total</th>
                    <th className="align-right">
                      {exactMoney(
                        journal.lines.reduce(
                          (sum, line) => sum + moneyCents(line.debit),
                          0n,
                        ),
                      )}
                    </th>
                    <th className="align-right">
                      {exactMoney(
                        journal.lines.reduce(
                          (sum, line) => sum + moneyCents(line.credit),
                          0n,
                        ),
                      )}
                    </th>
                  </tr>
                </tfoot>
              </table>
            </div>
            <div className="form-body">
              <p className="muted">
                Koreksi jurnal otomatis dilakukan melalui penerimaan asalnya.
              </p>
            </div>
          </>
        )
      )}
      <div className="form-actions">
        <button className="button secondary" onClick={onClose}>
          Kembali ke penerimaan
        </button>
      </div>
    </Modal>
  );
}
