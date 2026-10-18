import { useState, type FormEvent } from "react";
import { CircleAlert } from "lucide-react";
import { api, errorMessage } from "./api";
import { FormError, Modal } from "./components";

export function DeleteMaster({
  path,
  name,
  onClose,
  onSaved,
}: {
  path: string;
  name: string;
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
      await api(path, { method: "DELETE", body: {} });
      onSaved();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title="Hapus data"
      description={`Hapus ${name} dari ruang kerja?`}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <div className="form-error">
            <CircleAlert size={18} />
            <p>
              Data yang dihapus tidak dapat dipulihkan. Data yang sudah
              digunakan dalam transaksi tidak dapat dihapus, agar riwayat bisnis
              tetap utuh.
            </p>
          </div>
          <FormError error={error} />
        </div>
        <div className="form-actions">
          <button
            className="button secondary"
            type="button"
            disabled={busy}
            onClick={onClose}
          >
            Batal
          </button>
          <button className="button danger" disabled={busy}>
            {busy ? "Menghapus…" : "Hapus data"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
