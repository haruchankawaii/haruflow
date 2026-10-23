<p align="center">
  <img src="docs/images/hero.svg" alt="Haruflow Community — kenali usaha, rapikan alurnya" width="1280" />
</p>

<p align="center">
  <a href="LICENSE"><img alt="Lisensi AGPL-3.0-only" src="https://img.shields.io/badge/License-AGPL--3.0--only-295d46?style=flat-square&amp;labelColor=193d31" /></a>
  <img alt="Community: 10 akun aktif" src="https://img.shields.io/badge/Community-10%20akun%20aktif-d4ecba?style=flat-square&amp;labelColor=193d31" />
  <img alt="Satu perusahaan, mata uang IDR" src="https://img.shields.io/badge/Satu%20perusahaan-IDR-d4ecba?style=flat-square&amp;labelColor=193d31" />
  <a href="docs/MCP.md"><img alt="MCP lokal untuk Codex" src="https://img.shields.io/badge/Codex-MCP%20baca-295d46?style=flat-square&amp;labelColor=193d31" /></a>
</p>

<p align="center">
  <a href="#mulai">Mulai</a> ·
  <a href="#ruang-kerja">Ruang kerja</a> ·
  <a href="docs/GUIDE.md">Panduan bergambar</a> ·
  <a href="docs/MCP.md">Hubungkan Codex</a> ·
  <a href="#untuk-pengembang">Pengembang</a>
</p>

**Haruflow Community adalah ERP gratis untuk usaha kecil yang dijalankan sendiri.** Catat penjualan dan pembelian, kelola stok serta kas, lalu telusuri laporan ke transaksi sumbernya. Satu perusahaan, IDR, maksimal 10 akun aktif termasuk admin. Tidak memerlukan akun Haruflow Cloud atau server lisensi.

<p align="center">
  <img src="docs/images/dashboard.png" alt="Tampilan asli dashboard Haruflow dengan ringkasan dan chart; menggunakan data simulasi" width="1280" />
  <br /><sub>Tampilan aplikasi dengan data simulasi. Angka pada gambar bukan data usaha pengguna.</sub>
</p>

## Mulai

**Windows:** ekstrak paket yang sudah diverifikasi dan jalankan wizard:

```powershell
./Setup-Haruflow.ps1
```

Wizard menemukan binary PostgreSQL yang terpasang dan dapat menyiapkan database privat di folder Haruflow, atau memakai koneksi yang sudah tersedia. Pilih jam backup dan apakah aplikasi dimulai saat akun Windows masuk; tidak ada jam yang ditebak atau password Windows yang disimpan. Wizard tidak mengunduh PostgreSQL otomatis atau membuat service administrator. Selanjutnya buat perusahaan/admin pertama dengan kode setup pribadi. [Instalasi Windows](docs/WINDOWS.md) · [Panduan bergambar](docs/GUIDE.md).

**Docker Compose:** jalankan dari root repository; perlu Docker dengan Compose v2 serta Node.js 22+ atau Bun untuk utilitas konfigurasi.

```sh
node scripts/configure.mjs
docker compose --env-file .env -f deploy/community/compose.yml build app
docker compose --env-file .env -f deploy/community/compose.yml up -d postgres
docker compose --env-file .env -f deploy/community/compose.yml run --rm migrate
docker compose --env-file .env -f deploy/community/compose.yml up -d app
```

Buka [localhost:8080](http://localhost:8080). Gunakan `HARUFLOW_SETUP_TOKEN` dari `.env` secara pribadi untuk setup pertama. Konfigurasi tidak menimpa `.env` yang sudah ada atau mencetak secrets. Migrasi merupakan langkah eksplisit; server memeriksa kecocokan schema sebelum siap.

<p>
  <a href="docs/GUIDE.md"><strong>→ Panduan pengguna</strong></a> &nbsp;
  <a href="docs/WINDOWS_BACKUP.md"><strong>→ Backup Windows</strong></a> &nbsp;
  <a href="docs/OPERATIONS.md"><strong>→ Runbook Docker &amp; server</strong></a>
</p>

## Ruang kerja

- **Jual dan beli.** Pesanan barang/jasa, pengiriman/penerimaan bertahap, invoice/tagihan, pembayaran sebagian, retur dan koreksi dengan riwayat. Biaya pembelian yang memenuhi syarat dapat dialokasikan ke persediaan/HPP setelah preview.
- **Stok yang dapat ditelusuri.** Beberapa gudang, transfer, satuan, lot/seri, kedaluwarsa dan reservasi; jumlah serta nilai terkait pembukuan.
- **Kas dan saldo pihak.** Rekening kas/bank, transfer internal, rekonsiliasi CSV bank, piutang/utang, uang muka, kredit dan refund.
- **Pembukuan dan laporan.** Saldo awal, jurnal berpasangan, laba rugi, neraca, arus kas, chart, periode serta tutup tahun.
- **Data dan bukti.** Impor CSV dengan preview, transaksi historis, PDF/PNG/JPEG, pencarian dan riwayat terpaginasikan.
- **Akses dan insight.** Peran pengguna, audit, batas akun serta enam tool MCP baca untuk Codex dengan izin dan kunci pribadi.

<p align="center"><img src="docs/images/workflow.svg" alt="Alur penjualan dan pembelian hingga stok, jurnal, laporan dan insight Codex" width="1280" /></p>

Lihat [cakupan Community](docs/COMMUNITY_SMALL_BUSINESS.md) untuk aturan dan batas setiap alur. Pembayaran mencatat perpindahan uang yang sudah terjadi; Haruflow tidak mentransfer uang melalui bank secara otomatis. Pajak eksplisit tidak menggantikan e-faktur atau pelaporan pajak.

## Dari laporan ke insight

<p align="center"><img src="docs/images/accounting.png" alt="Laporan keuangan asli Haruflow pada data simulasi" width="1280" /></p>

Gunakan laporan untuk mencocokkan angka, lalu bertanya di Codex, misalnya:

> “Ringkas kondisi usaha bulan ini. Bedakan laba dan perubahan kas; tunjukkan piutang yang perlu ditinjau, sumber angka, periode dan batas kesimpulannya.”

Percakapan berlangsung **di Codex**. Admin mengatur izin dalam **Pengaturan → Integrasi AI**; pengguna membuat kunci pribadi sesuai lingkupnya. Koneksi MCP lokal hanya membaca data yang diizinkan. `haruflow.exe mcp check` memeriksa akun, lingkup dan katalog tanpa membuka laporan. [Hubungkan Codex](docs/MCP.md) · [Pemecahan masalah](docs/MCP_TROUBLESHOOTING.md).

Data yang diberikan ke Codex dapat diproses layanan AI yang digunakannya. Pemilik menentukan data yang dibagikan; kunci tidak ditempel ke chat atau Git.

## Jalankan dengan tenang

Paket Windows menyediakan pilihan jadwal backup, status layanan dan retensi backup terkelola; jam/lokasi dapat diubah pengguna. Task memakai akun Windows saat ini tanpa password tersimpan. Simpan salinan di perangkat terpisah dan latihan pemulihan pada database uji. Setelah restore, cabut kunci MCP lama sebelum aplikasi dibuka kembali. `haruflow database check` memeriksa konsistensi melalui snapshot baca tanpa memperbaiki angka otomatis. [Backup Windows](docs/WINDOWS_BACKUP.md) · [Pemeriksaan database](docs/DATABASE.md) · [Pemulihan dan upgrade](docs/OPERATIONS.md#restore-ke-instalasi-pengujian).

Validasi paket Community menggunakan **simulasi transaksi, instalasi, pemulihan dan percakapan Codex**, dengan hasil pada [status implementasi](docs/IMPLEMENTATION_STATUS.md). Pilot usaha nyata merupakan pilihan deployment terpisah dan tidak dianggap sudah dilakukan. [Skenario latihan](docs/SMALL_BUSINESS_PILOT.md) · [Identitas paket dan pengukuran](docs/RELEASE_CANDIDATE.md).

**Rencana Enterprise:** HR/payroll, produksi, POS khusus, multi-company, multicurrency dan AI lanjutan seperti chat tertanam atau MCP jarak jauh. Ini rencana pengembangan Enterprise, bukan fitur yang sudah tersedia atau syarat Community selesai. MCP lokal baca tetap bagian Community.

## Untuk pengembang

Rust/Axum + SQLx/PostgreSQL menyajikan API dan frontend React/TypeScript dari satu aplikasi. Instalasi default cukup aplikasi dan PostgreSQL; Redis, broker dan Kubernetes tidak diperlukan.

- [Setup lokal, perintah tes dan kontrak API](docs/OPERATIONS.md#pengembangan-dan-verifikasi).
- [Status implementasi dan bukti](docs/IMPLEMENTATION_STATUS.md), [arsitektur awal](haruflow.md), [rencana Community](docs/COMMUNITY_PLAN.md).
- Source: [frontend](apps/web/src), [backend](crates/server/src), [migrasi](migrations), [Docker Compose](deploy/community).
- Panduan modul: [penjualan](docs/SALES_FULFILLMENT.md), [pembelian](docs/PURCHASING.md), [biaya perolehan](docs/LANDED_COSTS.md), [inventori](docs/INVENTORY_LOCATIONS.md), [saldo pihak](docs/PARTY_BALANCES.md), [laporan](docs/FINANCIAL_REPORTS.md), [impor](docs/TRANSACTION_IMPORTS.md).

## Lisensi

Haruflow Community menggunakan **GNU Affero General Public License v3.0 (AGPL-3.0-only)**. Source yang sesuai disertakan dalam paket kandidat; ketentuan lengkap ada di [LICENSE](LICENSE). Batas 10 akun ditegakkan aplikasi/database; pemilik yang menguasai source dan database tetap dapat mengubah instalasinya sesuai lisensi.
