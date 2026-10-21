import { useEffect, useState } from "react";
import {
  ArrowUpRight,
  BellRing,
  Building2,
  ChevronDown,
  ChevronRight,
  CircleHelp,
  Command,
  LayoutDashboard,
  LogOut,
  Menu,
  Package,
  RefreshCw,
  Settings2,
  ShieldCheck,
  ShoppingBag,
  ClipboardList,
  HandCoins,
  History as HistoryIcon,
  Sprout,
  Users,
  WalletCards,
  X,
} from "lucide-react";
import { api, ApiError, errorMessage } from "./api";
import { Brand, ErrorState, Loading, Modal } from "./components";
import { roleNames, setCompanyTimezone } from "./format";
import { ResourceRevisionContext, SessionRoleContext } from "./hooks";
import Auth from "./Auth";
import Dashboard from "./Dashboard";
import Customers from "./Customers";
import Inventory from "./Inventory";
import Sales from "./Sales";
import Accounting from "./Accounting";
import Settings from "./Settings";
import Purchasing from "./Purchasing";
import PartyBalances from "./PartyBalances";
import History from "./History";
import Treasury from "./Treasury";
import { TransactionImports } from "./TransactionImports";
import type { Session } from "./types";

const navigation = [
  { key: "dashboard", label: "Beranda", icon: LayoutDashboard },
  { key: "sales", label: "Penjualan", icon: ShoppingBag },
  { key: "purchasing", label: "Pembelian", icon: ClipboardList },
  { key: "inventory", label: "Inventori", icon: Package },
  { key: "customers", label: "Pelanggan", icon: Users },
  { key: "accounting", label: "Akuntansi", icon: WalletCards },
  { key: "balances", label: "Saldo pihak", icon: HandCoins },
  { key: "treasury", label: "Kas & bank", icon: WalletCards },
  { key: "transaction-imports", label: "Impor transaksi", icon: ClipboardList },
  { key: "history", label: "Riwayat lengkap", icon: HistoryIcon },
];
const validPages = [...navigation.map((item) => item.key), "settings"];

export default function App() {
  const [session, setSession] = useState<Session | null>(null);
  const [setup, setSetup] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [page, setPage] = useState(() =>
    validPages.includes(location.hash.slice(1))
      ? location.hash.slice(1)
      : "dashboard",
  );
  const [mobileNav, setMobileNav] = useState(false);
  const [help, setHelp] = useState(false);
  const [revision, setRevision] = useState(0);
  const [notice, setNotice] = useState("");
  const [loggingOut, setLoggingOut] = useState(false);

  function updateSession(next: Session) {
    setCompanyTimezone(next.company.timezone);
    setSession(next);
  }

  async function initialize() {
    setLoading(true);
    setError("");
    try {
      const state = await api<{ needs_setup: boolean }>("/setup/status");
      setSetup(state.needs_setup);
      if (!state.needs_setup) {
        try {
          updateSession(await api<Session>("/auth/me"));
        } catch (error) {
          if (error instanceof ApiError && error.status === 401)
            setSession(null);
          else throw error;
        }
      }
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => {
    void initialize();
  }, []);
  useEffect(() => {
    const unauthorized = () => {
      setSession(null);
      setNotice("Sesi Anda telah berakhir. Silakan masuk kembali.");
    };
    window.addEventListener("haruflow:unauthorized", unauthorized);
    return () =>
      window.removeEventListener("haruflow:unauthorized", unauthorized);
  }, []);
  useEffect(() => {
    const hash = () => {
      const next = location.hash.slice(1);
      if (validPages.includes(next)) setPage(next);
    };
    window.addEventListener("hashchange", hash);
    return () => window.removeEventListener("hashchange", hash);
  }, []);
  useEffect(() => {
    if (notice) {
      const timeout = setTimeout(() => setNotice(""), 5000);
      return () => clearTimeout(timeout);
    }
  }, [notice]);
  function navigate(next: string) {
    setPage(next);
    location.hash = next;
    setMobileNav(false);
    window.scrollTo({ top: 0 });
  }
  function changed() {
    setRevision((value) => value + 1);
    setNotice("Perubahan berhasil disimpan.");
  }
  async function logout() {
    setLoggingOut(true);
    try {
      await api("/auth/logout", { method: "POST", body: {} });
      setSession(null);
      navigate("dashboard");
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setLoggingOut(false);
    }
  }
  if (loading)
    return (
      <div className="boot-screen">
        <Brand dark />
        <Loading label="Menyiapkan ruang kerja Anda…" />
      </div>
    );
  if (error)
    return (
      <div className="boot-screen">
        <Brand dark />
        <ErrorState message={error} onRetry={initialize} />
      </div>
    );
  if (!session)
    return (
      <>
        <Auth
          setup={setup}
          onAuthenticated={(next) => {
            updateSession(next);
            setSetup(false);
            setNotice("Selamat datang di Haruflow.");
          }}
        />
        {notice && (
          <div className="toast" role="status">
            <ShieldCheck size={18} />
            {notice}
          </div>
        )}
      </>
    );
  const currentName =
    [...navigation, { key: "settings", label: "Pengaturan" }].find(
      (item) => item.key === page,
    )?.label ?? "Beranda";
  return (
    <div className="app-layout">
      {mobileNav && (
        <button
          className="sidebar-overlay"
          aria-label="Tutup navigasi"
          onClick={() => setMobileNav(false)}
        />
      )}
      <aside className={`sidebar ${mobileNav ? "open" : ""}`}>
        <div className="sidebar-brand">
          <Brand />
          <button
            className="icon-button mobile-close"
            aria-label="Tutup navigasi"
            onClick={() => setMobileNav(false)}
          >
            <X size={20} />
          </button>
        </div>
        <button
          className="workspace-selector"
          onClick={() => navigate("settings")}
        >
          <span className="workspace-icon">
            <Building2 size={18} />
          </span>
          <span>
            <strong>{session.company.name}</strong>
            <small>Ruang kerja Anda</small>
          </span>
          <ChevronDown size={15} />
        </button>
        <div className="nav-label">RUANG KERJA</div>
        <nav aria-label="Navigasi utama">
          {navigation.map((item) => (
            <button
              key={item.key}
              className={`nav-item ${page === item.key ? "active" : ""}`}
              onClick={() => navigate(item.key)}
              aria-current={page === item.key ? "page" : undefined}
            >
              <item.icon size={19} strokeWidth={1.7} />
              <span>{item.label}</span>
              {page === item.key && <span className="active-nav-dot" />}
            </button>
          ))}
        </nav>
        <div className="nav-divider" />
        <div className="nav-label">PREFERENSI</div>
        <nav aria-label="Preferensi">
          <button
            className={`nav-item ${page === "settings" ? "active" : ""}`}
            onClick={() => navigate("settings")}
            aria-current={page === "settings" ? "page" : undefined}
          >
            <Settings2 size={19} strokeWidth={1.7} />
            <span>Pengaturan</span>
          </button>
          <button className="nav-item" onClick={() => setHelp(true)}>
            <CircleHelp size={19} strokeWidth={1.7} />
            <span>Panduan singkat</span>
            <ArrowUpRight size={14} />
          </button>
        </nav>
        <div className="sidebar-bottom">
          <div className="community-card">
            <Sprout size={22} strokeWidth={1.5} />
            <strong>Bisnis Anda. Cara Anda.</strong>
            <p>
              Gratis, terbuka, dan sepenuhnya
              <br />
              dalam kendali Anda.
            </p>
            <span>
              COMMUNITY EDITION <span>v0.1</span>
            </span>
          </div>
          <div className="sidebar-user">
            <button
              className="sidebar-profile"
              onClick={() => {
                navigate("settings");
              }}
            >
              <span className="user-avatar">
                {session.user.name.slice(0, 1).toUpperCase()}
              </span>
              <span>
                <strong>{session.user.name}</strong>
                <small>{roleNames[session.user.role]}</small>
              </span>
            </button>
            <button
              className="logout-button"
              disabled={loggingOut}
              aria-label="Keluar"
              title="Keluar"
              onClick={logout}
            >
              <LogOut size={18} />
            </button>
          </div>
        </div>
      </aside>
      <div className="main-layout">
        <header className="topbar">
          <div className="topbar-breadcrumb">
            <button
              className="icon-button mobile-menu"
              aria-label="Buka navigasi"
              onClick={() => setMobileNav(true)}
            >
              <Menu size={21} />
            </button>
            <span className="breadcrumb-company">{session.company.name}</span>
            <ChevronRight size={13} />
            <strong>{currentName}</strong>
          </div>
          <div className="topbar-actions">
            <span className="workspace-status">
              <span /> Ruang kerja aktif
            </span>
            <button
              className="icon-button"
              aria-label="Muat ulang data"
              title="Muat ulang data"
              onClick={() => setRevision((value) => value + 1)}
            >
              <RefreshCw size={17} />
            </button>
            <span className="topbar-divider" />
            <button
              className="topbar-avatar"
              onClick={() => navigate("settings")}
              aria-label="Pengaturan akun"
            >
              {session.user.name.slice(0, 1).toUpperCase()}
            </button>
          </div>
        </header>
        <ResourceRevisionContext.Provider value={revision}>
          <SessionRoleContext.Provider value={session.user.role}>
            <main className="page-content" key={page} id="main-content">
              {page === "dashboard" ? (
                <Dashboard
                  session={session}
                  navigate={navigate}
                  revision={revision}
                />
              ) : page === "sales" ? (
                <Sales
                  session={session}
                  role={session.user.role}
                  changed={changed}
                />
              ) : page === "purchasing" ? (
                <Purchasing role={session.user.role} changed={changed} />
              ) : page === "inventory" ? (
                <Inventory role={session.user.role} changed={changed} />
              ) : page === "customers" ? (
                <Customers role={session.user.role} changed={changed} />
              ) : page === "accounting" ? (
                <Accounting role={session.user.role} changed={changed} />
              ) : page === "balances" ? (
                <PartyBalances role={session.user.role} changed={changed} />
              ) : page === "history" ? (
                <History />
              ) : page === "treasury" ? (
                <Treasury role={session.user.role} changed={changed}/>
              ) : page === "transaction-imports" ? (
                <TransactionImports revision={revision} canManage={["admin","finance"].includes(session.user.role)} onSaved={changed}/>
              ) : (
                <Settings
                  session={session}
                  onSession={updateSession}
                  changed={changed}
                />
              )}
            </main>
          </SessionRoleContext.Provider>
        </ResourceRevisionContext.Provider>
        <footer className="app-footer">
          <span>© {new Date().getFullYear()} Haruflow</span>
          <span>
            <span className="footer-dot" /> Dirancang untuk bisnis yang
            bertumbuh.
          </span>
        </footer>
      </div>
      {notice && (
        <div className="toast" role="status">
          <ShieldCheck size={18} />
          {notice}
          <button
            aria-label="Tutup pemberitahuan"
            onClick={() => setNotice("")}
          >
            <X size={15} />
          </button>
        </div>
      )}
      {help && (
        <Modal
          title="Selamat datang di Haruflow"
          description="Lima langkah untuk mulai mengalirkan bisnis Anda."
          onClose={() => setHelp(false)}
        >
          <div className="form-body">
            <ol className="guide-list">
              <li>
                <span>1</span>
                <div>
                  <strong>Tambahkan pelanggan</strong>
                  <p>Simpan nama dan kontak pada menu Pelanggan.</p>
                </div>
              </li>
              <li>
                <span>2</span>
                <div>
                  <strong>Siapkan katalog barang dan jasa</strong>
                  <p>
                    Tambah barang atau jasa pada Inventori. Catat penerimaan
                    stok hanya untuk barang.
                  </p>
                </div>
              </li>
              <li>
                <span>3</span>
                <div>
                  <strong>Buat dan konfirmasi pesanan</strong>
                  <p>Pilih pelanggan dan produk pada menu Penjualan.</p>
                </div>
              </li>
              <li>
                <span>4</span>
                <div>
                  <strong>Terbitkan invoice</strong>
                  <p>
                    Kirim barang sebelum menerbitkan invoice. Pesanan jasa dapat
                    langsung ditagihkan setelah konfirmasi. Stok dan jurnal
                    diperbarui otomatis.
                  </p>
                </div>
              </li>
              <li>
                <span>5</span>
                <div>
                  <strong>Catat pembayaran</strong>
                  <p>
                    Terima pembayaran pada tab Invoice. Pantau pembukuan pada
                    Akuntansi.
                  </p>
                </div>
              </li>
            </ol>
            <div className="info-box">
              <BellRing size={18} />
              <p>
                Hak akses tim menentukan langkah yang dapat diproses.
                Administrator dapat mengelola seluruh alur.
              </p>
            </div>
          </div>
          <div className="form-actions">
            <span className="guide-footer">
              <Command size={15} /> Satu alur, satu sumber data.
            </span>
            <button className="button primary" onClick={() => setHelp(false)}>
              Siap memulai <ArrowUpRight size={16} />
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
