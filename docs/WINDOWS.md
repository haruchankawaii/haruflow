# Instalasi Windows untuk usaha kecil

Paket kandidat `haruflow-community-win32-x64-<commit>` berisi binary, web, source, BUILD.json, SHA256SUMS dan Install-Haruflow.ps1. Ekstrak seluruh paket; jangan mencampur file dua versi. Paket belum ditandatangani dan mengikuti kebijakan eksekusi PowerShell komputer Anda.

Prasyarat: Windows x64, PostgreSQL 14+ yang sudah berjalan, database khusus Haruflow dan kredensial yang dapat menjalankan migrasi/backup. Installer tidak mengunduh PostgreSQL atau membuat service administrator. Database public yang berisi aplikasi lain ditolak.

```powershell
./Install-Haruflow.ps1 -PostgresBin 'C:/Program Files/PostgreSQL/16/bin'
```

Sesuaikan lokasi PostgreSQL. Koneksi database diminta secara tersembunyi. Default instalasi per pengguna di `%LOCALAPPDATA%/HaruflowCommunity`, loopback port 8080. Parameter `-InstallRoot`, `-Port`, `-DatabaseUrl`, `-NoShortcuts`, `-NonInteractive` tersedia untuk operator. Jangan menulis password ke dokumen atau command history bersama.

Installer memeriksa semua checksum, menyalin versi ke `versions/<commit>`, memeriksa identitas binary/schema, dan memigrasikan database. Sebelum upgrade database yang sudah berisi Haruflow, server instalasi ini dihentikan dan dibuat backup PostgreSQL custom beserta SHA-256/TOC. Jika migrasi gagal, server tetap berhenti; versi lama tidak dijalankan otomatis pada schema yang sudah berubah.

Start-Haruflow.ps1 memulai proses tersembunyi, memeriksa readiness/commit, dan membuka browser. Stop-Haruflow.ps1 hanya menghentikan PID yang path-nya sesuai instalasi ini. Log ada di logs/. PostgreSQL harus tetap berjalan; aplikasi tidak otomatis menjadi service/berjalan saat boot. Konfigurasi .env dan Setup-pertama.txt dibatasi ACL pemilik/SYSTEM. Kode setup hanya diperlukan untuk membuat perusahaan/admin pertama. Ubah password akun setelah penyerahan operator.

Backup installer memakai metadata `haruflow-windows-pre-upgrade-v1`, berbeda dari manifest `database.mjs`. Periksa SHA-256 dan `pg_restore --list`, lalu restore ke database kosong yang disiapkan operator. Pastikan jumlah/saldo/bukti sesuai sebelum mengganti koneksi. Jangan menjalankan dua server berbeda terhadap database yang sedang dipulihkan. Untuk backup rutin, uji pemulihan dan simpan salinan di perangkat terpisah; lihat [operasi](OPERATIONS.md).

HTTP loopback hanya untuk komputer lokal. Akses LAN/internet memerlukan konfigurasi HTTPS dan secure cookie menurut panduan operasi. Paket kandidat dan uji sintetis bukan persetujuan pemilik untuk menjalankan data produksi.
