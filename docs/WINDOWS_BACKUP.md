# Backup rutin Windows dan latihan pemulihan

Untuk instalasi native Haruflow Community, jalankan `Backup-Haruflow.ps1` dari akun Windows pemilik instalasi. Skrip membaca `.env` dan `current.json` secara pribadi, memeriksa identitas `versions/<commit>/BUILD.json`, serta mencocokkan schema database sebelum membuat backup. Skrip tidak menghentikan server dan `pg_dump` memakai snapshot konsisten PostgreSQL. Jangan menjalankan backup bersamaan dengan upgrade atau perubahan schema.

## Buat dan periksa backup

Pilih sendiri lokasi yang tersedia dan dapat ditulis, misalnya drive cadangan `E:`. Ganti folder PostgreSQL sesuai PC. Contoh ini bukan konfigurasi drive yang sudah dipasang:

```powershell
& "$env:LOCALAPPDATA/HaruflowCommunity/Backup-Haruflow.ps1" `
  -InstallRoot "$env:LOCALAPPDATA/HaruflowCommunity" `
  -BackupDirectory 'E:/BackupHaruflow' `
  -PostgresBin 'C:/Program Files/PostgreSQL/16/bin'
```

Hasil satu pemanggilan berada dalam subfolder unik `haruflow-<UTC>-<acak>`:

- `haruflow.backup`: arsip PostgreSQL custom, termasuk data transaksi, lampiran, hash password akun, dan hash kunci MCP.
- `haruflow.backup.json`: waktu UTC, commit, schema, versi PostgreSQL, ukuran, SHA256, dan jenis pemeriksaan. Koneksi/password database tidak dimasukkan ke metadata.
- `haruflow.backup.sha256`: checksum arsip. Simpan ketiga file bersama.

Folder backup setiap pemanggilan dan berkasnya diberi ACL hanya untuk akun Windows pembuat serta SYSTEM. Skrip tidak mencetak password atau mengirim URL/password database melalui argumen program. Konfigurasi URL mendukung opsi `sslmode`, `sslrootcert`, `sslcert`, `sslkey`, `channel_binding`, dan `connect_timeout`; opsi lain ditolak agar skrip tidak diam-diam mengubah tujuan koneksi. Gunakan utilitas PostgreSQL yang cocok dengan server: `pg_dump` tidak boleh lebih tua daripada server. Instalasi yang ada tidak diubah bila backup gagal.

Verifikasi ulang setelah menyalin ke perangkat cadangan:

```powershell
& "$env:LOCALAPPDATA/HaruflowCommunity/Backup-Haruflow.ps1" `
  -ArchivePath 'E:/BackupHaruflow/haruflow-<UTC>-<acak>/haruflow.backup' `
  -PostgresBin 'C:/Program Files/PostgreSQL/16/bin'
```

Perintah memeriksa ukuran, SHA256, identitas metadata, dan `pg_restore --list` tanpa menyambung atau mengubah database. Checksum bukan tanda tangan digital dan verifikasi daftar isi bukan pembuktian bahwa seluruh data sudah dapat dipulihkan. Latihan restore berikut tetap diperlukan.

Skrip tidak menghapus backup lama, termasuk ketika dipanggil berulang. Pemilik menentukan retensi setelah mengetahui ukuran dan kapasitas. Contoh kebijakan usaha kecil: simpan 14 backup harian serta satu salinan akhir bulan, lalu hapus salinan lama secara manual hanya setelah backup terbaru lolos verifikasi dan satu restore berhasil. Pisahkan minimal satu salinan dari PC ERP; sinkronisasi ke folder lain pada disk yang sama tidak melindungi dari kerusakan disk. ACL bukan enkripsi media: gunakan penyimpanan terenkripsi seperti BitLocker dan lindungi kunci pemulihannya. Simpan `.env`, kode setup, dan berkas kunci MCP secara terpisah; jangan memasukkannya ke folder berbagi atau Git.

## Jadwal harian, setelah lokasi dan jam diputuskan pemilik

Tidak ada scheduled task yang dipasang otomatis. Di **Task Scheduler → Create Task**, operator dapat mengisi:

1. Nama misalnya `Haruflow backup harian`, akun Windows yang sama dengan pemilik instalasi dan lokasi backup. Pilih **Run only when user is logged on** agar tidak menyimpan password Windows di task. Tidak perlu **Run with highest privileges**.
2. Trigger harian pada jam yang disetujui saat PC, PostgreSQL dan perangkat backup tersedia. Aktifkan **Run task as soon as possible after a scheduled start is missed**.
3. Program `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`.
4. Arguments, dengan path aktual, bukan variabel `%LOCALAPPDATA%` atau placeholder:

   ```text
   -NoProfile -NonInteractive -File "C:\Users\<akun>\AppData\Local\HaruflowCommunity\Backup-Haruflow.ps1" -InstallRoot "C:\Users\<akun>\AppData\Local\HaruflowCommunity" -BackupDirectory "E:\BackupHaruflow" -PostgresBin "C:\Program Files\PostgreSQL\16\bin"
   ```

5. Start in: folder instalasi aktual. Pada **If the task is already running**, pilih **Do not start a new instance**. Jangan menambahkan password database ke task atau melewati kebijakan eksekusi organisasi.
6. Jalankan **Run** sekali dan periksa **Last Run Result = 0x0**, waktu metadata, serta verifikasi arsip. Jalankan kembali setelah restart/login dan setelah perangkat cadangan disambungkan ulang. Task hanya berjalan saat akun masuk; bila kebutuhan usaha adalah berjalan tanpa login, IT harus menyiapkan akun layanan, ACL dan kebijakan credential tersendiri, lalu menguji ulang.

Periksa hasil setiap hari kerja. Hasil task `0x0` perlu disertai arsip bertanggal terbaru dan metadata/checksum lengkap; jangan menganggap task yang terdaftar berarti backup berhasil. Pencabutan kunci MCP di Haruflow tidak memengaruhi backup database.

## Latihan restore sebelum mulai operasional

Operator dan pemilik memilih satu backup, memeriksa metadata, dan menyiapkan **database kosong baru** serta folder/port aplikasi uji terpisah. Gunakan versi PostgreSQL yang sesuai dan binary Haruflow yang cocok dengan commit/schema backup. Jangan arahkan aplikasi asli ke database uji atau menjalankan dua versi server pada database yang sedang dipulihkan.

Contoh untuk administrator PostgreSQL; perintah meminta password secara tersembunyi, tanpa password pada command line. Nama tujuan harus diganti dengan database uji kosong yang baru dibuat:

```powershell
& 'C:/Program Files/PostgreSQL/16/bin/psql.exe' -X -W `
  --host=127.0.0.1 --port=5432 --username=postgres --dbname=postgres `
  -v ON_ERROR_STOP=1 -c 'CREATE DATABASE haruflow_restore_practice;'
& 'C:/Program Files/PostgreSQL/16/bin/pg_restore.exe' -W `
  --host=127.0.0.1 --port=5432 --username=postgres --dbname=haruflow_restore_practice `
  --no-owner --no-acl --single-transaction --exit-on-error `
  'E:/BackupHaruflow/haruflow-<UTC>-<acak>/haruflow.backup'
if ($LASTEXITCODE -ne 0) { throw 'Restore gagal; aplikasi uji tetap berhenti.' }
```

`--no-owner --no-acl` di atas memakai akun pemulihan sebagai pemilik objek; IT harus memberikan kepemilikan/hak yang benar kepada akun aplikasi tujuan sebelum menjalankannya. Tidak ada `--clean` dan contoh tidak mengganti isi database asli. Jangan langsung menginstal paket terbaru terhadap database latihan yang belum direview: installer dapat memigrasikan schema. Bila backup lebih lama, catat hasil restore pada schema asli dahulu, baru lakukan migrasi eksplisit dengan backup dan versi yang sesuai.

**Putuskan akses MCP hasil restore sebelum server tujuan dinyalakan.** Backup mengembalikan status token pada waktu backup; kunci yang sudah dicabut kemudian bisa kembali aktif. Bila backup memakai schema lama, migrasikan database tujuan ke schema 31 dahulu dengan aplikasi berhenti. Dari folder instalasi **tujuan** yang `.env`-nya sudah diarahkan ke database hasil restore, jalankan binary versi yang sesuai:

```powershell
# Jalankan dari folder instalasi tujuan, bukan folder instalasi asli.
& './versions/<commit>/haruflow.exe' admin revoke-ai-tokens 'admin@usaha.example'
if ($LASTEXITCODE -ne 0) { throw 'Pencabutan kunci gagal; server harus tetap berhenti.' }
```

Ganti email dengan admin aktif pada database tujuan untuk atribusi audit. Command ini memerlukan akses lokal ke koneksi database; email bukan pengganti otorisasi database. Periksa hasil pencabutan dan jumlah kunci aktif nol sebelum menjalankan server. Lanjutkan pemeriksaan kunci lama ditolak sesuai [panduan operasi](OPERATIONS.md). Admin baru memberikan izin/kunci pribadi kembali setelah pemilik menyetujui. Berkas token Codex lama harus diganti, bukan dipakai ulang. Jangan hanya menghapus file token lokal: hal itu tidak mencabut token di database.

Setelah target diamankan, mulai aplikasi uji dengan port terpisah. Pemilik mencocokkan tanggal/jumlah dokumen, saldo kas/bank, jumlah/nilai stok, piutang/utang beserta kredit, jurnal, dan minimal satu lampiran (byte serta SHA256). Catat waktu pemulihan, titik data terakhir, commit/schema dan masalah yang tersisa pada [lembar pilot](SMALL_BUSINESS_PILOT.md). Uji satu kunci MCP lama ditolak, lalu kunci baru hanya dapat membaca lingkup yang diizinkan. Jangan mulai menulis transaksi nyata sampai pemilik menyetujui hasilnya.

## Uji pengembang yang dapat diulang

```powershell
powershell.exe -NoProfile -File scripts/windows/Test-Backup.ps1 `
  -PostgresBin 'C:/Program Files/PostgreSQL/16/bin' -PostgresPort 5432
```

Uji membuat dua database bernama acak `haruflow_backup_*` dan `haruflow_restore_*`, konfigurasi fixture privat, serta output di `output/readiness/windows-backup-*`. Hanya database miliknya yang dihentikan/dihapus. Uji memeriksa ACL, SHA256/TOC, pemulihan data fixture, isolasi/pemulihan override libpq (termasuk `PGHOSTADDR`, `PGSERVICE`, dan `PGOPTIONS`), pelestarian backup sebelumnya, penolakan arsip berubah, dan penolakan schema berbeda. Normalisasi URI IPv6 diperiksa tanpa memerlukan server IPv6. Ini latihan teknis sintetis, bukan penerimaan data usaha nyata. Bukti `verified.json` tidak memuat koneksi/password; jangan membagikan seluruh folder uji karena `.env` fixture tetap bersifat pribadi.
