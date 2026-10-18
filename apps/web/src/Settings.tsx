import { useState, type FormEvent } from "react";
import {
  Building2,
  KeyRound,
  Plus,
  Shield,
  UserRound,
  Users,
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
  PageHeader,
  SearchInput,
} from "./components";
import { roleNames } from "./format";
import { useResource } from "./hooks";
import type { Company, Role, Session, User } from "./types";
import Audit from "./Audit";

export default function Settings({
  session,
  onSession,
}: {
  session: Session;
  onSession: (session: Session) => void;
}) {
  const [tab, setTab] = useState("workspace");
  const [password, setPassword] = useState(false);
  return (
    <>
      <PageHeader
        eyebrow="RUANG KERJA SESUAI KEBUTUHAN"
        title="Pengaturan"
        description="Kelola akun, akses tim, dan identitas ruang kerja Anda."
      />
      <section className="panel">
        <div
          className="settings-tabs tabs"
          role="tablist"
          aria-label="Pengaturan"
        >
          <button
            role="tab"
            aria-selected={tab === "workspace"}
            className={tab === "workspace" ? "active" : ""}
            onClick={() => setTab("workspace")}
          >
            <Building2 size={16} /> Ruang kerja
          </button>
          {session.user.role === "admin" && (
            <button
              role="tab"
              aria-selected={tab === "users"}
              className={tab === "users" ? "active" : ""}
              onClick={() => setTab("users")}
            >
              <Users size={16} /> Pengguna
            </button>
          )}
          {["admin", "finance"].includes(session.user.role) && (
            <button
              role="tab"
              aria-selected={tab === "audit"}
              className={tab === "audit" ? "active" : ""}
              onClick={() => setTab("audit")}
            >
              <Shield size={16} /> Aktivitas
            </button>
          )}
          <button
            role="tab"
            aria-selected={tab === "profile"}
            className={tab === "profile" ? "active" : ""}
            onClick={() => setTab("profile")}
          >
            <UserRound size={16} /> Akun saya
          </button>
        </div>
        {tab === "workspace" ? (
          <CompanySettings
            session={session}
            onSaved={(company) => onSession({ ...session, company })}
          />
        ) : tab === "users" ? (
          <UsersView currentUser={session.user} />
        ) : tab === "audit" ? (
          <Audit />
        ) : (
          <div className="settings-body">
            <div className="settings-section-heading">
              <span className="initial-avatar large-avatar">
                {session.user.name.slice(0, 1)}
              </span>
              <div>
                <h2>{session.user.name}</h2>
                <p>{roleNames[session.user.role]}</p>
              </div>
            </div>
            <dl className="workspace-details">
              <div>
                <dt>Alamat email</dt>
                <dd>{session.user.email}</dd>
              </div>
              <div>
                <dt>Hak akses</dt>
                <dd>{roleNames[session.user.role]}</dd>
              </div>
            </dl>
            <div className="security-setting">
              <div>
                <h3>Kata sandi akun</h3>
                <p>
                  Perbarui secara berkala untuk melindungi ruang kerja Anda.
                </p>
              </div>
              <button
                className="button secondary"
                onClick={() => setPassword(true)}
              >
                <KeyRound size={16} /> Ubah kata sandi
              </button>
            </div>
          </div>
        )}
      </section>
      {password && (
        <PasswordForm
          onClose={() => setPassword(false)}
          onSaved={async () => {
            setPassword(false);
            onSession(await api<Session>("/auth/me"));
          }}
        />
      )}
    </>
  );
}

function CompanySettings({
  session,
  onSaved,
}: {
  session: Session;
  onSaved: (company: Company) => void;
}) {
  const { data, loading, error, reload } = useResource<Company>("/company");
  const [editing, setEditing] = useState(false);
  const [saved, setSaved] = useState(false);
  return (
    <div className="settings-body">
      <div className="settings-section-heading">
        <div className="settings-big-icon">
          <Building2 size={25} />
        </div>
        <div>
          <h2>Identitas perusahaan</h2>
          <p>Profil perusahaan dan penomoran dokumen penjualan Anda.</p>
        </div>
      </div>
      {loading ? (
        <Loading />
      ) : error ? (
        <ErrorState message={error} onRetry={reload} />
      ) : (
        data && (
          <>
            <dl className="workspace-details">
              <div>
                <dt>Nama perusahaan</dt>
                <dd>{data.name}</dd>
              </div>
              <div>
                <dt>Alamat</dt>
                <dd className="preserve-lines">{data.address || "—"}</dd>
              </div>
              <div>
                <dt>Email</dt>
                <dd>{data.email || "—"}</dd>
              </div>
              <div>
                <dt>Telepon</dt>
                <dd>{data.phone || "—"}</dd>
              </div>
              <div>
                <dt>NPWP / identitas pajak</dt>
                <dd>{data.tax_id || "—"}</dd>
              </div>
              <div>
                <dt>Mata uang</dt>
                <dd>{data.currency} · Rupiah Indonesia</dd>
              </div>
              <div>
                <dt>Zona waktu</dt>
                <dd>{data.timezone}</dd>
              </div>
              <div>
                <dt>Awalan nomor pesanan</dt>
                <dd className="mono">{data.order_prefix}</dd>
              </div>
              <div>
                <dt>Awalan nomor invoice</dt>
                <dd className="mono">{data.invoice_prefix}</dd>
              </div>
              <div>
                <dt>Edisi</dt>
                <dd>
                  <span className="subtle-chip">
                    Haruflow Community · Self-hosted
                  </span>
                </dd>
              </div>
            </dl>
            {saved && (
              <p className="muted" role="status">
                Profil perusahaan berhasil disimpan.
              </p>
            )}
            {session.user.role === "admin" && (
              <button
                className="button secondary"
                onClick={() => setEditing(true)}
              >
                Edit profil perusahaan
              </button>
            )}
          </>
        )
      )}
      <div className="info-box">
        <Shield size={18} />
        <p>
          Community menggunakan Rupiah (IDR). Zona waktu hanya dapat diubah
          sebelum jurnal pertama dicatat. Kelola backup dan pembaruan server
          melalui administrator sistem.
        </p>
      </div>
      {editing && data && (
        <CompanyForm
          company={data}
          onClose={() => setEditing(false)}
          onSaved={(company) => {
            setEditing(false);
            setSaved(true);
            onSaved(company);
            reload();
          }}
        />
      )}
    </div>
  );
}

function CompanyForm({
  company,
  onClose,
  onSaved,
}: {
  company: Company;
  onClose: () => void;
  onSaved: (company: Company) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const timezones = [
    ...new Set([
      company.timezone,
      "Asia/Jakarta",
      "Asia/Makassar",
      "Asia/Jayapura",
    ]),
  ];
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      const company = await api<Company>("/company", {
        method: "PATCH",
        body: Object.fromEntries(new FormData(event.currentTarget)),
      });
      onSaved(company);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title="Edit profil perusahaan"
      description="Identitas ini digunakan pada tampilan dan cetakan invoice."
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <FormField label="Nama perusahaan *">
            <input
              name="name"
              required
              minLength={2}
              maxLength={120}
              defaultValue={company.name}
              autoFocus
            />
          </FormField>
          <FormField label="Alamat">
            <textarea
              name="address"
              rows={3}
              maxLength={500}
              defaultValue={company.address ?? ""}
            />
          </FormField>
          <div className="form-grid">
            <FormField label="Email">
              <input
                name="email"
                type="email"
                maxLength={254}
                defaultValue={company.email ?? ""}
              />
            </FormField>
            <FormField label="Telepon">
              <input
                name="phone"
                type="tel"
                maxLength={40}
                defaultValue={company.phone ?? ""}
              />
            </FormField>
          </div>
          <FormField
            label="NPWP / identitas pajak"
            hint="Informasi pada invoice; perhitungan pajak otomatis belum tersedia."
          >
            <input
              name="tax_id"
              maxLength={64}
              defaultValue={company.tax_id ?? ""}
            />
          </FormField>
          <div className="form-grid">
            <FormField label="Mata uang">
              <input value="IDR · Rupiah Indonesia" readOnly />
            </FormField>
            <FormField
              label="Zona waktu *"
              hint="Dikunci setelah jurnal pertama agar tanggal pembukuan tetap konsisten."
            >
              <select name="timezone" defaultValue={company.timezone}>
                {timezones.map((timezone) => (
                  <option key={timezone} value={timezone}>
                    {timezone}
                  </option>
                ))}
              </select>
            </FormField>
          </div>
          <div className="form-grid">
            <FormField label="Awalan pesanan *">
              <input
                name="order_prefix"
                required
                minLength={2}
                maxLength={10}
                pattern="[A-Z][A-Z0-9\\-]{1,9}"
                title="2–10 huruf kapital, angka, atau tanda hubung, dimulai dengan huruf."
                defaultValue={company.order_prefix ?? "SO"}
              />
            </FormField>
            <FormField label="Awalan invoice *">
              <input
                name="invoice_prefix"
                required
                minLength={2}
                maxLength={10}
                pattern="[A-Z][A-Z0-9\\-]{1,9}"
                title="2–10 huruf kapital, angka, atau tanda hubung, dimulai dengan huruf."
                defaultValue={company.invoice_prefix ?? "INV"}
              />
            </FormField>
          </div>
          <div className="info-box">
            <Building2 size={18} />
            <p>
              Awalan berlaku untuk dokumen baru. Nomor dokumen yang sudah dibuat
              tetap tersimpan.
            </p>
          </div>
          <FormError error={error} />
        </div>
        <FormActions
          busy={busy}
          onClose={onClose}
          label="Simpan profil perusahaan"
        />
      </form>
    </Modal>
  );
}

function UsersView({ currentUser }: { currentUser: User }) {
  const { data, loading, error, reload } = useResource<{ items: User[] }>(
    "/users",
  );
  const [search, setSearch] = useState("");
  const [create, setCreate] = useState(false);
  const [edit, setEdit] = useState<User | null>(null);
  const items = (data?.items ?? []).filter((item) =>
    `${item.name} ${item.email}`.toLowerCase().includes(search.toLowerCase()),
  );
  return (
    <>
      <div className="table-toolbar">
        <SearchInput
          value={search}
          onChange={setSearch}
          placeholder="Cari nama atau email…"
        />
        <button className="button primary" onClick={() => setCreate(true)}>
          <Plus size={16} /> Tambah pengguna
        </button>
      </div>
      {loading ? (
        <Loading />
      ) : error ? (
        <ErrorState message={error} onRetry={reload} />
      ) : items.length ? (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>Pengguna</th>
                <th>Email</th>
                <th>Hak akses</th>
                <th>Status</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {items.map((user) => (
                <tr key={user.id}>
                  <td>
                    <span className="customer-cell">
                      <span className="initial-avatar">
                        {user.name.slice(0, 1)}
                      </span>
                      <strong>{user.name}</strong>
                      {user.id === currentUser.id && (
                        <span className="subtle-chip">Anda</span>
                      )}
                    </span>
                  </td>
                  <td className="muted">{user.email}</td>
                  <td>{roleNames[user.role]}</td>
                  <td>
                    <span
                      className={`status status-${user.active ? "paid" : "cancelled"}`}
                    >
                      <span />
                      {user.active ? "Aktif" : "Nonaktif"}
                    </span>
                  </td>
                  <td>
                    {user.id !== currentUser.id && (
                      <button
                        className="text-button"
                        onClick={() => setEdit(user)}
                      >
                        Kelola
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <EmptyState
          title="Pengguna tidak ditemukan"
          description="Coba nama atau email yang berbeda."
        />
      )}
      {create && (
        <UserForm
          onClose={() => setCreate(false)}
          onSaved={() => {
            setCreate(false);
            reload();
          }}
        />
      )}
      {edit && (
        <UserForm
          user={edit}
          onClose={() => setEdit(null)}
          onSaved={() => {
            setEdit(null);
            reload();
          }}
        />
      )}
    </>
  );
}

function RoleSelect({ initial = "viewer" }: { initial?: Role }) {
  return (
    <select name="role" defaultValue={initial} required>
      {Object.entries(roleNames).map(([value, label]) => (
        <option key={value} value={value}>
          {label}
        </option>
      ))}
    </select>
  );
}

function UserForm({
  user,
  onClose,
  onSaved,
}: {
  user?: User;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    const fields = Object.fromEntries(new FormData(event.currentTarget));
    try {
      if (user)
        await api(`/users/${user.id}`, {
          method: "PATCH",
          body: { role: fields.role, active: fields.active === "true" },
        });
      else await api("/users", { method: "POST", body: fields });
      onSaved();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={user ? "Kelola akses pengguna" : "Tambah pengguna"}
      description={
        user
          ? `${user.name} · ${user.email}`
          : "Berikan akses sesuai tanggung jawab anggota tim."
      }
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          {!user && (
            <>
              <FormField label="Nama lengkap *">
                <input
                  name="name"
                  required
                  maxLength={100}
                  autoFocus
                  placeholder="Nama anggota tim"
                />
              </FormField>
              <FormField label="Alamat email *">
                <input
                  name="email"
                  type="email"
                  required
                  maxLength={200}
                  autoComplete="off"
                  placeholder="nama@perusahaan.com"
                />
              </FormField>
              <FormField
                label="Kata sandi awal *"
                hint="Minimal 12 karakter. Pengguna dapat mengubahnya setelah masuk."
              >
                <input
                  name="password"
                  type="password"
                  required
                  minLength={12}
                  maxLength={128}
                  autoComplete="new-password"
                />
              </FormField>
            </>
          )}
          <FormField label="Hak akses *">
            <RoleSelect initial={user?.role} />
          </FormField>
          {user && (
            <FormField label="Status akun">
              <select name="active" defaultValue={String(user.active)}>
                <option value="true">Aktif</option>
                <option value="false">Nonaktif — cabut akses pengguna</option>
              </select>
            </FormField>
          )}
          <div className="info-box">
            <Shield size={18} />
            <p>
              Administrator mengelola seluruh modul. Penjualan, Gudang, dan
              Keuangan hanya dapat memproses alur sesuai perannya. Pengamat
              memiliki akses baca.
            </p>
          </div>
          <FormError error={error} />
        </div>
        <FormActions
          onClose={onClose}
          busy={busy}
          label={user ? "Simpan akses" : "Tambah pengguna"}
        />
      </form>
    </Modal>
  );
}

function PasswordForm({
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
    const fields = Object.fromEntries(new FormData(event.currentTarget));
    if (fields.password !== fields.confirm_password) {
      setError("Konfirmasi kata sandi belum sama.");
      setBusy(false);
      return;
    }
    try {
      await api("/auth/password", {
        method: "POST",
        body: {
          current_password: fields.current_password,
          password: fields.password,
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
      title="Ubah kata sandi"
      description="Sesi perangkat lain akan diakhiri setelah perubahan berhasil."
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      <form onSubmit={submit}>
        <div className="form-body">
          <FormField label="Kata sandi saat ini">
            <input
              name="current_password"
              type="password"
              required
              autoComplete="current-password"
              autoFocus
            />
          </FormField>
          <FormField
            label="Kata sandi baru"
            hint="Gunakan minimal 12 karakter."
          >
            <input
              name="password"
              type="password"
              required
              minLength={12}
              maxLength={128}
              autoComplete="new-password"
            />
          </FormField>
          <FormField label="Konfirmasi kata sandi baru">
            <input
              name="confirm_password"
              type="password"
              required
              minLength={12}
              maxLength={128}
              autoComplete="new-password"
            />
          </FormField>
          <FormError error={error} />
        </div>
        <FormActions onClose={onClose} busy={busy} label="Ubah kata sandi" />
      </form>
    </Modal>
  );
}
