# Menjalankan Haruflow Community

## Konfigurasi dan batas instalasi

Setiap instalasi memiliki satu perusahaan, satu gudang logis, satu database, dan mata uang IDR. Zona waktu perusahaan awal adalah Asia/Jakarta. Nominal API menggunakan string decimal, bukan floating point. Kuantitas mendukung tiga tempat desimal, biaya unit empat, dan nominal uang dua. Aplikasi belum menghitung pajak atau dokumen perpajakan.

`scripts/configure.mjs` membuat `.env` dengan password database dan token setup acak. File ini diabaikan Git. Jangan memakai `.env.example` tanpa mengganti semua placeholder. Untuk konfigurasi manual, gunakan password database URL-safe atau percent-encode password pada `DATABASE_URL`.

Variabel penting:

- `DATABASE_URL`: koneksi PostgreSQL; pool aplikasi dibatasi 10 koneksi.
- `HARUFLOW_SETUP_TOKEN`: token minimal 32 karakter untuk setup perusahaan pertama.
- `HARUFLOW_SECURE_COOKIES`: `true` untuk HTTPS; `false` hanya untuk HTTP lokal.
- `HARUFLOW_BIND`: alamat listen server native, default `127.0.0.1:8080`; container memakai `0.0.0.0:8080`.
- `HARUFLOW_PORT`: port loopback host pada Compose, default 8080.
- `HARUFLOW_STATIC_DIR`: lokasi frontend hasil build.
- `RUST_LOG`: level log; hindari mengaktifkan logging body atau secrets pada reverse proxy.

Compose menyimpan data pada named volume PostgreSQL. `docker compose down` mempertahankan volume; jangan menambahkan `-v` kecuali memang bermaksud menghapus database. Hindari perubahan mayor PostgreSQL langsung pada volume lama: lakukan prosedur upgrade database yang sesuai dengan backup dan restore teruji.

## Akses dari jaringan

Default Compose hanya mendengarkan pada loopback host. Pasang reverse proxy di host atau jaringan Docker dengan TLS dan forward request ke aplikasi. Jangan membuka port database. Pertahankan hostname yang sama untuk frontend dan API, lalu set `HARUFLOW_SECURE_COOKIES=true` dan restart aplikasi. Cookie memakai HttpOnly serta SameSite Strict.

Semua request API yang mengubah data harus memakai header `X-Haruflow-Request: 1`. Frontend menyediakannya secara otomatis. Server tidak membuka akses CORS lintas origin.

Cloudflare Tunnel bersifat opsional. Sesuaikan batas ukuran request dan timeout proxy dengan batas aplikasi; API membatasi body 256 KiB. Tidak ada endpoint upload file pada versi awal.

## Setup dan role

Form setup memerlukan token lokal dan membuat satu perusahaan sekaligus admin pertama. Tidak ada akun atau password bawaan. Simpan akses admin dan backup secrets di tempat yang dikendalikan operator.

- Administrator: seluruh aksi serta manajemen pengguna.
- Sales: pelanggan, pembuatan dan konfirmasi sales order.
- Warehouse: produk, penerimaan, adjustment, pengiriman, serta reversal pengiriman.
- Finance: invoice, pembayaran/reversal, jurnal manual, dan periode pembukuan.
- Viewer: akses baca; tidak dapat mengubah data.

Perubahan role, penonaktifan, dan reset password mencabut sesi pengguna. Admin tidak dapat menonaktifkan atau menurunkan role dirinya sendiri. Pengguna dapat mengganti password sendiri dengan password saat ini; sesi lama dicabut.

## Urutan kerja awal

1. Buat pelanggan dan produk.
2. Masukkan stok awal melalui penerimaan stok dengan biaya unit yang benar. Penerimaan versi awal mengkredit akun modal awal; jangan gunakan sebagai pengganti purchasing/utang supplier.
3. Jika perlu, masukkan modal kas melalui jurnal manual debit kas dan kredit modal awal.
4. Buat sales order lalu konfirmasi. Konfirmasi belum mereservasi stok.
5. Posting pengiriman penuh. Aplikasi mengunci stok, menolak stok negatif, serta membukukan HPP.
6. Buat invoice dari pengiriman lalu catat pembayaran; pembayaran sebagian didukung, uang muka dan overpayment belum didukung.
7. Periksa neraca saldo, kartu stok, dan piutang sebelum menutup periode.

Jurnal manual tidak boleh langsung mengubah akun kontrol piutang (1200) atau persediaan (1300). Gunakan alur invoice/pembayaran dan mutasi stok agar saldo subledger tetap cocok.

## Koreksi transaksi dan penutupan periode

Dokumen posted, jurnal, pembayaran, audit, dan pergerakan stok tidak dihapus dari riwayat. Reversal mencatat transaksi lawan serta alasan. Untuk membatalkan transaksi penjualan yang sudah lunas, balik pembayaran terlebih dahulu, kemudian invoice, lalu pengiriman. Order draft/confirmed dapat dibatalkan bila belum memiliki transaksi turunan aktif.

Reversal pengiriman mengembalikan biaya historis pengiriman ke nilai persediaan saat ini dan menghitung ulang biaya rata-rata. Reversal berbeda dari retur penjualan komersial; retur belum tersedia.

Penutupan periode menolak posting baru sampai tanggal yang ditutup, berdasarkan zona waktu perusahaan. Tanggal tutup hanya dapat maju dan tidak dapat dibuka kembali melalui UI. Semua posting versi awal menggunakan waktu transaksi saat ini; backdating belum didukung. Menutup tanggal hari ini menghentikan posting sampai hari berikutnya. Lakukan pemeriksaan dan backup sebelum menutup periode.

## Backup

Jalankan `node scripts/database.mjs backup` dari root repository. Script menjalankan `pg_dump -Fc`, menulis arsip pada folder `backups/`, dan membuat metadata waktu serta versi aplikasi. Nama file dapat diberikan sebagai argumen kedua. File yang sudah ada tidak ditimpa.

Cadangkan arsip, versi/tag atau digest image aplikasi, dan konfigurasi yang diperlukan ke lokasi lain yang terenkripsi. `.env` berisi secrets dan harus disimpan dengan perlindungan terpisah. Backup dalam disk yang sama tidak melindungi dari kegagalan disk.

Operator menentukan jadwal dan retensi, misalnya backup harian serta sebelum upgrade. Jadwal otomatis dan PITR tidak dikonfigurasi oleh instalasi default. Dokumentasikan kehilangan data maksimum yang dapat diterima sebelum menetapkan interval.

## Restore ke instalasi pengujian

1. Siapkan checkout dan image versi yang sesuai backup pada mesin/direktori lain, dengan nama proyek Compose dan volume yang berbeda. Buat `.env` untuk instalasi tujuan.
2. Jalankan PostgreSQL tujuan. Jangan menjalankan aplikasi penerima penulisan selama restore.
3. Jalankan `node scripts/database.mjs restore path/arsip.backup --confirm-restore`. Script menghentikan aplikasi tujuan, melakukan restore satu transaksi, dan membiarkan aplikasi berhenti.
4. Pastikan command berhasil. Kegagalan restore tidak boleh diabaikan; jangan menyalakan aplikasi sebelum hasilnya jelas.
5. Jalankan `docker compose --env-file .env -f deploy/community/compose.yml start app`.
6. Periksa `/health/ready`, login, daftar dokumen, jumlah stok, nilai persediaan, piutang, serta total debit/kredit neraca saldo terhadap sumber backup.
7. Catat tanggal, versi image, arsip, durasi, dan hasil verifikasi. Ulangi latihan secara berkala.

Secara default file Compose menggunakan nama proyek `haruflow`. Untuk latihan pada host yang sama gunakan checkout terpisah dan `COMPOSE_PROJECT_NAME=haruflow-restore-test` pada environment shell, serta ubah `HARUFLOW_PORT`. Nilai environment tersebut juga dipakai utilitas Node yang menjalankan Compose. Pastikan port dan volume berbeda sebelum restore.

## Upgrade

1. Baca release notes dan perubahan skema; siapkan image versi sebelumnya dan backup yang telah diuji.
2. Buat backup baru, lalu hentikan aplikasi untuk mencegah penulisan selama upgrade.
3. Ambil source/image versi tujuan dan build image bila menggunakan source lokal.
4. Jalankan `docker compose --env-file .env -f deploy/community/compose.yml run --rm migrate` sekali. Server tidak menjalankan migrasi otomatis di setiap startup.
5. Jalankan aplikasi, periksa readiness dan alur transaksi representatif.
6. Simpan versi yang diterapkan dan hasil pemeriksaan.

Jangan mengedit migrasi yang sudah diterapkan. Tambahkan migrasi baru berurutan. Rollback executable hanya aman bila schema masih kompatibel. Jika tidak, pulihkan backup dengan versi aplikasi yang cocok dan perhitungkan perubahan setelah waktu backup.

## Health, log, dan troubleshooting

- `/health/live`: proses HTTP hidup.
- `/health/ready`: koneksi database dapat melakukan query.
- `haruflow healthcheck`: probe readiness untuk container.
- `docker compose --env-file .env -f deploy/community/compose.yml logs --tail=100 app`: log aplikasi.

Jika tabel tidak ditemukan, jalankan migrasi sesuai prosedur. Jika setup ditolak, periksa token lokal dan apakah perusahaan sudah dibuat. Jika login berhasil tetapi sesi tidak bertahan pada HTTP lokal, pastikan cookie Secure tidak diaktifkan untuk lingkungan HTTP itu. Jika posting ditolak karena periode tertutup, periksa tanggal tutup; sistem tidak mengubahnya diam-diam.

Saat melaporkan bug sertakan versi commit/image, langkah reproduksi, request ID bila tersedia, dan pesan error yang sudah disanitasi. Jangan melampirkan `.env`, password, cookie sesi, atau dump data pelanggan ke issue publik.
