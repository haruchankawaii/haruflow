# Menjalankan Haruflow Community

## Konfigurasi dan batas instalasi

Setiap instalasi memiliki satu perusahaan, satu gudang logis, satu database, dan mata uang IDR. Zona waktu perusahaan awal adalah Asia/Jakarta. Nominal API menggunakan string decimal, bukan floating point. Kuantitas mendukung tiga tempat desimal, biaya unit empat, dan nominal uang dua. Aplikasi belum menghitung pajak atau dokumen perpajakan.

Administrator dapat memperbarui profil perusahaan, alamat, email, telepon, nomor pajak, dan awalan nomor pesanan/invoice melalui Pengaturan. Zona waktu dapat diganti sebelum jurnal pertama; sesudahnya perubahan ditolak agar tanggal pembukuan historis tetap konsisten. Mata uang tetap IDR. Mengubah awalan nomor tidak menomori ulang dokumen lama dan tidak mereset urutan nomor.

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
- Warehouse: produk, supplier, PO, penerimaan, adjustment, pengiriman, serta reversal pengiriman.
- Finance: saldo awal, invoice penjualan, pembayaran/reversal pelanggan, tagihan supplier/reversal, jurnal manual, dan periode pembukuan.
- Viewer: akses baca; tidak dapat mengubah data.

Perubahan role, penonaktifan, dan reset password mencabut sesi pengguna. Admin tidak dapat menonaktifkan atau menurunkan role dirinya sendiri. Pengguna dapat mengganti password sendiri dengan password saat ini; sesi lama dicabut.

Jika lupa password semua administrator, jalankan `haruflow admin reset-password admin@example.com` pada host dengan konfigurasi database, atau gunakan Compose `exec app haruflow admin reset-password admin@example.com`. Password baru diminta dua kali secara tersembunyi. Pemulihan hanya berlaku pada admin aktif, mencabut sesi lama, dan membuat audit `user.password_reset_local`. Untuk otomasi, flag `--password-stdin` menerima satu baris password 12–128 byte dari stdin; jangan memasukkan password ke argumen proses atau file yang dilacak Git.

## Urutan kerja awal

1. Buat pelanggan, produk, dan akun yang diperlukan.
2. Buka Akuntansi → Saldo awal sebelum mencatat transaksi pertama. Isi stok dan saldo akun dalam draf; nilai stok otomatis menjadi debit persediaan. Masukkan akun lawan yang sesuai sampai total debit dan kredit seimbang.
3. Simpan, tinjau, lalu posting draf. Stok, biaya rata-rata, jurnal, dan audit tersimpan bersama. Baca [panduan saldo awal](./OPENING_BALANCES.md) untuk contoh kas dan persediaan serta batas pembukaan piutang. Instalasi yang sudah bertransaksi tidak menerima pembukaan tambahan.
4. Buat sales order lalu konfirmasi. Konfirmasi belum mereservasi stok.
5. Posting pengiriman penuh. Aplikasi mengunci stok, menolak stok negatif, serta membukukan HPP.
6. Buat invoice dari pengiriman lalu catat pembayaran; pembayaran sebagian didukung, uang muka dan overpayment belum didukung.
7. Untuk pembelian, buat supplier dan PO, terima barang penuh, kemudian catat tagihan supplier beserta tanggal invoice/jatuh tempo. Pembayaran supplier belum tersedia.
8. Periksa neraca saldo, kartu stok, piutang, barang diterima belum ditagih, dan utang supplier sebelum menutup periode.

Penerimaan stok manual tetap tersedia dan mengkredit modal awal; jangan gunakan sebagai pengganti purchasing/utang supplier. Hindari memasukkan barang awal dua kali melalui saldo awal dan penerimaan stok.

Menu Pembelian menyediakan supplier, PO, penerimaan penuh, serta tagihan supplier dan jatuh tempo. Pembuatan/konfirmasi PO tidak mengubah stok/jurnal dan dapat dilakukan saat periode akuntansi tertutup. Supplier aktif diperlukan saat membuat/mengubah draf atau mengonfirmasi PO. PO dikonfirmasi dikunci; penerimaan memakai seluruh jumlah/biaya tersimpan dan dapat diproses walaupun supplier kemudian nonaktif. Penerimaan memposting stok serta jurnal debit persediaan/kredit barang diterima belum ditagih, tanpa kas.

Finance/Admin mencatat tepat satu tagihan aktif penuh per penerimaan aktif. Barang/biaya/total berasal dari snapshot penerimaan; supplier nonaktif dan mutasi stok lanjutan tidak menghalangi penagihan barang yang telah diterima. Jurnal debit barang diterima belum ditagih/kredit utang supplier memindahkan kewajiban tanpa menambah stok atau mengurangi kas. Nomor invoice supplier harus unik per supplier untuk tagihan aktif setelah trim dan tanpa membedakan huruf besar/kecil. Tanggal invoice/jatuh tempo menggunakan `YYYY-MM-DD` tahun 0001–9999, jatuh tempo tidak sebelum invoice, dan invoice tidak melewati hari perusahaan. Invoice bertanggal lampau tetap memposting jurnal saat ini pada periode terbuka. Ringkasan/chart utang memakai seluruh tagihan dan zona waktu perusahaan; daftar/CSV terbatas 500 terbaru. Pembayaran supplier menyusul; lihat [panduan pembelian](./PURCHASING.md).

Produk berjenis jasa/nonstok tidak memiliki saldo persediaan, biaya rata-rata, atau HPP stok. Pesanan yang seluruh isinya jasa dapat dibuatkan invoice sesudah konfirmasi. Pesanan campuran barang dan jasa tetap memerlukan pengiriman barang sebelum invoice, dan hanya baris barang yang mengurangi stok. Penerimaan dan penyesuaian persediaan menolak produk jasa. Jenis produk tidak dapat diganti setelah digunakan pada pesanan penjualan, PO, atau mutasi stok; jenis pada dokumen lama disimpan sebagai snapshot.

Jurnal manual tidak boleh langsung mengubah akun kontrol piutang (1200), persediaan (1300), barang diterima belum ditagih, atau utang supplier. Akun penampung penerimaan PO dipilih dari kode bebas 2110–2199 dan akun utang supplier dari 2200–2299; masing-masing memakai kode sepuluh digit kosong bila rentangnya penuh. Akun pengguna yang ada tidak diambil alih. Kedua akun kontrol sistem tersebut juga tidak dapat diisi lewat saldo awal. Gunakan dokumen operasional agar saldo subledger tetap cocok; pembukaan utang historis per supplier belum tersedia.

## Koreksi transaksi dan penutupan periode

Dokumen posted, jurnal, pembayaran, audit, dan pergerakan stok tidak dihapus dari riwayat. Reversal mencatat transaksi lawan serta alasan. Untuk membatalkan transaksi penjualan yang sudah lunas, balik pembayaran terlebih dahulu, kemudian invoice, lalu pengiriman. Order draft/confirmed dapat dibatalkan bila belum memiliki transaksi turunan aktif.

Reversal pengiriman mengembalikan biaya historis pengiriman ke nilai persediaan saat ini dan menghitung ulang biaya rata-rata. Reversal berbeda dari retur penjualan komersial; retur belum tersedia.

Finance dapat membalik jurnal manual melalui Jurnal umum; seluruh baris dibalik persis dan jurnal asal tetap terhubung ke jurnal pembalik. Warehouse dapat membatalkan penerimaan yang memiliki snapshot biaya sebelum penerimaan dan belum diikuti mutasi stok apa pun pada produk yang sama. Pembatalan mengembalikan jumlah, nilai, dan biaya rata-rata sebelum penerimaan. Penerimaan lama tanpa snapshot tidak ditebak biaya historisnya; gunakan penyesuaian stok setelah memeriksa transaksi terkait. Lihat [panduan koreksi dan chart](./CORRECTIONS_AND_CHARTS.md).

Penerimaan PO dikoreksi melalui riwayat Pembelian, bukan tombol pembatalan penerimaan stok manual. Tagihan supplier aktif harus dibalik terlebih dahulu. Seluruh barang harus belum memiliki mutasi lanjutan; snapshot jumlah/nilai/biaya, versi PO, serta periode terbuka diperiksa bersama. Pembalikan menambah mutasi/jurnal lawan, memulihkan biaya persis, dan mengembalikan PO ke Dikonfirmasi. Penerimaan asal tetap tersimpan dan dapat digantikan dokumen GR baru. PO dengan penerimaan aktif tidak dapat dibatalkan langsung. Penyesuaian stok tidak otomatis membalik kewajiban penerimaan PO.

Tagihan supplier tidak dapat diedit/dihapus setelah posting. Finance/Admin dapat membalik tagihan beralasan pada periode terbuka: jurnal asal dibalik persis, status menjadi Dibatalkan (`void`), dan sisa utangnya nol. Stok/kas tidak berubah. Dokumen, alasan, serta tautan jurnal asal/pembalik tetap tersimpan. Tagihan pengganti dapat diterbitkan atas penerimaan yang sama; nomor invoice supplier yang sudah void boleh dipakai kembali. Pembalikan tagihan tidak menghapus syarat koreksi stok. Posting/pembalikan tagihan juga menaikkan versi PO sehingga pengguna gudang perlu memuat versi baru sebelum bertindak.

Saldo awal hanya dapat dibalik sebelum ada jurnal atau pergerakan stok lain. Pembalikan memulihkan saldo ke nol, menyimpan alasan serta snapshot posting, dan mengembalikan dokumen menjadi draf untuk koreksi. Dokumen pembukaan hanya satu per perusahaan; posting ulang setelah pembalikan tetap memakai konteks yang sama. Setelah transaksi operasional berjalan, koreksi pembukaan tidak dilakukan dengan mereset saldo.

Penutupan periode menolak posting baru sampai tanggal yang ditutup, berdasarkan zona waktu perusahaan. Tanggal tutup hanya dapat maju dan tidak dapat dibuka kembali melalui UI. Semua jurnal menggunakan waktu transaksi saat ini; tanggal invoice supplier yang lampau tidak melakukan backdating jurnal. Menutup tanggal hari ini menghentikan posting sampai hari berikutnya. Lakukan pemeriksaan dan backup sebelum menutup periode.

## Backup

Jalankan `node scripts/database.mjs backup` dari root repository. Script menjalankan `pg_dump -Fc`, menulis arsip pada folder `backups/`, dan membuat metadata waktu serta versi aplikasi. Nama file dapat diberikan sebagai argumen kedua. Arsip ditulis ke file `.partial` secara eksklusif dan nama final baru diterbitkan setelah dump selesai. File final atau `.partial` yang sudah ada tidak ditimpa atau dihapus. Gunakan filesystem yang mendukung hard link untuk lokasi backup; file sementara dan final berada di direktori yang sama.

Cadangkan arsip, versi/tag atau digest image aplikasi, dan konfigurasi yang diperlukan ke lokasi lain yang terenkripsi. `.env` berisi secrets dan harus disimpan dengan perlindungan terpisah. Backup dalam disk yang sama tidak melindungi dari kegagalan disk.

Operator menentukan jadwal dan retensi, misalnya backup harian serta sebelum upgrade. Jadwal otomatis dan PITR tidak dikonfigurasi oleh instalasi default. Dokumentasikan kehilangan data maksimum yang dapat diterima sebelum menetapkan interval.

## Restore ke instalasi pengujian

1. Siapkan checkout dan image versi yang sesuai backup pada mesin/direktori lain, dengan nama proyek Compose dan volume yang berbeda. Buat `.env` untuk instalasi tujuan.
2. Jalankan PostgreSQL tujuan. Jangan menjalankan aplikasi penerima penulisan selama restore.
3. Jalankan `node scripts/database.mjs restore path/arsip.backup --confirm-restore`. Script menghentikan aplikasi tujuan, melakukan restore satu transaksi, dan membiarkan aplikasi berhenti.
4. Pastikan command berhasil. Kegagalan restore tidak boleh diabaikan; jangan menyalakan aplikasi sebelum hasilnya jelas.
5. Jalankan `docker compose --env-file .env -f deploy/community/compose.yml up -d app`. Perintah ini juga membuat container aplikasi bila instalasi tujuan baru menjalankan PostgreSQL.
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

Migrasi `0008_opening_balances.sql` menambahkan dokumen/snapshot saldo awal dan jenis pergerakan baru. Migrasi tidak mengubah saldo maupun dokumen lama. Perusahaan yang telah memiliki jurnal tetap memakai saldo historisnya; fitur saldo awal akan menunjukkan alasan mengapa pembukaan baru diblokir. Versi sebelum fitur ini tidak memahami posting saldo awal baru, sehingga jangan menurunkan versi executable setelah fitur dipakai tanpa pemulihan backup yang sesuai.

Migrasi `0009_document_corrections.sql` menambahkan urutan mutasi yang monoton dan snapshot posisi sebelum penerimaan baru. Pengisian urutan riwayat lama tidak mengubah jumlah atau nilai transaksi. Penerimaan yang sudah ada tetap memiliki snapshot kosong dan tidak dapat dibatalkan melalui fasilitas baru. Setelah jenis pembalikan baru dipakai, versi sebelum migrasi ini memerlukan restore backup yang sesuai untuk penurunan versi.

Migrasi `0010_purchasing.sql` menambahkan supplier, PO, baris snapshot, serta urutan nomor pembelian. Migrasi ini tidak memposting stok atau jurnal. Setelah dipakai, referensi PO ikut mengunci jenis produk dan mencegah penghapusannya. Gunakan executable yang mengenali referensi pembelian tersebut; untuk penurunan versi, pulihkan backup beserta versi aplikasi yang cocok.

Migrasi `0011_purchase_receipts.sql` menambahkan penerimaan PO, snapshot biaya, status Diterima, metadata akun kontrol, dan fungsi riwayat/koreksi. Akun barang diterima belum ditagih dibuat pada kode kosong 2110–2199, atau kode sepuluh digit kosong bila rentang tersebut penuh; akun pengguna tidak diubah. Migrasi tidak memposting ulang transaksi lama. Jalankan setelah menghentikan aplikasi dan backup. Setelah penerimaan baru dipakai, downgrade memerlukan restore backup serta versi aplikasi yang cocok. Uji restore menyertakan snapshot penerimaan dan saldo akun penampung.

Migrasi `0012_supplier_bills.sql` menambahkan tagihan supplier, tanggal invoice/jatuh tempo, urutan PB, aturan unik tagihan aktif, perlindungan dokumen posted, serta metadata penagihan dan blokir koreksi pada penerimaan. Akun kontrol Utang supplier dibuat pada kode kosong 2200–2299 atau kode sepuluh digit kosong tanpa mengambil alih akun pengguna/akun penampung penerimaan. Migrasi tidak membuat tagihan dari penerimaan lama dan tidak mengubah stok, jurnal, atau saldo yang telah ada. Penerimaan lama yang masih aktif dapat ditagih melalui alur baru. Setelah tagihan dipakai, downgrade harus memulihkan backup beserta aplikasi yang sesuai; versi sebelumnya tidak mengenali jurnal tagihan atau ketergantungan koreksinya. Saat memverifikasi restore, periksa dokumen/riwayat pembalikan tagihan, jatuh tempo, nilai belum ditagih, serta saldo akun penampung dan utang supplier.

## Health, log, dan troubleshooting

- `/health/live`: proses HTTP hidup.
- `/health/ready`: koneksi database dapat melakukan query.
- `haruflow healthcheck`: probe readiness untuk container.
- `docker compose --env-file .env -f deploy/community/compose.yml logs --tail=100 app`: log aplikasi.

Jika tabel tidak ditemukan, jalankan migrasi sesuai prosedur. Jika setup ditolak, periksa token lokal dan apakah perusahaan sudah dibuat. Jika login berhasil tetapi sesi tidak bertahan pada HTTP lokal, pastikan cookie Secure tidak diaktifkan untuk lingkungan HTTP itu. Jika posting ditolak karena periode tertutup, periksa tanggal tutup; sistem tidak mengubahnya diam-diam.

Saat melaporkan bug sertakan versi commit/image, langkah reproduksi, request ID bila tersedia, dan pesan error yang sudah disanitasi. Jangan melampirkan `.env`, password, cookie sesi, atau dump data pelanggan ke issue publik.
