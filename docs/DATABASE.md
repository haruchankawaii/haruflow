# Database dan pemeriksaan integritas

> Diverifikasi pada schema 33: tiga tes diagnostik lulus, dan executable memeriksa fixture transaksi serta alokasi biaya perolehan dengan seluruh pemeriksaan sehat. Hasil paket/upgrade dicatat per build pada [status implementasi](IMPLEMENTATION_STATUS.md).

Haruflow menyimpan transaksi, saldo, jurnal, audit, lampiran dan hash akun/kunci dalam **satu database PostgreSQL**. Frontend dan MCP tidak memegang kredensial database. Perubahan operasional yang saling terkait disimpan dalam transaksi yang sama; jurnal posted dan audit dipertahankan, sedangkan koreksi memakai catatan pembalik.

Panduan ini ditujukan untuk operator instalasi. Pengguna sehari-hari memakai aplikasi dan laporan; tidak perlu mengedit SQL atau tabel secara langsung.

## Periksa versi sebelum bertindak

- `haruflow version` atau `/version`: identitas aplikasi, commit dan schema yang diperlukan build.
- `/health/live`: proses HTTP hidup.
- `/health/ready`: koneksi dan seluruh migrasi/checksum cocok; database yang tertinggal, lebih baru atau berbeda checksum tidak dinyatakan siap.
- `haruflow database check`: pemeriksaan konsistensi database melalui satu snapshot baca. Ini berbeda dari `haruflow mcp check`, yang hanya memeriksa koneksi/izin AI tanpa akses database.

## Pemeriksaan lokal tanpa mengubah data

Gunakan binary versi terpasang dari folder konfigurasi instalasi yang benar. Mode ini memerlukan akses database operator melalui `.env` atau `DATABASE_URL`, bukan kunci MCP. Jangan menulis koneksi/password pada argumen terminal, chat atau berkas yang dilacak Git.

Untuk Windows native, sesuaikan folder bila memakai instalasi khusus:

```powershell
& "$env:LOCALAPPDATA/HaruflowCommunity/Status-Haruflow.ps1" -CheckDatabase
```

Untuk Docker, dari checkout/konfigurasi instalasi tujuan:

```sh
docker compose --env-file .env -f deploy/community/compose.yml run --rm --no-deps app database check
```

Output JSON memuat `healthy`, `read_only`, `checked_at`, `schema_version`, daftar `checks` dengan `name`/`passed`/`issues`, serta `warnings`. `schema_version` adalah target build; hasil `migration_checksums` menunjukkan kecocokannya dengan database. Respons tidak memuat nama pelanggan, rincian transaksi, nominal saldo atau password.

Perintah memakai transaksi **REPEATABLE READ, READ ONLY** dan batas 30 detik per statement. Tidak ada perbaikan otomatis atau migrasi saat pemeriksaan. Bila ketidaksesuaian ditemukan, JSON tetap tersedia dan proses berakhir dengan exit code bukan nol. Kegagalan koneksi/query juga berakhir bukan nol; jangan menganggap tidak adanya JSON sebagai kondisi sehat.

## Apa yang diperiksa

- **Schema dan pelindung data:** migrasi/checksum, validasi foreign key/check constraint, kesiapan indeks, trigger aktif dan keberadaan guard penting pada jurnal, stok, audit serta kuota pengguna.
- **Jurnal:** setiap jurnal memiliki minimal dua baris dan total debit sama dengan kredit.
- **Stok:** jumlah/nilai produk sama dengan jumlah/nilai seluruh posisi lokasi; kuantitas cocok dengan mutasi; posisi lokasi cocok dengan riwayat lokasi; nilai persediaan cocok dengan akun kontrol.
- **Saldo pihak:** piutang/utang dan kredit pelanggan/supplier dari riwayat operasional, pembukaan, impor, alokasi, refund dan pembalikan cocok dengan akun kontrol ledger.
- **Fondasi penyimpanan:** satu perusahaan dan konfigurasi durability PostgreSQL `fsync`, `full_page_writes`, `synchronous_commit` tidak dimatikan.

Bila schema belum kompatibel, pemeriksaan yang memerlukan struktur tabel tersebut tidak dijalankan. Instalasi lama yang memiliki lebih dari 10 akun aktif menghasilkan peringatan terpisah; perintah tidak menonaktifkan akun. `healthy: true` berarti pemeriksaan yang dijalankan lolos, bukan jaminan terhadap semua jenis kerusakan, kapasitas disk atau kesalahan pemilihan data usaha.

## Jika ada masalah

Catat identitas build, waktu dan nama pemeriksaan yang gagal. Simpan backup yang ada; jangan menghapus jurnal, mematikan trigger atau menyamakan saldo lewat SQL untuk menutupi selisih. Periksa versi aplikasi/schema dan sumber transaksi, kemudian reproduksi pada database uji. Koreksi keuangan tetap melalui alur aplikasi yang mempertahankan audit.

Jika query melebihi batas waktu pada volume besar, ukur perangkat serta workload secara terpisah; pemeriksaan yang terputus tidak dianggap lulus. Tinjau log yang sudah disanitasi dan kebutuhan kapasitas sebelum mencoba ulang. Perintah ini tidak melakukan `VACUUM`, rebuild indeks, upgrade PostgreSQL atau tuning server.

## Migrasi, backup dan restore

1. Gunakan migrasi berurutan dari paket/commit yang sama. Jangan mengubah migrasi yang sudah diterapkan.
2. Verifikasi backup sebelum upgrade; hentikan server penulis ketika memigrasikan schema. Server tidak menjalankan migrasi otomatis setiap startup.
3. Untuk latihan restore, siapkan database kosong serta folder/port terpisah. Cocokkan PostgreSQL dan versi/schema aplikasi dengan backup; jangan menjalankan dua versi server pada target yang sedang dipulihkan.
4. Setelah restore dan migrasi yang diperlukan, cabut kunci AI hasil pemulihan sebelum server dibuka. `haruflow admin revoke-ai-tokens EMAIL` memakai admin aktif untuk atribusi audit dan mencabut seluruh kunci, tanpa menerbitkan pengganti.
5. Jalankan pemeriksaan database, lalu cocokkan dokumen/saldo dan byte/SHA-256 lampiran ke sumber backup. Pemeriksaan otomatis dan pengecekan bukti saling melengkapi.

[Backup Windows](WINDOWS_BACKUP.md) · [Runbook server, migrasi dan pemulihan](OPERATIONS.md) · [Status pengujian per build](IMPLEMENTATION_STATUS.md).
