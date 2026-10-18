# Haruflow Community

ERP gratis yang di-host sendiri, dengan frontend React/TypeScript dan backend Rust/Axum + PostgreSQL. Satu aplikasi Rust melayani UI dan API; instalasi default hanya menjalankan aplikasi dan PostgreSQL.

Versi pengembangan awal: satu perusahaan, satu gudang, mata uang IDR, tanpa pajak otomatis. Tidak memerlukan akun Haruflow Cloud atau layanan lisensi.

## Fitur

- Setup perusahaan dan admin, login lokal, role, pengelolaan pengguna, penggantian password, pencabutan sesi, dan audit aktivitas.
- Profil perusahaan, kontak, zona waktu sebelum posting pertama, serta awalan nomor pesanan dan invoice yang dapat dikonfigurasi. Mata uang tetap IDR.
- Pelanggan, produk, penerimaan stok, biaya rata-rata bergerak, penyesuaian, dan riwayat stok.
- Sales order → konfirmasi → pengiriman penuh untuk barang → invoice → pembayaran sebagian/penuh.
- Produk jasa/nonstok dapat ditagih langsung setelah konfirmasi, tanpa pergerakan stok; pesanan campuran tetap mengirim barang terlebih dahulu.
- Jurnal otomatis, jurnal manual seimbang, neraca saldo, dan penutupan periode.
- Saldo awal kas/aset/modal dan stok melalui draf, pemeriksaan, posting satu kali, serta pembalikan sebelum transaksi operasional dimulai. Lihat [panduan saldo awal](./docs/OPENING_BALANCES.md).
- Koreksi pembayaran, invoice, dan pengiriman melalui reversal dengan alasan; riwayat posted dipertahankan.
- Pembalikan jurnal manual dan penerimaan stok terbaru yang memiliki snapshot biaya; dokumen asal dan alasan koreksi tetap tersimpan. Lihat [panduan koreksi dan chart](./docs/CORRECTIONS_AND_CHARTS.md).
- Dashboard, pencarian, ekspor CSV, dan tampilan cetak invoice.
- Chart pendapatan/HPP dan mutasi kas dengan pilihan 3/6/12 bulan, serta posisi invoice terbayar/piutang saat ini.

Master supplier serta PO draf/edit/konfirmasi/pembatalan tersedia pada menu Pembelian. PO mencatat rencana pembelian dan belum memengaruhi stok, utang, atau kas; lihat [panduan pembelian](./docs/PURCHASING.md).

Penerimaan dari PO, tagihan/utang supplier, retur komersial, pengiriman sebagian, multicurrency, multi-company, payroll, dan integrasi pajak belum tersedia. Detail lingkup dan tahap selanjutnya ada di [rencana Community](./docs/COMMUNITY_PLAN.md).

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

Backup menggunakan `pg_dump` dalam container PostgreSQL. Restore mengganti isi database, menghentikan aplikasi, dan memakai satu transaksi. Aplikasi tetap berhenti setelah restore untuk memberi kesempatan verifikasi versi. Cadangkan `.env` dengan enkripsi secara terpisah dan uji restore pada instalasi lain. Lihat [runbook operasi](./docs/OPERATIONS.md).

## Struktur dan dokumen

- `apps/web`: React, TypeScript, Vite, dan UI Bahasa Indonesia.
- `crates/server`: server, autentikasi, domain, transaksi bisnis, dan pengujian.
- `migrations`: migrasi schema PostgreSQL yang berurutan.
- `deploy/community`: Docker Compose untuk instalasi mandiri.
- [Rencana Community](./docs/COMMUNITY_PLAN.md), [proposal arsitektur](./haruflow.md), dan [operasi](./docs/OPERATIONS.md).
- [Status implementasi dan batas baseline](./docs/IMPLEMENTATION_STATUS.md).

## License

Haruflow Community is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0-only). See [LICENSE](./LICENSE).
