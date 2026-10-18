import { useState, type FormEvent } from 'react';
import { Building2, KeyRound, Plus, Shield, UserRound, Users } from 'lucide-react';
import { api, errorMessage } from './api';
import { EmptyState, ErrorState, FormActions, FormError, FormField, Loading, Modal, PageHeader, SearchInput } from './components';
import { roleNames } from './format';
import { useResource } from './hooks';
import type { Role, Session, User } from './types';
import Audit from './Audit';

export default function Settings({ session, onSession }: { session: Session; onSession: (session: Session) => void }) {
  const [tab, setTab] = useState('workspace');
  const [password, setPassword] = useState(false);
  return <><PageHeader eyebrow="RUANG KERJA SESUAI KEBUTUHAN" title="Pengaturan" description="Kelola akun, akses tim, dan identitas ruang kerja Anda." /><section className="panel"><div className="settings-tabs tabs" role="tablist" aria-label="Pengaturan"><button role="tab" aria-selected={tab === 'workspace'} className={tab === 'workspace' ? 'active' : ''} onClick={() => setTab('workspace')}><Building2 size={16} /> Ruang kerja</button>{session.user.role === 'admin' && <button role="tab" aria-selected={tab === 'users'} className={tab === 'users' ? 'active' : ''} onClick={() => setTab('users')}><Users size={16} /> Pengguna</button>}{['admin', 'finance'].includes(session.user.role) && <button role="tab" aria-selected={tab === 'audit'} className={tab === 'audit' ? 'active' : ''} onClick={() => setTab('audit')}><Shield size={16} /> Aktivitas</button>}<button role="tab" aria-selected={tab === 'profile'} className={tab === 'profile' ? 'active' : ''} onClick={() => setTab('profile')}><UserRound size={16} /> Akun saya</button></div>{tab === 'workspace' ? <div className="settings-body"><div className="settings-section-heading"><div className="settings-big-icon"><Building2 size={25} /></div><div><h2>Identitas perusahaan</h2><p>Informasi dasar ruang kerja Haruflow Anda.</p></div></div><dl className="workspace-details"><div><dt>Nama perusahaan</dt><dd>{session.company.name}</dd></div><div><dt>Mata uang</dt><dd>{session.company.currency} · Rupiah Indonesia</dd></div><div><dt>Zona waktu</dt><dd>{session.company.timezone}</dd></div><div><dt>Edisi</dt><dd><span className="subtle-chip">Haruflow Community · Self-hosted</span></dd></div></dl><div className="info-box"><Shield size={18} /><p>Ruang kerja ini di-host pada infrastruktur Anda. Kelola backup dan pembaruan server melalui administrator sistem.</p></div></div> : tab === 'users' ? <UsersView currentUser={session.user} /> : tab === 'audit' ? <Audit /> : <div className="settings-body"><div className="settings-section-heading"><span className="initial-avatar large-avatar">{session.user.name.slice(0, 1)}</span><div><h2>{session.user.name}</h2><p>{roleNames[session.user.role]}</p></div></div><dl className="workspace-details"><div><dt>Alamat email</dt><dd>{session.user.email}</dd></div><div><dt>Hak akses</dt><dd>{roleNames[session.user.role]}</dd></div></dl><div className="security-setting"><div><h3>Kata sandi akun</h3><p>Perbarui secara berkala untuk melindungi ruang kerja Anda.</p></div><button className="button secondary" onClick={() => setPassword(true)}><KeyRound size={16} /> Ubah kata sandi</button></div></div>}</section>{password && <PasswordForm onClose={() => setPassword(false)} onSaved={async () => { setPassword(false); onSession(await api<Session>('/auth/me')); }} />}</>;
}

function UsersView({ currentUser }: { currentUser: User }) {
  const { data, loading, error, reload } = useResource<{ items: User[] }>('/users');
  const [search, setSearch] = useState('');
  const [create, setCreate] = useState(false);
  const [edit, setEdit] = useState<User | null>(null);
  const items = (data?.items ?? []).filter(item => `${item.name} ${item.email}`.toLowerCase().includes(search.toLowerCase()));
  return <><div className="table-toolbar"><SearchInput value={search} onChange={setSearch} placeholder="Cari nama atau email…" /><button className="button primary" onClick={() => setCreate(true)}><Plus size={16} /> Tambah pengguna</button></div>{loading ? <Loading /> : error ? <ErrorState message={error} onRetry={reload} /> : items.length ? <div className="table-scroll"><table><thead><tr><th>Pengguna</th><th>Email</th><th>Hak akses</th><th>Status</th><th /></tr></thead><tbody>{items.map(user => <tr key={user.id}><td><span className="customer-cell"><span className="initial-avatar">{user.name.slice(0, 1)}</span><strong>{user.name}</strong>{user.id === currentUser.id && <span className="subtle-chip">Anda</span>}</span></td><td className="muted">{user.email}</td><td>{roleNames[user.role]}</td><td><span className={`status status-${user.active ? 'paid' : 'cancelled'}`}><span />{user.active ? 'Aktif' : 'Nonaktif'}</span></td><td>{user.id !== currentUser.id && <button className="text-button" onClick={() => setEdit(user)}>Kelola</button>}</td></tr>)}</tbody></table></div> : <EmptyState title="Pengguna tidak ditemukan" description="Coba nama atau email yang berbeda." />}{create && <UserForm onClose={() => setCreate(false)} onSaved={() => { setCreate(false); reload(); }} />}{edit && <UserForm user={edit} onClose={() => setEdit(null)} onSaved={() => { setEdit(null); reload(); }} />}</>;
}

function RoleSelect({ initial = 'viewer' }: { initial?: Role }) { return <select name="role" defaultValue={initial} required>{Object.entries(roleNames).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select>; }

function UserForm({ user, onClose, onSaved }: { user?: User; onClose: () => void; onSaved: () => void }) {
  const [busy, setBusy] = useState(false); const [error, setError] = useState('');
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); setBusy(true); setError('');
    const fields = Object.fromEntries(new FormData(event.currentTarget));
    try {
      if (user) await api(`/users/${user.id}`, { method: 'PATCH', body: { role: fields.role, active: fields.active === 'true' } });
      else await api('/users', { method: 'POST', body: fields });
      onSaved();
    } catch (error) { setError(errorMessage(error)); } finally { setBusy(false); }
  }
  return <Modal title={user ? 'Kelola akses pengguna' : 'Tambah pengguna'} description={user ? `${user.name} · ${user.email}` : 'Berikan akses sesuai tanggung jawab anggota tim.'} onClose={() => { if (!busy) onClose(); }}><form onSubmit={submit}><div className="form-body">{!user && <><FormField label="Nama lengkap *"><input name="name" required maxLength={100} autoFocus placeholder="Nama anggota tim" /></FormField><FormField label="Alamat email *"><input name="email" type="email" required maxLength={200} autoComplete="off" placeholder="nama@perusahaan.com" /></FormField><FormField label="Kata sandi awal *" hint="Minimal 12 karakter. Pengguna dapat mengubahnya setelah masuk."><input name="password" type="password" required minLength={12} maxLength={128} autoComplete="new-password" /></FormField></>}<FormField label="Hak akses *"><RoleSelect initial={user?.role} /></FormField>{user && <FormField label="Status akun"><select name="active" defaultValue={String(user.active)}><option value="true">Aktif</option><option value="false">Nonaktif — cabut akses pengguna</option></select></FormField>}<div className="info-box"><Shield size={18} /><p>Administrator mengelola seluruh modul. Penjualan, Gudang, dan Keuangan hanya dapat memproses alur sesuai perannya. Pengamat memiliki akses baca.</p></div><FormError error={error} /></div><FormActions onClose={onClose} busy={busy} label={user ? 'Simpan akses' : 'Tambah pengguna'} /></form></Modal>;
}

function PasswordForm({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const [busy, setBusy] = useState(false); const [error, setError] = useState('');
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); setBusy(true); setError('');
    const fields = Object.fromEntries(new FormData(event.currentTarget));
    if (fields.password !== fields.confirm_password) { setError('Konfirmasi kata sandi belum sama.'); setBusy(false); return; }
    try { await api('/auth/password', { method: 'POST', body: { current_password: fields.current_password, password: fields.password } }); onSaved(); } catch (error) { setError(errorMessage(error)); } finally { setBusy(false); }
  }
  return <Modal title="Ubah kata sandi" description="Sesi perangkat lain akan diakhiri setelah perubahan berhasil." onClose={() => { if (!busy) onClose(); }}><form onSubmit={submit}><div className="form-body"><FormField label="Kata sandi saat ini"><input name="current_password" type="password" required autoComplete="current-password" autoFocus /></FormField><FormField label="Kata sandi baru" hint="Gunakan minimal 12 karakter."><input name="password" type="password" required minLength={12} maxLength={128} autoComplete="new-password" /></FormField><FormField label="Konfirmasi kata sandi baru"><input name="confirm_password" type="password" required minLength={12} maxLength={128} autoComplete="new-password" /></FormField><FormError error={error} /></div><FormActions onClose={onClose} busy={busy} label="Ubah kata sandi" /></form></Modal>;
}
