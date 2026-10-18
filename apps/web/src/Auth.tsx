import { useState, type FormEvent } from "react";
import {
  ArrowRight,
  Check,
  Eye,
  EyeOff,
  Layers3,
  LockKeyhole,
  Package,
  ShieldCheck,
} from "lucide-react";
import { api, errorMessage } from "./api";
import { Brand, FormError, FormField } from "./components";
import type { Session } from "./types";

export default function Auth({
  setup,
  onAuthenticated,
}: {
  setup: boolean;
  onAuthenticated: (session: Session) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    const fields = Object.fromEntries(new FormData(event.currentTarget));
    try {
      await api(setup ? "/setup" : "/auth/login", {
        method: "POST",
        body: fields,
      });
      onAuthenticated(await api<Session>("/auth/me"));
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="auth-layout">
      <aside className="auth-story">
        <Brand />
        <div className="auth-story-content">
          <span className="community-pill">
            <span /> COMMUNITY EDITION
          </span>
          <h1>
            Ruang untuk
            <br />
            bisnis Anda
            <br />
            <em>bertumbuh.</em>
          </h1>
          <p>
            Penjualan yang lancar. Stok yang tertata.
            <br />
            Keuangan yang jelas. Semuanya terhubung.
          </p>
          <div className="auth-illustration" aria-hidden="true">
            <div className="illustration-card illustration-back">
              <Package size={25} />
              <div>
                <i />
                <i />
              </div>
              <Check size={18} />
            </div>
            <div className="illustration-card illustration-front">
              <Layers3 size={29} />
              <div>
                <strong>Satu alur. Satu sumber data.</strong>
                <span>Bisnis lebih teratur, hari lebih tenang.</span>
              </div>
              <span className="illustration-check">
                <Check size={15} />
              </span>
            </div>
            <div className="illustration-line" />
            <span className="illustration-dot" />
          </div>
        </div>
        <div className="auth-story-footer">
          <ShieldCheck size={17} /> Data Anda. Infrastruktur Anda. Kendali Anda.
        </div>
      </aside>
      <main className="auth-main">
        <div className="mobile-brand">
          <Brand dark />
        </div>
        <div className="auth-form-wrap">
          <div className="auth-icon">
            <LockKeyhole size={23} />
          </div>
          <div className="eyebrow">
            {setup ? "MULAI DARI SINI" : "SELAMAT DATANG KEMBALI"}
          </div>
          <h2>
            {setup ? "Bangun ruang kerja Anda." : "Masuk ke ruang kerja."}
          </h2>
          <p className="auth-intro">
            {setup
              ? "Siapkan perusahaan dan akun administrator pertama untuk memulai Haruflow."
              : "Semua yang dibutuhkan bisnis Anda, siap dilanjutkan."}
          </p>
          <form onSubmit={submit} className="auth-form">
            {setup && (
              <>
                <FormField
                  label="Token pengaturan"
                  hint="Token HARUFLOW_SETUP_TOKEN dari konfigurasi server."
                >
                  <input
                    autoFocus
                    name="token"
                    type="password"
                    required
                    autoComplete="off"
                    placeholder="Masukkan token pengaturan"
                  />
                </FormField>
                <FormField label="Nama perusahaan">
                  <input
                    name="company_name"
                    required
                    maxLength={150}
                    placeholder="Contoh: Toko Haru"
                    autoComplete="organization"
                  />
                </FormField>
                <FormField label="Nama lengkap">
                  <input
                    name="name"
                    required
                    maxLength={100}
                    placeholder="Nama administrator"
                    autoComplete="name"
                  />
                </FormField>
              </>
            )}
            <FormField label="Alamat email">
              <input
                name="email"
                type="email"
                required
                maxLength={200}
                autoComplete="username"
                placeholder="nama@perusahaan.com"
                autoFocus={!setup}
              />
            </FormField>
            <FormField
              label="Kata sandi"
              hint={
                setup
                  ? "Gunakan minimal 12 karakter agar akun lebih aman."
                  : undefined
              }
            >
              <span className="password-input">
                <input
                  name="password"
                  type={showPassword ? "text" : "password"}
                  minLength={setup ? 12 : undefined}
                  required
                  autoComplete={setup ? "new-password" : "current-password"}
                  placeholder={
                    setup ? "Buat kata sandi yang kuat" : "Masukkan kata sandi"
                  }
                />
                <button
                  type="button"
                  aria-label={
                    showPassword
                      ? "Sembunyikan kata sandi"
                      : "Tampilkan kata sandi"
                  }
                  onClick={() => setShowPassword(!showPassword)}
                >
                  {showPassword ? <EyeOff size={18} /> : <Eye size={18} />}
                </button>
              </span>
            </FormField>
            <FormError error={error} />
            <button className="button primary auth-submit" disabled={busy}>
              {busy
                ? "Menghubungkan…"
                : setup
                  ? "Buat ruang kerja"
                  : "Masuk ke Haruflow"}
              <ArrowRight size={18} />
            </button>
          </form>
          <div className="auth-footnote">
            <ShieldCheck size={15} />
            <span>
              {setup
                ? "Gratis, self-hosted, dan selalu milik Anda."
                : "Sesi Anda dilindungi dan disimpan dengan aman."}
            </span>
          </div>
        </div>
        <footer className="auth-footer">
          Haruflow Community <span>ERP yang mengalir bersama bisnis Anda.</span>
        </footer>
      </main>
    </div>
  );
}
