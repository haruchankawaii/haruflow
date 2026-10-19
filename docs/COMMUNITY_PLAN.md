# Rencana Haruflow Community

Status: rencana acuan; baseline aplikasi Community sudah diimplementasikan. Lihat [status implementasi](./IMPLEMENTATION_STATUS.md) untuk hasil dan pekerjaan tersisa.
Tanggal pembaruan: 19 Oktober 2026.
Dasar: `haruflow.md` dan kondisi repository yang saat ini berisi README serta lisensi AGPL-3.0-only.

## 1. Arah produk

Haruflow Community adalah ERP gratis yang dapat dijalankan sendiri oleh usaha kecil. Sasaran awal adalah usaha dagang dengan satu perusahaan, satu gudang, dan satu mata uang operasional. Asumsi ini membatasi kompleksitas versi pertama dan dapat ditinjau sebelum implementasi domain.

Hasil yang dituju untuk v0.1: pengguna dapat memasang aplikasi, membuat perusahaan dan pengguna, memasukkan saldo awal, mencatat penjualan sampai pembayaran, lalu melihat stok dan jurnal yang saling cocok.

Community tidak memerlukan akun Haruflow, layanan lisensi, atau koneksi ke Haruflow Cloud. Tidak ada batas pengguna berbasis lisensi; kapasitas mengikuti perangkat dan konfigurasi pemilik instalasi. Lisensi mengikuti `LICENSE` yang sudah ada di repository.

Pemilik instalasi mengelola server, domain/TLS, backup, dan upgrade. Haruflow menyediakan aplikasi, image rilis, panduan, serta perintah operasional. Dukungan awal melalui dokumentasi dan issue repository; belum ada SLA.

## 2. Batas versi pertama

### Termasuk dalam v0.1

- Setup perusahaan: nama, zona waktu, mata uang, periode pembukuan, dan penomoran dokumen.
- Login lokal, sesi yang dapat dicabut, logout, pengelolaan pengguna, dan reset password melalui perintah admin lokal.
- Permission di Rust dengan role awal Administrator, Sales, Warehouse, Finance, dan Viewer. Permission rinci mengikuti aksi, bukan sekadar nama role.
- Master data pelanggan, produk stok/nonstok, satuan dasar, gudang utama, dan bagan akun.
- Saldo awal stok dan akuntansi melalui proses draft, validasi, dan posting yang dapat diaudit.
- Sales order, pengiriman penuh, invoice dari pengiriman, dan penerimaan pembayaran; pembayaran sebagian diperbolehkan.
- Penerimaan stok manual dengan biaya per unit, penyesuaian stok, dan riwayat pergerakan stok.
- Biaya persediaan dengan moving weighted average; pengiriman mencatat biaya historisnya untuk pembatalan atau penelusuran.
- Jurnal berpasangan dari invoice, pengiriman, penerimaan stok, penyesuaian, dan pembayaran; jurnal manual untuk Finance.
- Daftar stok, kartu stok, piutang pelanggan, jurnal umum, buku besar, dan neraca saldo.
- Audit aktivitas penting, pencarian, filter, pagination, ekspor CSV, serta tampilan cetak invoice melalui browser.
- Docker Compose, health check, migrasi database, panduan backup/restore, dan prosedur upgrade.

### Ditunda setelah v0.1

- Purchasing lengkap, pembayaran supplier, penerimaan/tagihan sebagian, dan retur barang komersial. Supplier, PO, penerimaan penuh, tagihan, serta posisi utang/jatuh tempo sudah ditambahkan bertahap setelah baseline; lihat catatan pembaruan di akhir dokumen.
- Pengiriman sebagian, backorder, konversi satuan, transfer antargudang, lot, serial number, dan expiry.
- Multi-company, multicurrency, konsolidasi, dan pembatasan akses lintas cabang/gudang.
- HR, payroll, manufacturing, POS offline, dan aplikasi mobile native.
- Perhitungan pajak otomatis, pelaporan pajak, e-Faktur, integrasi bank, dan rekonsiliasi bank otomatis.
- SSO, marketplace plugin, kustomisasi workflow, dan konektor eksternal.
- Email, realtime, lampiran dokumen, dan object storage; tidak diperlukan untuk alur minimum pertama.

Community tetap dapat menerima fitur-fitur lanjutan tersebut di kemudian hari. Batas di atas adalah urutan pengembangan, bukan keputusan permanen mengenai pemisahan lisensi fitur Enterprise.

## 3. Arsitektur yang akan dipakai

- Frontend: React + TypeScript + Vite, memakai Bun untuk package management, build, dan pengujian frontend. Ini pilihan awal rencana; framework UI belum ditentukan dalam dokumen sumber.
- Backend: Rust + Axum + Tokio, dengan Tower/tower-http untuk middleware standar.
- Persistence: PostgreSQL + SQLx, dengan migrasi yang disimpan di repository.
- Kontrak API: HTTP/JSON + OpenAPI; client TypeScript dihasilkan dari kontrak yang sama.
- Struktur: modular monolith. Modul berbagi satu aplikasi dan transaksi database, dengan batas ownership data yang jelas.
- Runtime default: dua container, `app` dan `postgres`. Aplikasi Rust melayani API serta frontend hasil build Bun.
- Pekerjaan latar belakang: antrean PostgreSQL dengan eksekutor dalam proses aplikasi pada konfigurasi sederhana. Mode worker terpisah memakai codebase/image yang sama ketika diperlukan.
- Elysia tidak menjadi dependensi runtime Community v0.1. Redis, Kubernetes, dan message broker juga tidak dibutuhkan.

Versi dependensi akan dipilih dan dikunci pada tahap fondasi. Tidak menggunakan tag `latest` untuk image rilis yang didokumentasikan.

Struktur repository yang diusulkan:

```text
apps/web/                   frontend React
crates/haruflow/             binary, routing, konfigurasi, lifecycle
crates/domain/               aturan bisnis, tipe nilai, state transition
crates/application/          use case, permission, koordinasi transaksi
crates/infrastructure/       SQLx, repository, auth adapters, jobs
packages/api-client/         client TypeScript yang dihasilkan
migrations/                 schema PostgreSQL
deploy/community/           Compose dan contoh konfigurasi
docs/                       panduan pengguna, arsitektur, operasi
```

Modul awal: identity, company, master-data, sales, inventory, accounting, audit, dan jobs. Gunakan struktur sederhana dahulu; crate tambahan dibuat hanya ketika membantu batas dependensi atau pengujian.

## 4. Aturan domain yang wajib dipenuhi

### Transaksi dan angka

- Semua mutasi bisnis melalui Rust; browser dan adapter tidak menulis langsung ke database.
- Gunakan tipe decimal untuk nominal, harga, biaya, dan kuantitas. Hindari floating point untuk perhitungan bisnis.
- Tetapkan skala, pembulatan mata uang, dan pembulatan biaya per unit secara eksplisit sebelum membangun posting.
- Satu aksi posting menyimpan dokumen, ledger, saldo, audit, serta event yang relevan dalam satu transaksi.
- Terapkan constraint database, penguncian baris, dan retry terbatas untuk mutasi stok serta penomoran yang bersamaan.
- Endpoint posting dan pembayaran mendukung idempotency key. Pengulangan request tidak boleh membuat transaksi baru.

### Dokumen, stok, dan akuntansi

- Dokumen mengikuti state transition eksplisit, misalnya draft → confirmed → posted atau cancelled. Penolakan transisi diuji di backend.
- Stok tidak boleh negatif pada v0.1. Sales order belum mereservasi stok; ketersediaan diperiksa dan dikunci saat pengiriman diposting.
- Pengiriman v0.1 harus memenuhi seluruh order; invoice dibuat satu kali untuk pengiriman tersebut. Tidak ada invoice atas kuantitas yang belum dikirim.
- Penerimaan stok memperbarui kuantitas, nilai persediaan, dan biaya rata-rata secara atomik. Kebijakan saat saldo nol serta pembulatan harus mempunyai contoh uji.
- Setiap jurnal posted memiliki total debit sama dengan kredit. Jurnal pada periode tertutup ditolak.
- Invoice mencatat piutang dan pendapatan; pengiriman mencatat HPP dan pengurangan persediaan; pembayaran mencatat kas/bank dan pengurangan piutang.
- Penerimaan dan penyesuaian stok memakai akun lawan yang dikonfigurasi. Penambahan stok tidak boleh diam-diam menghilangkan pencatatan nilai persediaan.
- Pembayaran tidak boleh melampaui sisa piutang pada v0.1; uang muka dan overpayment ditunda.
- Data posted tidak diedit atau dihapus. Koreksi menggunakan reversal yang merujuk transaksi asal, dengan permission dan alasan.
- Pembatalan dokumen berantai ditolak bila masih memiliki transaksi turunan aktif. Reversal pembayaran, invoice, lalu pengiriman dilakukan berurutan; reversal stok juga harus menjaga saldo valid.
- Saldo awal hanya dapat diposting sekali per konteks pembukaan; koreksi memakai reversal. Nilai awal stok harus cocok dengan akun persediaan.
- Laporan berasal dari ledger/dokumen posted, memakai batas tanggal dan zona waktu yang konsisten.

## 5. Keamanan dan operasi minimum

- Password disimpan dengan Argon2id; akun admin pertama dibuat melalui setup token sekali pakai atau CLI lokal, tanpa password default.
- Sesi memakai cookie Secure pada HTTPS, HttpOnly, dan SameSite, disertai proteksi CSRF untuk mutasi berbasis cookie.
- Permission diperiksa di setiap endpoint; pengguna nonaktif dan sesi yang dicabut segera kehilangan akses.
- Batasi percobaan login dan ukuran request; query memakai bound parameters; error publik tidak membocorkan detail internal.
- Secrets berasal dari konfigurasi lokal atau mounted secrets; contoh konfigurasi tidak memuat kredensial nyata.
- PostgreSQL tidak dipublikasikan ke internet oleh Compose default. Dokumentasikan reverse proxy/TLS untuk akses dari luar mesin.
- Log terstruktur membawa request ID dan menghindari password, token, cookie, serta isi sensitif dokumen.
- Backup mencakup database, versi aplikasi, dan konfigurasi yang diperlukan. Secrets dicadangkan dengan perlindungan tersendiri.
- Restore diuji ke instalasi kosong; panduan menjelaskan penghentian penulisan, validasi hasil, dan aktivasi kembali.
- Migrasi dijalankan sekali melalui langkah upgrade yang eksplisit, dengan pemeriksaan versi dan backup sebelum perubahan.
- Rollback aplikasi hanya diperbolehkan jika skema kompatibel. Jika tidak, gunakan restore backup sesuai runbook dan jelaskan konsekuensi kehilangan perubahan setelah backup.
- Telemetri keluar ke Haruflow tidak diaktifkan secara default.

## 6. Tahapan implementasi

Estimasi di bawah adalah kisaran awal untuk satu developer, bukan janji tanggal rilis. Total sekitar 17–24 minggu kerja efektif; kalender bisa lebih panjang. Validasi ulang setelah tahap 2 karena pengalaman Rust, keputusan domain, dan kapasitas kerja belum diketahui.

### Tahap 0 — Finalisasi lingkup dan contoh bisnis (1 minggu)

Hasil:
- ADR untuk stack, runtime Community, batas MVP, dan format nilai decimal.
- Daftar permission, state transition, mapping akun, serta aturan pembatalan.
- Skenario contoh yang dapat diperiksa manual, termasuk saldo awal dan pembulatan biaya stok.

Selesai ketika: satu contoh transaksi end-to-end mempunyai hasil stok, piutang, dan jurnal yang disepakati; fitur tertunda tercatat jelas.

### Tahap 1 — Fondasi aplikasi dan instalasi (2–3 minggu)

Hasil:
- Rust workspace dan frontend Bun/React, routing API, konfigurasi tervalidasi, SQLx, migrasi, dan health endpoints.
- Frontend statis disajikan oleh Rust; Compose development serta rancangan image rilis.
- CI untuk formatting, lint/typecheck, unit test, dan integrasi PostgreSQL.

Selesai ketika: checkout baru dapat dibangun dan dijalankan mengikuti README; database bersih dapat dimigrasikan; kegagalan konfigurasi menghasilkan pesan yang jelas.

### Tahap 2 — Identitas, perusahaan, dan master data (2–3 minggu)

Hasil:
- Setup admin/perusahaan, login/logout, session revocation, permission, dan audit dasar.
- CRUD pelanggan, produk, satuan, gudang utama, periode, dan bagan akun.
- Komponen UI bersama untuk form, tabel, pagination, validasi, dan penanganan error.

Selesai ketika: role yang tidak berwenang gagal mengakses API meskipun memanggil endpoint langsung; data master memiliki aturan unik dan tidak bisa dihapus bila sudah digunakan.

### Tahap 3 — Mesin ledger dan stok (3–4 minggu)

Hasil:
- Infrastruktur transaksi, idempotensi, penomoran, dan audit atomik.
- Mesin jurnal seimbang, posting/reversal, periode tutup, serta saldo awal.
- Movement ledger, penerimaan, adjustment, moving weighted average, dan kartu stok.

Selesai ketika: tes concurrent mutation tidak menghasilkan stok negatif atau saldo hilang; saldo stok cocok dengan movement ledger; jurnal tidak seimbang dan posting pada periode tertutup selalu ditolak.

### Tahap 4 — Order dan pengiriman (2–3 minggu)

Hasil:
- Sales order draft/confirm/cancel serta pengiriman penuh.
- Posting pengiriman mengurangi stok dan membukukan HPP dengan biaya historis.
- Pemeriksaan stok, idempotensi pengiriman, dan pembatasan pembatalan berantai.

Selesai ketika: dua pengiriman yang berebut stok terakhir menghasilkan hanya transaksi yang valid; retry tidak menggandakan pengiriman, movement, atau jurnal.

### Tahap 5 — Invoice, pembayaran, dan laporan (3–4 minggu)

Hasil:
- Invoice dari pengiriman, pembayaran sebagian/penuh, dan reversal yang terhubung.
- Piutang, jurnal umum, buku besar, neraca saldo, pencarian/filter, CSV, dan cetak invoice.
- Uji integrasi seluruh alur bisnis melalui API dan pengujian browser untuk alur pengguna utama.

Selesai ketika: contoh bisnis pada bagian 7 lulus; piutang cocok dengan ledger akuntansi; dokumen tidak dapat ditagih atau dibayar dua kali melalui request berulang.

### Tahap 6 — Backup, upgrade, dan ketahanan (2–3 minggu)

Hasil:
- CLI backup/restore atau wrapper terdokumentasi, migrasi upgrade, graceful shutdown, dan observability dasar.
- Pengujian kehilangan koneksi database, restart saat posting, timeout, dan dua instance aplikasi dengan job executor.
- Jika ada pekerjaan latar belakang pada rilis ini: claim/lease, retry terbatas, pencatatan kegagalan, dan pemulihan job setelah crash.
- Pemeriksaan keamanan endpoint, audit dependensi, serta uji beban pada perangkat yang dicatat spesifikasinya.

Selesai ketika: backup dapat dipulihkan pada instalasi baru, jumlah/saldo bisnis cocok, dan upgrade dari kandidat rilis sebelumnya berhasil dengan data tetap konsisten.

### Tahap 7 — Pilot dan rilis Community v0.1 (2–3 minggu)

Hasil:
- Panduan instalasi, penggunaan dasar, akses jaringan/TLS, backup, upgrade, troubleshooting, dan pelaporan bug.
- Image dengan tag versi tetap, release notes, checksum artefak, serta contoh konfigurasi.
- Pilot pada instalasi baru dengan pengguna sasaran; masalah yang merusak transaksi, hak akses, atau pemulihan data diselesaikan sebelum rilis.

Selesai ketika: pengguna pilot dapat menyelesaikan alur utama berdasarkan dokumentasi, seluruh kriteria rilis terpenuhi, dan batas fitur v0.1 dinyatakan pada release notes.

## 7. Skenario penerimaan end-to-end

Gunakan skenario tanpa pajak untuk memeriksa alur minimum:

1. Admin membuat perusahaan IDR, gudang utama, akun kas/bank, piutang, persediaan, modal awal, pendapatan, dan HPP.
2. Finance memposting saldo awal 10 unit barang dengan biaya Rp50.000/unit: debit persediaan Rp500.000 dan kredit modal awal Rp500.000.
3. Sales membuat dan mengonfirmasi order 2 unit dengan harga Rp80.000/unit.
4. Warehouse memposting pengiriman penuh. Stok menjadi 8 unit; debit HPP Rp100.000 dan kredit persediaan Rp100.000.
5. Finance memposting invoice Rp160.000: debit piutang dan kredit pendapatan dengan nilai yang sama.
6. Finance mencatat pembayaran Rp60.000 lalu Rp100.000: kas bertambah Rp160.000, piutang menjadi nol, invoice lunas.
7. Laporan menunjukkan persediaan Rp400.000, kas Rp160.000, pendapatan Rp160.000, dan HPP Rp100.000. Total debit dan kredit neraca saldo masing-masing Rp660.000.
8. Request posting/pembayaran yang diulang dengan idempotency key sama mengembalikan hasil transaksi asal, tanpa menambah dokumen atau saldo.
9. Percobaan pengiriman melebihi stok dan posting ke periode tertutup ditolak tanpa mutasi parsial.
10. Restore backup ke instance baru menghasilkan dokumen, audit, stok, dan neraca saldo yang sama.

Tambahkan skenario uji tersendiri untuk penerimaan berbiaya berbeda, pembulatan biaya rata-rata, saldo nol, reversal, request bersamaan, dan crash sebelum/sesudah commit.

## 8. Kriteria rilis dan prioritas berikutnya

v0.1 dapat dirilis jika:

- Alur penerimaan end-to-end, tes invariant, permission, concurrency, backup/restore, dan upgrade lulus.
- Tidak ada bug diketahui yang menyebabkan transaksi ganda, jurnal tidak seimbang, kebocoran akses, atau kehilangan data tanpa penjelasan pemulihan.
- Instalasi bersih dan restart mempertahankan data; tidak ada kredensial default atau keharusan mengedit source code.
- Dokumentasi menjelaskan batas produk, konfigurasi produksi, pemulihan, dan pengelolaan secrets.
- Performa diuji dengan dataset serta hardware yang dilaporkan. Target sementara P95 baca <300 ms dan posting <500 ms dipakai sebagai sasaran pengukuran, bukan SLA atau penghalang rilis tanpa melihat workload.

Urutan pengembangan purchasing saat ini: pembayaran supplier sebagian/penuh dan koreksinya → umur utang dengan kelompok hari serta chart pembayaran → retur serta partial delivery → multiwarehouse → lampiran dan impor CSV dengan dry run → laporan keuangan lebih lengkap. Supplier, PO, penerimaan penuh, tagihan, dan ringkasan jatuh tempo telah diimplementasikan sebagai perluasan baseline. Urutan akhir mengikuti kebutuhan pengguna pilot.

## 9. Pekerjaan pertama yang siap dieksekusi

1. Dokumentasikan keputusan lingkup dan contoh posting pada ADR.
2. Buat Rust workspace, frontend React/TypeScript dengan Bun, dan migrasi awal PostgreSQL.
3. Buat Compose lokal dengan app + PostgreSQL serta health checks.
4. Tambahkan CI build/lint/test dan cara menjalankan aplikasi pada README.
5. Implementasikan setup admin/perusahaan sebagai fitur vertikal pertama, lalu login dan permission.

Dokumen ini tetap menjadi rencana acuan. Implementasi aktual, cakupan pengujian, dan batas baseline dicatat terpisah dalam [status implementasi](./IMPLEMENTATION_STATUS.md).

Pembaruan bertahap 19 Oktober 2026: supplier, PO, penerimaan penuh, koreksi penerimaan aman, tagihan supplier/jatuh tempo, serta chart status PO dan utang sudah diimplementasikan sebagai perluasan Community. Finance/Admin dapat mencatat satu tagihan penuh per penerimaan aktif, membaliknya dengan alasan, lalu menerbitkan pengganti tanpa menghapus sejarah. Tagihan memakai snapshot penerimaan dan jurnal debit barang diterima belum ditagih/kredit utang supplier, tanpa perubahan stok atau kas. Tanggal invoice tidak melakukan backdating jurnal; periode saat ini harus terbuka. Tagihan aktif mencegah koreksi penerimaannya sampai tagihan dibalik terlebih dahulu. Pembayaran sebagian/penuh, koreksi pembayaran, umur utang per kelompok hari, dan chart pembayaran tetap pekerjaan berikutnya. Baca [panduan pembelian](./PURCHASING.md) untuk lingkup dan batas tahap ini.
