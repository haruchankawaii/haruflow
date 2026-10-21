# Haruflow Community

ERP gratis yang di-host sendiri, dengan frontend React/TypeScript dan backend Rust/Axum + PostgreSQL. Satu aplikasi Rust melayani UI dan API; instalasi default hanya menjalankan aplikasi dan PostgreSQL.

Versi pengembangan awal: satu perusahaan, satu gudang, mata uang IDR, tanpa pajak otomatis. Tidak memerlukan akun Haruflow Cloud atau layanan lisensi.

## Fitur

- Setup perusahaan dan admin, login lokal, role, pengelolaan pengguna, penggantian password, pencabutan sesi, dan audit aktivitas.
- Maksimal 10 akun aktif per instalasi, termasuk administrator. Akun nonaktif menyimpan riwayat dan melepaskan slot; batas diperiksa di backend/database. Lihat [batas pengguna dan perlindungan kode](./docs/USER_LIMIT.md).
- Profil perusahaan, kontak, zona waktu sebelum posting pertama, serta awalan nomor pesanan dan invoice yang dapat dikonfigurasi. Mata uang tetap IDR.
- Pelanggan, produk, penerimaan stok, biaya rata-rata bergerak, penyesuaian, dan riwayat stok.
- Sales order → konfirmasi → pengiriman sebagian/penuh untuk barang → invoice sebagian/penuh → pembayaran sebagian/penuh.
- Produk jasa/nonstok dapat ditagih langsung setelah konfirmasi, tanpa pergerakan stok; pesanan campuran tetap mengirim barang terlebih dahulu.
- Jurnal otomatis, jurnal manual seimbang, neraca saldo, dan penutupan periode.
- Saldo awal kas/aset/modal, stok, piutang/utang per dokumen, serta sisa uang muka/kredit pelanggan dan supplier melalui draf seimbang. Rincian pihak dapat diisi manual atau melalui CSV dengan preview; posting tetap satu kali sebelum operasi. Lihat [panduan saldo awal](./docs/OPENING_BALANCES.md).
- Buku pembantu **Saldo pihak** untuk piutang/utang, uang muka, kredit, alokasi lintas tagihan pihak yang sama, serta refund tertunda/sebagian. Chart aging, posisi per tanggal, rekonsiliasi ke ledger, CSV, dan cetak tersedia. Lihat [panduan saldo pihak](./docs/PARTY_BALANCES.md).
- Koreksi pembayaran, invoice, dan pengiriman melalui reversal dengan alasan; riwayat posted dipertahankan.
- Pembalikan jurnal manual dan penerimaan stok terbaru yang memiliki snapshot biaya; dokumen asal dan alasan koreksi tetap tersimpan. Lihat [panduan koreksi dan chart](./docs/CORRECTIONS_AND_CHARTS.md).
- Dashboard, pencarian, ekspor CSV, dan tampilan cetak invoice. **Riwayat lengkap** menyediakan pencarian/filter dan pagination server untuk data di luar daftar ringkas; ekspor mengambil seluruh hasil filter sampai 10.000 baris. Lihat [panduan riwayat](./docs/HISTORY.md).
- Chart pendapatan/HPP dan mutasi kas dengan pilihan 3/6/12 bulan, serta posisi invoice terbayar/piutang saat ini.
- Laporan laba rugi, neraca, dan arus kas dengan rentang tanggal, chart, CSV, rincian akun/jurnal, serta klasifikasi arus kas beralasan. Lihat [panduan laporan](./docs/FINANCIAL_REPORTS.md).
- Retur pelanggan dan supplier sebagian/penuh, kredit tagihan, pilihan refund aktual/simpan kredit/menunggu refund, jurnal biaya persediaan, serta pembalikan beralasan. Lihat [panduan retur](./docs/RETURNS.md).
- Impor pelanggan, supplier, dan produk dari CSV UTF-8, dengan template, preview wajib, validasi per baris, perlindungan duplikasi, serta penyimpanan atomik. Lihat [panduan impor](./docs/IMPORTS.md).
- Lampiran bukti PDF/PNG/JPEG pada dokumen dan pembayaran, unduhan setelah login, arsip/pemulihan beralasan, serta riwayat. Isi file ikut backup database. Lihat [panduan lampiran](./docs/ATTACHMENTS.md).

Menu Pembelian menyediakan supplier, PO draf/edit/konfirmasi/pembatalan, penerimaan dan penagihan sebagian/penuh, pembayaran supplier, koreksi beralasan, serta chart status PO, umur utang, dan pembayaran. Penerimaan memperbarui stok dan jurnal persediaan/barang diterima belum ditagih; tagihan memindahkan kewajiban tersebut ke utang supplier. Pembayaran mengurangi kas serta utang. Jumlah tersisa tetap terlacak; lihat [panduan pembelian](./docs/PURCHASING.md).

Variasi harga invoice pembelian, pembelian jasa, ongkos/pajak otomatis, multicurrency, multi-company, payroll, serta integrasi bank/pajak belum tersedia. Arus kas memakai akun Kas 1100; satu jurnal yang mencampur aktivitas dapat diklasifikasikan secara manual sebagai satu kelompok. Aplikasi mencatat perpindahan uang aktual, sedangkan transfer bank dilakukan di luar aplikasi. Detail lingkup ada di [status implementasi](./docs/IMPLEMENTATION_STATUS.md).

## Menjalankan dengan Docker Compose

Prasyarat: Docker Engine/Desktop dengan Compose v2, serta Node.js 22+ atau Bun untuk membuat konfigurasi lokal dan menjalankan utilitas backup.

Jalankan dari root repository:

```sh
node scripts/configure.mjs
docker compose --env-file .env -f deploy/community/compose.yml build app
docker compose --env-file .env -f deploy/community/compose.yml up -d postgres
docker compose --env-file .env -f deploy/community/compose.yml run --rm migrate
docker compose --env-file .env -f deploy/community/compose.yml up -d app
```

Buka http://localhost:8080. Salin `HARUFLOW_SETUP_TOKEN` dari `.env` ke formulir setup, lalu buat perusahaan dan admin pertama dengan password minimal 12 karakter. Token setup tidak dapat digunakan untuk membuat perusahaan kedua. Script konfigurasi tidak menimpa `.env` yang sudah ada dan tidak mencetak secrets.

Port aplikasi hanya dipublikasikan pada loopback; PostgreSQL tidak dipublikasikan. Untuk akses jaringan/produksi gunakan reverse proxy HTTPS, atur `HARUFLOW_SECURE_COOKIES=true`, dan ikuti [panduan operasi](./docs/OPERATIONS.md).

## Pengembangan lokal

Prasyarat: Rust 1.98.1, Bun 1.3.14, dan PostgreSQL 14 atau lebih baru. CI juga menguji PostgreSQL 18.1. Buat database dan user pengembangan tersendiri; jangan gunakan database produksi untuk tes.

```sh
node scripts/configure.mjs
# Sesuaikan DATABASE_URL pada .env dengan PostgreSQL lokal.
bun install --cwd apps/web
bun run build:web
cargo run --locked -- migrate
cargo run --locked -- serve
```

Aplikasi tersedia pada http://localhost:8080. Saat mengerjakan UI, jalankan `bun run dev:web` di terminal lain; Vite mem-proxy `/api` ke server Rust. Semua perubahan bisnis tetap disimpan di PostgreSQL.

## Pengujian

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
# Set DATABASE_URL di environment shell ke server PostgreSQL untuk tes.
# User tes memerlukan CREATEDB; sqlx membuat database terpisah per integration test.
cargo test --locked -- --test-threads=1
bun run --cwd apps/web test
bun run build:web
node --test scripts/backup-manifest.test.mjs
```

`.env` hanya dibaca otomatis oleh executable, sehingga untuk `cargo test` tetap set `DATABASE_URL` di environment shell. Gunakan format `postgres://user:password@127.0.0.1:5432/postgres`; tes tidak memakai database aplikasi sebagai tempat fixture. Database tes yang gagal mungkin disimpan SQLx untuk investigasi.

Tes mencakup perhitungan decimal, setup bersamaan, permission dan pencabutan sesi, alur ERP, idempotensi, perebutan stok, jurnal seimbang, ledger yang tidak bisa diedit, reversal, dan periode tutup. Workflow GitHub Actions menjalankan pemeriksaan backend, frontend, serta build container.

## Pemulihan administrator

Jika seluruh akses admin hilang karena lupa password, operator lokal dapat memulihkan administrator aktif dari terminal:

```sh
docker compose --env-file .env -f deploy/community/compose.yml exec app haruflow admin reset-password admin@example.com
# Native: cargo run --locked -- admin reset-password admin@example.com
```

Password diminta dua kali melalui prompt tersembunyi. Perintah ini memerlukan akses server/database, mencabut seluruh sesi admin tersebut, dan tidak dapat menaikkan role pengguna biasa atau mengaktifkan akun nonaktif. Untuk otomasi yang memakai input terlindungi tersedia `--password-stdin`; jangan menulis password sebagai argumen command atau ke shell history.

## Backup dan restore data

```sh
node scripts/database.mjs backup
node scripts/database.mjs restore backups/nama-file.backup --confirm-restore
```

Backup menggunakan `pg_dump` dalam container PostgreSQL, disertai manifest versi aplikasi/schema, ukuran, dan SHA-256. Utilitas memeriksa manifest serta arsip sebelum restore. Restore mengganti isi database, menghentikan aplikasi, dan memakai satu transaksi. Aplikasi tetap berhenti setelah restore untuk memberi kesempatan verifikasi versi dan migrasi eksplisit. Cadangkan `.env` dengan enkripsi secara terpisah dan uji restore pada instalasi lain. Lihat [runbook operasi](./docs/OPERATIONS.md).

Perintah `haruflow version` dan endpoint `/version` menampilkan identitas build tanpa konfigurasi rahasia. Readiness memeriksa kecocokan migrasi sebelum aplikasi dinyatakan siap. Panduan [kandidat rilis](./docs/RELEASE_CANDIDATE.md) menjelaskan pembuatan paket berisi binary, web, source yang sesuai, dan checksum; workflow kandidat tidak menerbitkan tag atau image registry otomatis. Pilot usaha nyata tetap memerlukan hasil penggunaan yang dicatat.

## Struktur dan dokumen

- `apps/web`: React, TypeScript, Vite, dan UI Bahasa Indonesia.
- `crates/server`: server, autentikasi, domain, transaksi bisnis, dan pengujian.
- `migrations`: migrasi schema PostgreSQL yang berurutan.
- `deploy/community`: Docker Compose untuk instalasi mandiri.
- [Rencana Community](./docs/COMMUNITY_PLAN.md), [proposal arsitektur](./haruflow.md), dan [operasi](./docs/OPERATIONS.md).
- [Status implementasi dan batas fitur](./docs/IMPLEMENTATION_STATUS.md), [saldo pihak](./docs/PARTY_BALANCES.md), [riwayat lengkap](./docs/HISTORY.md), dan [kandidat rilis](./docs/RELEASE_CANDIDATE.md).

## License

Haruflow Community is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0-only). See [LICENSE](./LICENSE).
