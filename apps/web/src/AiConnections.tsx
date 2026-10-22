import { useState, type FormEvent } from "react";
import { Copy, KeyRound, Plug, Plus, Shield, Unplug } from "lucide-react";
import { api, errorMessage } from "./api";
import {
  EmptyState,
  ErrorState,
  FormActions,
  FormError,
  FormField,
  Loading,
  Modal,
} from "./components";
import { dateTime, roleNames } from "./format";
import { useResource } from "./hooks";
import type { User } from "./types";
import {
  aiConnectionStatus,
  usableAiScopes,
  type AiAccess,
  type AiAccessResponse,
  type AiConnection,
  type AiScope,
  type AiUser,
} from "./ai-connections-model";
import "./ai-connections.css";

const codexConfiguration = String.raw`[mcp_servers.haruflow]
command = 'C:\Haruflow\haruflow.exe'
args = ["mcp"]

[mcp_servers.haruflow.env]
HARUFLOW_MCP_URL = "http://127.0.0.1:8080"
HARUFLOW_MCP_TOKEN_FILE = 'C:\Users\NAMA_ANDA\.haruflow\codex-token.txt'`;

export default function AiConnections({ user }: { user: User }) {
  const { data, loading, error, reload } =
    useResource<AiAccessResponse>("/ai/access");
  const [create, setCreate] = useState(false);
  const [grant, setGrant] = useState<AiUser | null>(null);
  const [revoke, setRevoke] = useState<AiConnection | null>(null);
  const [secret, setSecret] = useState("");
  const [notice, setNotice] = useState("");
  const labels = new Map(
    data?.scope_catalog.map((scope) => [scope.id, scope.label]),
  );
  const scopeNames = (scopes: AiScope[]) =>
    scopes.map((scope) => labels.get(scope) ?? scope).join(", ") || "Belum ada";
  const scopes = data ? usableAiScopes(data.access) : [];

  return (
    <div className="settings-body ai-settings">
      <div className="settings-section-heading">
        <div className="settings-big-icon">
          <Plug size={25} />
        </div>
        <div>
          <h2>Integrasi AI melalui MCP</h2>
          <p>Tanyakan kondisi usaha di Codex dengan data sesuai izin Anda.</p>
        </div>
      </div>
      <div className="ai-flow" aria-label="Alur integrasi">
        <span>Chat di Codex</span>
        <span aria-hidden="true">→</span>
        <span>MCP Haruflow</span>
        <span aria-hidden="true">→</span>
        <span>Laporan sesuai izin</span>
      </div>
      <div className="info-box">
        <Shield size={18} />
        <p>
          Koneksi ini hanya membaca data. Codex menyusun jawaban; chat AI belum
          tersedia di Haruflow. Data yang dibaca dapat dikirim ke penyedia AI
          yang digunakan Codex. Gunakan setelah mendapat persetujuan pemilik
          usaha dan berikan akses sesuai tanggung jawab pengguna.
        </p>
      </div>
      {notice && (
        <p className="ai-notice" role="status">
          {notice}
        </p>
      )}
      {loading ? (
        <Loading label="Memuat akses integrasi AI…" />
      ) : error ? (
        <ErrorState message={error} onRetry={reload} />
      ) : (
        data && (
          <>
            <section
              className="ai-section"
              aria-labelledby="ai-personal-heading"
            >
              <div className="ai-section-title">
                <div>
                  <h3 id="ai-personal-heading">Koneksi saya</h3>
                  <p className="muted">
                    {scopes.length
                      ? `Data yang boleh dibaca: ${scopeNames(scopes)}.`
                      : "Akses AI belum diberikan. Hubungi administrator yang ditunjuk pemilik usaha."}
                  </p>
                </div>
                <button
                  className="button primary"
                  disabled={!scopes.length}
                  onClick={() => {
                    setNotice("");
                    setCreate(true);
                  }}
                >
                  <Plus size={16} /> Buat kunci akses
                </button>
              </div>
              <p className="muted ai-description">
                Gunakan kunci pribadi untuk setiap perangkat. Maksimal 5 kunci
                aktif per akun, berlaku hingga 90 hari. Koneksi memakai akun
                Haruflow yang sudah ada dan tidak menambah slot pengguna.
              </p>
              {data.tokens.length ? (
                <div className="table-scroll">
                  <table className="ai-connections-table">
                    <thead>
                      <tr>
                        <th>Koneksi</th>
                        <th>Akses data</th>
                        <th>Berlaku sampai</th>
                        <th>Status</th>
                        <th>
                          <span className="sr-only">Tindakan</span>
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.tokens.map((connection) => {
                        const status = aiConnectionStatus(
                          connection,
                          data.access,
                        );
                        return (
                          <tr key={connection.id}>
                            <td>
                              <strong>{connection.name}</strong>
                              <small className="ai-token-detail">
                                Dibuat {dateTime(connection.created_at)}
                              </small>
                              <small className="ai-token-detail">
                                {connection.last_used_at
                                  ? `Terakhir digunakan ${dateTime(connection.last_used_at)}`
                                  : "Belum pernah digunakan"}
                              </small>
                            </td>
                            <td>{scopeNames(connection.scopes)}</td>
                            <td>{dateTime(connection.expires_at)}</td>
                            <td>
                              <span
                                className={`status status-${status.usable ? "paid" : "cancelled"}`}
                              >
                                <span />
                                {status.label}
                              </span>
                            </td>
                            <td>
                              {!connection.revoked_at && (
                                <button
                                  className="text-button"
                                  onClick={() => setRevoke(connection)}
                                  aria-label={`Cabut koneksi ${connection.name}`}
                                >
                                  <Unplug size={14} /> Cabut
                                </button>
                              )}
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              ) : (
                <EmptyState
                  title="Belum ada koneksi"
                  description="Setelah memperoleh izin, buat kunci akses pribadi untuk menghubungkan Codex."
                />
              )}
            </section>

            {user.role === "admin" && (
              <section className="ai-section" aria-labelledby="ai-team-heading">
                <div className="ai-section-title">
                  <div>
                    <h3 id="ai-team-heading">Izin pengguna</h3>
                    <p className="muted">
                      Administrator mengatur izin setelah persetujuan pemilik.
                      Staf IT tidak otomatis mendapat akses data usaha.
                    </p>
                  </div>
                </div>
                <div className="table-scroll">
                  <table>
                    <thead>
                      <tr>
                        <th>Pengguna</th>
                        <th>Peran</th>
                        <th>Akses AI</th>
                        <th>Data yang diizinkan</th>
                        <th>
                          <span className="sr-only">Tindakan</span>
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {(data.users ?? []).map((account) => (
                        <tr key={account.id}>
                          <td>
                            <strong>
                              {account.name}
                              {account.id === user.id ? " (Anda)" : ""}
                            </strong>
                            <small className="ai-token-detail">
                              {account.email}
                              {account.active ? "" : " · Akun nonaktif"}
                            </small>
                          </td>
                          <td>{roleNames[account.role]}</td>
                          <td>
                            {account.enabled && account.active
                              ? "Diizinkan"
                              : "Nonaktif"}
                          </td>
                          <td>
                            {scopeNames(
                              account.scopes.filter((scope) =>
                                account.allowed_scopes.includes(scope),
                              ),
                            )}
                          </td>
                          <td>
                            <button
                              className="text-button"
                              onClick={() => setGrant(account)}
                              aria-label={`Kelola akses AI ${account.name}`}
                            >
                              Kelola
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                <p className="muted ai-description">
                  Menonaktifkan izin mencabut semua kunci pengguna secara
                  permanen. Mengaktifkan izin kembali membutuhkan kunci baru.
                  Pengurangan cakupan data berlaku pada permintaan berikutnya.
                </p>
              </section>
            )}
          </>
        )
      )}
      <details className="ai-setup">
        <summary>Menghubungkan Codex di Windows</summary>
        <ol>
          <li>
            Jalankan server Haruflow dan minta administrator memberikan izin AI
            pada akun Anda.
          </li>
          <li>
            Buat kunci akses di atas. Skrip Connect-Codex.ps1 dalam paket Windows
            membantu menyimpannya dalam berkas UTF-8 tanpa BOM dengan izin hanya
            untuk akun Windows Anda. Kunci diminta secara tersembunyi.
          </li>
          <li>
            Tambahkan MCP server lokal di pengaturan Codex. Gunakan{" "}
            <code>haruflow.exe</code> dengan argumen <code>mcp</code>, alamat
            server, dan lokasi berkas kunci. Contoh konfigurasi:
          </li>
        </ol>
        <pre className="ai-config">
          <code>{codexConfiguration}</code>
        </pre>
        <p className="muted">
          Ganti lokasi program, nama pengguna Windows, dan port agar sesuai
          instalasi Anda. Jalankan Codex dan Haruflow pada PC yang sama untuk
          koneksi lokal ini. Isi berkas kunci hanya kunci akses, tanpa tanda
          kutip. Jangan tempel kunci ke chat, URL, repositori, atau contoh
          konfigurasi.
        </p>
        <p className="ai-example">
          Setelah terhubung, coba: “Ringkas kondisi usaha bulan ini berdasarkan
          data Haruflow. Sertakan periode, sumber angka, dan hal yang perlu
          ditindaklanjuti.”
        </p>
      </details>
      {create && data && (
        <TokenForm
          access={data.access}
          catalog={data.scope_catalog}
          onClose={() => setCreate(false)}
          onSaved={(token) => {
            setCreate(false);
            setSecret(token);
            reload();
          }}
        />
      )}
      {grant && data && (
        <GrantForm
          user={grant}
          catalog={data.scope_catalog}
          onClose={() => setGrant(null)}
          onSaved={() => {
            setGrant(null);
            setNotice("Izin integrasi AI berhasil diperbarui.");
            reload();
          }}
        />
      )}
      {revoke && (
        <RevokeForm
          connection={revoke}
          onClose={() => setRevoke(null)}
          onSaved={() => {
            setRevoke(null);
            setNotice(
              "Kunci akses dicabut. Koneksi tersebut tidak dapat digunakan lagi.",
            );
            reload();
          }}
        />
      )}
      {secret && <SecretReveal secret={secret} onClose={() => setSecret("")} />}
    </div>
  );
}

function ScopeFields({
  catalog,
  allowed,
  selected,
  onChange,
  disabled = false,
}: {
  catalog: AiAccessResponse["scope_catalog"];
  allowed: AiScope[];
  selected: AiScope[];
  onChange: (scopes: AiScope[]) => void;
  disabled?: boolean;
}) {
  return (
    <fieldset className="ai-scope-fields" disabled={disabled}>
      <legend>Data yang boleh dibaca</legend>
      {catalog.map((scope) => (
        <label className="checkbox-field" key={scope.id}>
          <input
            type="checkbox"
            checked={selected.includes(scope.id) && allowed.includes(scope.id)}
            disabled={!allowed.includes(scope.id)}
            onChange={(event) =>
              onChange(
                event.target.checked
                  ? [...selected.filter((item) => item !== scope.id), scope.id]
                  : selected.filter((item) => item !== scope.id),
              )
            }
          />
          <span>
            {scope.label}
            {allowed.includes(scope.id) ? "" : " — di luar izin akun"}
          </span>
        </label>
      ))}
    </fieldset>
  );
}

function TokenForm({
  access,
  catalog,
  onClose,
  onSaved,
}: {
  access: AiAccess;
  catalog: AiAccessResponse["scope_catalog"];
  onClose: () => void;
  onSaved: (secret: string) => void;
}) {
  const allowed = usableAiScopes(access);
  const [selected, setSelected] = useState(allowed);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const scopes = selected.filter((scope) => allowed.includes(scope));
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!scopes.length) {
      setError("Pilih setidaknya satu cakupan data yang diizinkan.");
      return;
    }
    const fields = new FormData(event.currentTarget);
    setBusy(true);
    setError("");
    try {
      const result = await api<{ token: string; connection: AiConnection }>(
        "/ai/tokens",
        {
          method: "POST",
          body: {
            name: String(fields.get("name") ?? "").trim(),
            scopes,
            expires_in_days: Number(fields.get("expires_in_days")),
          },
        },
      );
      onSaved(result.token);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title="Buat kunci akses pribadi"
      description="Kunci hanya ditampilkan sekali dan memberikan akses baca sesuai pilihan Anda."
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <FormField label="Nama koneksi *">
            <input
              name="name"
              required
              maxLength={80}
              placeholder="Codex PC saya"
              autoFocus
              disabled={busy}
            />
          </FormField>
          <ScopeFields
            catalog={catalog}
            allowed={allowed}
            selected={selected}
            onChange={setSelected}
            disabled={busy}
          />
          <FormField label="Masa berlaku">
            <select name="expires_in_days" defaultValue="30" disabled={busy}>
              <option value="7">7 hari</option>
              <option value="30">30 hari</option>
              <option value="90">90 hari</option>
            </select>
          </FormField>
          <p className="muted">
            Setiap kunci terikat pada akun Anda. Data dapat diproses oleh
            penyedia AI yang digunakan Codex; gunakan sesuai persetujuan pemilik
            usaha.
          </p>
          <FormError error={error} />
        </div>
        <FormActions onClose={onClose} busy={busy} label="Buat kunci akses" />
      </form>
    </Modal>
  );
}

function GrantForm({
  user,
  catalog,
  onClose,
  onSaved,
}: {
  user: AiUser;
  catalog: AiAccessResponse["scope_catalog"];
  onClose: () => void;
  onSaved: () => void;
}) {
  const [enabled, setEnabled] = useState(user.enabled && user.active);
  const [scopes, setScopes] = useState(
    user.scopes.filter((scope) => user.allowed_scopes.includes(scope)),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    if (enabled && !scopes.length) {
      setError("Pilih setidaknya satu cakupan data.");
      return;
    }
    setBusy(true);
    try {
      await api(`/ai/access/${user.id}`, {
        method: "PUT",
        body: { enabled, scopes: enabled ? scopes : [] },
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
      title="Kelola izin integrasi AI"
      description={`${user.name} · ${roleNames[user.role]}`}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <label className="checkbox-field">
            <input
              type="checkbox"
              checked={enabled}
              disabled={busy || !user.active || !user.allowed_scopes.length}
              onChange={(event) => setEnabled(event.target.checked)}
            />
            <span>Izinkan pengguna menghubungkan Codex ke Haruflow</span>
          </label>
          {!user.active && (
            <p className="muted">
              Akun nonaktif tidak dapat diberi akses integrasi AI.
            </p>
          )}
          <ScopeFields
            catalog={catalog}
            allowed={user.allowed_scopes}
            selected={scopes}
            onChange={setScopes}
            disabled={busy || !enabled}
          />
          <div className="info-box">
            <Shield size={18} />
            <p>
              Berikan izin setelah persetujuan pemilik usaha. Menonaktifkan izin
              mencabut semua kunci pengguna secara permanen. Cakupan data tetap
              dibatasi oleh peran akun.
            </p>
          </div>
          <FormError error={error} />
        </div>
        <FormActions onClose={onClose} busy={busy} label="Simpan izin AI" />
      </form>
    </Modal>
  );
}

function RevokeForm({
  connection,
  onClose,
  onSaved,
}: {
  connection: AiConnection;
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
      await api(`/ai/tokens/${connection.id}`, { method: "DELETE" });
      onSaved();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title="Cabut kunci akses?"
      description={connection.name}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <p>
            Codex tidak dapat memakai kunci ini lagi. Jika ingin menghubungkan
            kembali, buat kunci baru.
          </p>
          <FormError error={error} />
        </div>
        <FormActions onClose={onClose} busy={busy} label="Cabut kunci akses" />
      </form>
    </Modal>
  );
}

function SecretReveal({
  secret,
  onClose,
}: {
  secret: string;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState("");
  async function copy() {
    setError("");
    setCopied(false);
    try {
      if (!window.isSecureContext || !navigator.clipboard?.writeText)
        throw new Error(
          "Penyalinan otomatis tidak tersedia. Pilih dan salin kunci secara manual.",
        );
      await navigator.clipboard.writeText(secret);
      setCopied(true);
    } catch {
      setError(
        "Penyalinan otomatis belum berhasil. Pilih dan salin kunci secara manual.",
      );
    }
  }
  return (
    <Modal
      title="Simpan kunci akses sekarang"
      description="Kunci ini hanya ditampilkan sekali. Setelah dialog ditutup, Haruflow tidak dapat menampilkannya kembali."
      onClose={onClose}
    >
      <div className="form-body">
        <div className="info-box">
          <KeyRound size={18} />
          <p>
            Simpan isi kunci ke berkas teks UTF-8 pribadi untuk
            HARUFLOW_MCP_TOKEN_FILE. Jangan kirim kunci melalui chat atau
            masukkan ke repositori.
          </p>
        </div>
        <FormField label="Kunci akses rahasia">
          <textarea
            className="ai-secret"
            value={secret}
            readOnly
            autoComplete="off"
            spellCheck={false}
            rows={3}
            onFocus={(event) => event.target.select()}
          />
        </FormField>
        <button type="button" className="button secondary" onClick={copy}>
          <Copy size={16} /> Salin kunci
        </button>
        {copied && (
          <p role="status" className="muted">
            Kunci berhasil disalin. Simpan ke berkas pribadi, lalu bersihkan
            clipboard setelah selesai.
          </p>
        )}
        <FormError error={error} />
      </div>
      <div className="form-actions">
        <button type="button" className="button primary" onClick={onClose}>
          Selesai dan tutup kunci
        </button>
      </div>
    </Modal>
  );
}
