# Kandidat rilis dan pengukuran Community

Paket teknis, pengukuran simulasi, deployment usaha nyata dan publikasi adalah hasil berbeda. Lingkup pengerjaan Community saat ini diterima melalui simulasi yang hasilnya dicatat; pilot usaha nyata tidak menjadi syarat penyelesaiannya. Keberhasilan simulasi tidak diberi label sebagai persetujuan pemilik bisnis atau SLA.

## Identitas build dan readiness

`haruflow version` dan `GET /version` menghasilkan nama aplikasi, versi paket, commit build, dan schema yang dibutuhkan; respons tidak berisi secrets. Build lokal mencoba membaca commit Git. Docker menerima `--build-arg HARUFLOW_BUILD_COMMIT=<40-karakter-SHA>`; identitas yang tidak valid menjadi `unknown`.

`GET /health/live` memeriksa proses HTTP. `/health/ready` memerlukan seluruh migrasi build sudah berhasil dan checksum cocok. Database yang terhubung tetapi tertinggal, lebih baru, atau berbeda checksum menghasilkan 503. Migrasi tetap eksplisit, termasuk setelah restore backup lama.

## Paket kandidat

Workflow manual **Community release candidate** memerlukan **Community checks** lulus pada commit yang sama. Paket Linux/Windows berisi executable native, frontend, `BUILD.json`, `SHA256SUMS`, panduan awal, lisensi AGPL, serta seluruh corresponding source yang dilacak Git. Artefak tersedia pada run Actions; workflow tidak membuat tag, GitHub Release, atau publikasi registry.

Paket source menyertakan Dockerfile/Compose dan panduan operasi. PostgreSQL tetap perlu disiapkan terpisah untuk executable native; tidak ada installer PostgreSQL otomatis. Untuk membangun lokal, commit source, set `HARUFLOW_BUILD_COMMIT` ke SHA tersebut, lalu:

```sh
bun run build:web
cargo build --release --locked
node scripts/package-release.mjs
```

Script menolak working tree kotor dan executable dari commit berbeda. Folder kandidat dibuat eksklusif dalam `output/releases/`, tanpa menimpa kandidat lain. `HARUFLOW_RELEASE_BINARY` memilih executable. Pada Linux, pulihkan izin executable dengan `chmod +x ./haruflow` setelah mengunduh artefak Actions. Panduan native memakai `./haruflow` pada Linux dan `.\haruflow.exe` pada PowerShell. Verifikasi checksum dan uji instalasi pada lingkungan baru sebelum distribusi.

Paket Windows menyertakan `Install-Haruflow.ps1`, `Connect-Codex.ps1`, `Backup-Haruflow.ps1` dan panduan [Windows](WINDOWS.md). Installer per pengguna memeriksa manifest, membuat backup sebelum upgrade, menyimpan konfigurasi pribadi dan menyediakan start/stop. `scripts/windows/Test-Install.ps1` memeriksa instalasi pada database acak/folder workspace tanpa pintasan desktop. [Alat backup Windows](WINDOWS_BACKUP.md) dan [penerimaan MCP sintetis](MCP_ACCEPTANCE.md) dapat diuji terpisah. Runner [kesiapan usaha kecil](SMALL_BUSINESS_PILOT.md) mencakup gangguan proses, retry/restart dan restore. [Panduan pengguna](GUIDE.md) dan gambar aplikasi menggunakan data simulasi, bukan screen mockup.

## Pengukuran sintetis berulang

`node scripts/performance.mjs` membuat database acak `haruflow_perf_<8-hex>`, melakukan migrasi/setup, menambahkan 1.200 pelanggan serta 100 jurnal, lalu mengukur daftar server, laporan, posting, retry bersamaan, upload/download file 5 MiB, backup, dan restore. Server uji dihentikan dan hanya database yang dibuat script dihapus setelah selesai. Database bisnis dan preview tidak menjadi target.

Prasyarat: executable, PostgreSQL pengujian yang mengizinkan pembuatan database, serta `psql`/`pg_dump`/`pg_restore` yang cocok. Konfigurasi:

- `HARUFLOW_PERF_POSTGRES_URL`: koneksi admin pengujian; default `postgres://postgres@127.0.0.1:55432/postgres`.
- `HARUFLOW_PERF_BINARY`: executable; default `target/debug/haruflow` atau `.exe` pada Windows. Gunakan release untuk mengukur kandidat produksi.
- `HARUFLOW_PERF_PSQL`, `HARUFLOW_PERF_PG_DUMP`, `HARUFLOW_PERF_PG_RESTORE`: lokasi utilitas bila tidak ada di PATH.
- `HARUFLOW_PERF_ROWS`: 100–50.000 pelanggan; default 1.200.
- `HARUFLOW_PERF_CONCURRENCY`: 1–10 permintaan bersamaan; default 5.

JSON pada `output/performance/` mencatat perangkat/OS/RAM, profile dan commit, PostgreSQL, dataset, jumlah permintaan, konkurensi, error, p50/p95, backup/restore, serta kecocokan kas/jurnal/checksum file. Angka dari perangkat, build, atau workload berbeda tidak dapat dibandingkan sebagai ukuran setara. Hasil loopback sintetis bukan SLA produksi atau bukti pilot bisnis.

## Penerimaan simulasi dan deployment

Penerimaan simulasi mencakup transaksi barang/jasa, saldo/jurnal yang cocok, pembatasan akses, restart/retry, backup/restore, instalasi/upgrade serta insight MCP. Pengujian model Codex sebenarnya dan koneksi demo 8081 tercatat pada `output/readiness/codex-model-demo-d1b1c76ae5f8.json` dan `output/readiness/codex8081-live-check.json`; [status implementasi](IMPLEMENTATION_STATUS.md) membedakan fixture beserta angka masing-masing. Tidak ada persetujuan data bisnis nyata yang disimpulkan dari dua bukti tersebut.

Langkah berikut hanya berlaku ketika pemilik memilih deployment usaha nyata:

Siapkan usaha sasaran, tanggal mulai pencatatan, saldo stok/piutang/utang/kredit yang direkonsiliasi, operator/hak akses hingga 10 akun aktif, perangkat server, HTTPS, dan backup terpisah. Tentukan kehilangan data maksimum serta sasaran durasi pemulihan.

Jalankan pembukaan → transaksi sebagian → DP/pembayaran → retur/kredit → refund → koreksi → laporan, lalu latihan restore di lingkungan lain. Beberapa gudang, rekening/rekonsiliasi CSV, pajak eksplisit, impor historis dan tutup tahun sudah tersedia. Pelaporan pajak otomatis serta koneksi bank langsung belum tersedia. Bila pilot nyata dipilih, gunakan [form deployment usaha kecil](SMALL_BUSINESS_PILOT.md) dan catat hasil serta keputusan pemilik secara terpisah.
