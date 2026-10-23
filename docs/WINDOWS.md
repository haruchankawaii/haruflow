# Instalasi Windows untuk usaha kecil

Paket kandidat `haruflow-community-win32-x64-<commit>` berisi binary, web, source, BUILD.json, SHA256SUMS dan Install-Haruflow.ps1. Ekstrak seluruh paket; jangan mencampur file dua versi. Paket belum ditandatangani dan mengikuti kebijakan eksekusi PowerShell komputer Anda.

Prasyarat: Windows x64 dan binaries PostgreSQL 14+ x64. Untuk komputer baru, gunakan **Setup-Haruflow.ps1**. Wizard mencari PostgreSQL di PATH/Program Files atau menerima folder `-PostgresBin`; pilihan pertama membuat cluster/database privat di folder Haruflow, tanpa mengubah service/data PostgreSQL lain dan tanpa hak administrator. Pilihan kedua memakai koneksi database yang sudah tersedia. Database public yang berisi aplikasi lain ditolak.

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File ./Setup-Haruflow.ps1
```

Kebijakan `Bypass` di contoh/pintasan/task berlaku pada proses PowerShell tersebut, tidak mengubah kebijakan mesin. Paket harus sudah diekstrak lengkap dan pemeriksaan checksum installer tetap berlaku. Ikuti kebijakan organisasi bila menjalankan skrip lokal dibatasi.

Wizard menanyakan jam backup (12:00, 17:00, 21:00, jam `HH:mm` lain, atau lewatkan) dan apakah layanan mulai saat akun Windows masuk. Tidak ada jam yang dipilih diam-diam. Default folder backup berada di `backups/scheduled` privat, dengan retensi 14 arsip terkelola terbaru. Folder pada PC yang sama membantu salah input/kerusakan database, tetapi bukan pengganti salinan pada perangkat terpisah. Lihat [backup Windows](WINDOWS_BACKUP.md).

Jika PostgreSQL belum ditemukan, wizard berhenti dengan alamat [unduhan Windows resmi PostgreSQL](https://www.postgresql.org/download/windows/). Halaman itu menyediakan installer EDB dan tautan ZIP binaries untuk pengguna lanjut. Unduh versi x64 stabil yang didukung, periksa penerbit/distribusinya, pasang atau ekstrak, lalu jalankan wizard lagi dengan folder `bin` yang berisi `initdb.exe`, `postgres.exe`, `pg_ctl.exe`, `psql.exe`, `pg_dump.exe` dan `pg_restore.exe`. Contoh:

```powershell
./Setup-Haruflow.ps1 -PostgresBin 'C:/Program Files/PostgreSQL/16/bin'
```

Haruflow tidak mengunduh/mengeksekusi installer internet secara otomatis. Binaries yang sudah ada boleh dipakai oleh cluster privat; service PostgreSQL hasil pemasangan tetap terpisah. Jangan menghapus direktori binaries tersebut selama cluster Haruflow masih menggunakannya.

## Cluster privat dan koneksi yang sudah tersedia

Cluster privat berada di `postgres/data` pada folder instalasi, memakai SCRAM, hanya mendengarkan `127.0.0.1`, dan port 55433 atau port bebas berikutnya. Bootstrap admin cluster terpisah dari role aplikasi `haruflow`; role aplikasi adalah pemilik database tanpa SUPERUSER, CREATEDB, CREATEROLE, REPLICATION, atau BYPASSRLS. `fsync`, `full_page_writes`, dan `synchronous_commit` tetap aktif. Password acak tidak ditampilkan atau dimasukkan ke argumen program; berkas koneksi aplikasi/admin dan direktori data diberi ACL akun pemilik/SYSTEM. Simpan konfigurasi privat secara terpisah dari backup rutin.

`Manage-Postgres.ps1 -Action Start|Stop|Status -InstallRoot <folder>` hanya mengelola cluster yang identitas akun/path/prosesnya cocok. Tidak ada Windows service baru, pergantian port server lain, atau penghapusan data otomatis. `Start-Haruflow.ps1` menyalakan cluster privat bila diperlukan; `Stop-Haruflow.ps1 -StopDatabase` menghentikan aplikasi dan cluster privatnya secara tertib. Folder cluster yang sudah ada tidak diinisialisasi ulang.

Jika peluncuran pertama PostgreSQL terputus, jalankan kembali wizard dengan folder instalasi yang sama. Kredensial acak sudah disimpan privat sebelum server diluncurkan; wizard hanya melanjutkan bootstrap milik akun ini yang tercatat belum selesai, tanpa menjalankan `initdb` ulang atau mengganti kredensial. Bila cluster sudah selesai dibuat tetapi installer belum menulis `.env`, wizard memakai koneksi privat yang tersimpan; `-DatabaseUrl` yang diberikan operator tetap diutamakan. Bila metadata/kredensial hilang atau identitas tidak cocok, proses berhenti dan mempertahankan data. Jangan menghapus `postgres/data`, `postgres.json`, atau berkas koneksi untuk mencoba memperbaikinya; periksa `postgres/postgres.log` dan minta operator memulihkan konfigurasi privat yang benar.

Untuk PostgreSQL yang dikelola sendiri, jalankan wizard dengan `-DatabaseMode Existing`, atau gunakan installer dasar berikut dan masukkan koneksi secara tersembunyi. Haruflow tidak mengubah role/security server eksternal:

```powershell
./Install-Haruflow.ps1 -PostgresBin 'C:/Program Files/PostgreSQL/16/bin'
```

Sesuaikan lokasi PostgreSQL. Koneksi database diminta secara tersembunyi. Default instalasi per pengguna di `%LOCALAPPDATA%/HaruflowCommunity`, loopback port 8080. Parameter `-InstallRoot`, `-Port`, `-DatabaseUrl`, `-NoShortcuts`, `-NonInteractive` tersedia untuk operator. Jangan menulis password ke dokumen atau command history bersama.

Installer memeriksa semua checksum, menyalin versi ke `versions/<commit>`, memeriksa identitas binary/schema, dan memigrasikan database. Sebelum upgrade database yang sudah berisi Haruflow, server instalasi ini dihentikan dan dibuat backup PostgreSQL custom beserta SHA-256/TOC. Jika migrasi gagal, server tetap berhenti; versi lama tidak dijalankan otomatis pada schema yang sudah berubah.

Start-Haruflow.ps1 memulai proses tersembunyi, memeriksa readiness/commit, dan membuka browser. Stop-Haruflow.ps1 hanya menghentikan PID yang path-nya sesuai instalasi ini. Log aplikasi ada di logs/; log cluster privat di postgres/postgres.log. Start saat login tersedia sebagai pilihan wizard, memakai task milik pengguna tanpa menyimpan password Windows. Task berjalan ketika akun masuk, bukan service saat boot. Konfigurasi .env dan Setup-pertama.txt dibatasi ACL pemilik/SYSTEM. Kode setup hanya diperlukan untuk membuat perusahaan/admin pertama. Ubah password akun setelah penyerahan operator.

Backup installer memakai metadata `haruflow-windows-pre-upgrade-v1`, berbeda dari manifest `database.mjs`. Periksa SHA-256 dan `pg_restore --list`, lalu restore ke database kosong yang disiapkan operator. Pastikan jumlah/saldo/bukti sesuai sebelum mengganti koneksi. Jangan menjalankan dua server berbeda terhadap database yang sedang dipulihkan. Untuk backup rutin gunakan `Backup-Haruflow.ps1` yang disertakan di root paket/instalasi; alat ini membuat arsip, metadata dan checksum dalam folder privat baru tanpa menghapus backup lama atau memasang jadwal. Ikuti [panduan backup Windows dan latihan restore](WINDOWS_BACKUP.md).

HTTP loopback hanya untuk komputer lokal. Akses LAN/internet memerlukan konfigurasi HTTPS dan secure cookie menurut panduan operasi. Paket kandidat dan uji sintetis bukan persetujuan pemilik untuk menjalankan data produksi.

Schema 31 menambahkan integrasi baca MCP untuk Codex melalui executable yang sama (`haruflow.exe mcp`). Installer menyertakan `Connect-Codex.ps1` untuk menyimpan kunci pribadi dengan ACL akun Windows dan menyiapkan contoh konfigurasi. Aktifkan izin melalui Pengaturan → Integrasi AI sebelum pengguna membuat kunci; akses awal nonaktif. Percakapan berlangsung di Codex. Lihat [panduan MCP](MCP.md).

## Pemeriksaan operator dan simulasi

```powershell
./Status-Haruflow.ps1 -CheckDatabase
./Status-Haruflow.ps1 -OpenRecoveryGuide
./Schedule-Haruflow.ps1 -Action Status
```

Status menampilkan versi, layanan, cluster privat/eksternal, jadwal, hasil backup terakhir, dan lokasi panduan pemulihan tanpa menampilkan password. `-CheckDatabase` memanggil diagnosis read-only binary; kegagalan harus diperiksa, tidak diperbaiki otomatis dengan mengubah catatan keuangan. `-OpenRecoveryGuide` membuka panduan lokal. Simulasi Community cukup untuk validasi pengembang saat ini; bukan klaim bahwa usaha nyata sudah memberikan persetujuan operasional.

Uji otomatis `scripts/windows/Test-WindowsSetup.ps1 -PackageRoot <paket> -PostgresBin <bin>` membuat folder/cluster/task acak, memeriksa pemulihan peluncuran awal yang sengaja diputus, prioritas koneksi eksplisit ketika `.env` belum ada, role terbatas dan durability, pemasangan, pilihan jadwal, disable/enable/ubah/hapus task, pelestarian lokasi/retensi ketika hanya jam diubah, backup/retensi aman dan restart. Task uji dihapus serta cluster uji dihentikan pada akhir uji; data fixture ditinggalkan di `.local` untuk pemeriksaan. Uji tidak mengubah preview, task pribadi, koneksi Codex atau instalasi pengguna lain.
