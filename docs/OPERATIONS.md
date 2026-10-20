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

Cloudflare Tunnel bersifat opsional. Sesuaikan batas ukuran request dan timeout proxy dengan endpoint yang dipakai. Body API biasa dibatasi 256 KiB; preview impor menerima envelope JSON sampai 2 MiB untuk menampung escaping, dengan isi CSV tetap maksimal 256 KiB. Endpoint unggah lampiran menerima isi file sampai 5 MiB. Proxy harus meneruskan request valid tersebut; lihat [impor](./IMPORTS.md) dan [lampiran](./ATTACHMENTS.md) untuk batas rinci.

## Setup dan role

Form setup memerlukan token lokal dan membuat satu perusahaan sekaligus admin pertama. Tidak ada akun atau password bawaan. Simpan akses admin dan backup secrets di tempat yang dikendalikan operator.

- Administrator: seluruh aksi serta manajemen pengguna.
- Sales: pelanggan, pembuatan dan konfirmasi sales order.
- Warehouse: produk, supplier, PO, penerimaan, adjustment, pengiriman, serta reversal pengiriman.
- Finance: saldo awal, invoice penjualan, tagihan supplier, pembayaran/reversal pelanggan dan supplier, retur kedua pihak, jurnal manual, periode pembukuan, serta klasifikasi arus kas beralasan.
- Viewer: akses baca; tidak dapat mengubah data.

Perubahan role, penonaktifan, dan reset password mencabut sesi pengguna. Admin tidak dapat menonaktifkan atau menurunkan role dirinya sendiri. Pengguna dapat mengganti password sendiri dengan password saat ini; sesi lama dicabut.

Jika lupa password semua administrator, jalankan `haruflow admin reset-password admin@example.com` pada host dengan konfigurasi database, atau gunakan Compose `exec app haruflow admin reset-password admin@example.com`. Password baru diminta dua kali secara tersembunyi. Pemulihan hanya berlaku pada admin aktif, mencabut sesi lama, dan membuat audit `user.password_reset_local`. Untuk otomasi, flag `--password-stdin` menerima satu baris password 12–128 byte dari stdin; jangan memasukkan password ke argumen proses atau file yang dilacak Git.

## Urutan kerja awal

1. Buat pelanggan, produk, dan akun yang diperlukan.
2. Buka **Akuntansi → Saldo awal** sebelum transaksi pertama. Isi stok dan saldo akun dalam draf; nilai stok otomatis menjadi debit persediaan. Lengkapi akun lawan sampai total debit dan kredit seimbang.
3. Simpan, tinjau, lalu posting. Stok, biaya rata-rata, jurnal, dan audit tersimpan bersama. Baca [panduan saldo awal](./OPENING_BALANCES.md) untuk batas pembukaan piutang dan utang. Instalasi yang sudah bertransaksi tidak menerima pembukaan tambahan.
4. Buat sales order lalu konfirmasi. Konfirmasi belum mereservasi stok.
5. Posting pengiriman sebagian atau penuh. Sistem mengunci stok, menolak stok negatif, membukukan HPP, serta menyimpan sisa kirim pada pesanan yang sama.
6. Buat invoice sebagian atau penuh dari jumlah yang sudah dikirim; baris jasa dapat ditagih sesudah konfirmasi. Catat pembayaran sebagian/penuh sampai sisa piutang. Uang muka dan overpayment belum didukung. Lihat [pemenuhan penjualan](./SALES_FULFILLMENT.md).
7. Untuk pembelian, buat supplier dan PO; terima barang sebagian/penuh; catat tagihan atas bagian yang diterima beserta invoice/jatuh tempo; kemudian bayar sebagian/penuh dari tagihan. Sisa penerimaan, penagihan, dan utang tetap terlacak.
8. Catat pengembalian komersial dari invoice/tagihan asal melalui Retur. Periksa kredit dan konfirmasi pengembalian uang aktual bila diperlukan; lihat [panduan retur](./RETURNS.md).
9. Periksa kartu stok, piutang, kewajiban penerimaan belum ditagih, utang supplier/umur utang, neraca saldo, serta **Akuntansi → Laporan keuangan** sebelum menutup periode.

Penerimaan stok manual tetap mengkredit modal awal; jangan gunakan sebagai pengganti purchasing/utang supplier atau memasukkan barang awal dua kali setelah saldo awal.

## Pembelian dan pembayaran supplier

Pembuatan/konfirmasi PO tidak mengubah stok atau jurnal dan dapat dilakukan saat periode tertutup. Supplier aktif diperlukan saat membuat/mengubah draf dan konfirmasi. PO dikonfirmasi dikunci; penerimaan boleh sebagian atau penuh memakai biaya serta satuan tersimpan, termasuk ketika supplier kemudian nonaktif. Satuan katalog yang berbeda ditolak. Penerimaan memposting debit Persediaan 1300/kredit Barang diterima belum ditagih, tanpa mengubah kas. Status PO menjadi Diterima sebagian sampai seluruh jumlah diterima.

Finance/Admin dapat mencatat beberapa tagihan untuk suatu penerimaan sampai seluruh jumlahnya ditagih. Tagihan tidak boleh melampaui sisa penerimaan atau memakai baris dari penerimaan lain. Barang/biaya/nilai berasal dari snapshot penerimaan; stok yang sudah bergerak tidak menghalangi penagihan. Jurnal debit Barang diterima belum ditagih/kredit Utang supplier memindahkan kewajiban tanpa menambah stok atau mengurangi kas. Pembulatan kumulatif menjaga seluruh alokasi tetap sesuai nilai dokumen asal; setiap bagian harus memiliki nilai positif, dan sisa jumlah tetap mendapat minimal Rp0,01 sebelum bagian terakhir diproses.

Nomor invoice supplier unik per supplier untuk seluruh tagihan yang belum dibatalkan, termasuk yang sudah lunas, setelah trim dan tanpa membedakan huruf besar/kecil. Tanggal invoice/jatuh tempo memakai `YYYY-MM-DD` tahun 0001–9999; jatuh tempo tidak sebelum invoice dan tanggal invoice tidak melewati hari perusahaan. Invoice lampau tetap memposting jurnal saat ini pada periode terbuka.

Pembayaran supplier mengurangi Kas 1100 dan Utang supplier, dengan nominal positif sampai sisa tagihan dan maksimal Rp1 triliun per pembayaran. Saldo kas harus cukup. Aplikasi mencatat dana yang dibayarkan, bukan mentransfer uang otomatis. Pembayaran berulang memakai kunci yang sama tidak mencatat dua transaksi. Riwayat dokumen, jurnal, referensi, dan pembalikan beralasan tetap tersedia.

Retur supplier mengurangi stok pada biaya rata-rata saat retur; kredit mengikuti nilai tagihan asal. Selisih nilai persediaan dan kredit masuk akun sistem beban **Selisih nilai retur pembelian**. Bila pembayaran sudah melebihi nilai tagihan bersih setelah kredit, refund langsung mencatat kas masuk dan mengurangi pembayaran bersih, dengan konfirmasi dana aktual. Refund tertunda dan kredit lintas dokumen belum tersedia. Retur tidak membuka kembali pemenuhan PO atau penagihan asal.

Daftar PO/tagihan, pencarian, dan CSV daftar dibatasi 500 terbaru. Ringkasan/chart memakai seluruh dokumen dalam satu snapshot. Umur utang dihitung dari sisa setelah pembayaran serta kredit retur, pada hari perusahaan saat ini: belum terlambat, 1–30, 31–60, 61–90, dan lebih dari 90 hari. Hari jatuh tempo belum terlambat. Analisis menyediakan rincian/CSV serta kas supplier 3/6/12 bulan, termasuk pembayaran, refund, dan pembalikannya. Lihat [panduan pembelian](./PURCHASING.md).

Produk jasa/nonstok tidak memiliki saldo persediaan, biaya rata-rata, atau HPP stok. Jasa penjualan dapat ditagih sesudah konfirmasi tanpa pengiriman; barang hanya dapat ditagih sampai jumlah yang dikirim. Pesanan campuran mendukung pemilihan baris yang memenuhi syarat. Jenis produk tidak dapat diganti setelah digunakan pada pesanan, PO, atau mutasi. Pembelian jasa belum tersedia.

Jurnal manual dan saldo awal tidak dapat langsung mengisi akun kontrol piutang 1200, persediaan 1300, atau akun dengan peran sistem. Akun penampung penerimaan PO dipilih dari kode bebas 2110–2199, Utang supplier dari 2200–2299, dan Selisih nilai retur pembelian dari 5900–5999; masing-masing memakai kode sepuluh digit kosong bila rentangnya penuh. Akun pengguna yang ada tidak diambil alih. Gunakan alur operasional agar saldo subledger tetap cocok; pembukaan piutang/utang historis per pihak belum tersedia.

## Koreksi transaksi dan penutupan periode

Dokumen posted, jurnal, pembayaran, audit, dan mutasi stok tidak dihapus dari riwayat. Pembalikan mencatat transaksi lawan serta alasan. Versi dokumen, kunci idempotensi, saldo, dan periode diperiksa bersama; kegagalan tidak meninggalkan sebagian stok, kas, atau jurnal.

Untuk koreksi penjualan, balikkan retur aktif terlebih dahulu, kemudian pembayaran bila ada, lalu invoice; pengiriman dapat dibalik per dokumen setelah alokasi invoice aktifnya dibatalkan. Pembalikan pengiriman mengembalikan biaya historis ke persediaan dan menghitung ulang biaya rata-rata. Retur komersial mempunyai alur tersendiri yang tidak membuka kembali jumlah pemenuhan pada pesanan.

Untuk pembelian, balikkan **retur aktif → pembayaran aktif → tagihan**, sesuai transaksi yang ada. Tagihan tidak dapat diedit/dihapus setelah posting. Pembalikannya menambah jurnal lawan persis, menjadikan status Dibatalkan, memulihkan jumlah penerimaan yang dapat ditagih, dan tidak mengubah stok atau kas. PB pengganti dapat memakai nomor invoice supplier yang telah dibatalkan. Pembayaran yang dibalik mengembalikan kas dan membuka kembali sisa utang.

Penerimaan PO dikoreksi melalui riwayat Pembelian setelah seluruh tagihan aktif yang menggunakannya dibalik. Setiap produk harus belum mempunyai mutasi setelah penerimaan dan posisi jumlah/nilai/biaya harus cocok dengan snapshot. Pembalikan memulihkan biaya persis, menambah mutasi/jurnal lawan, serta menghitung ulang sisa PO. Status menjadi Dikonfirmasi jika tidak ada penerimaan aktif, atau Diterima sebagian jika masih ada bagian diterima. PO dengan penerimaan aktif tidak dapat dibatalkan langsung.

Pembalikan retur barang juga membutuhkan kecocokan snapshot dan tanpa mutasi berikutnya. Mutasi yang kemudian sudah dibalik tetap dihitung sebagai riwayat lanjutan. Jika retur supplier sebelumnya mencatat refund, pembalikannya mengeluarkan kas kembali sehingga saldo kas harus cukup; konfirmasikan perpindahan uang aktual. Membalik retur, pembayaran, atau tagihan tidak menjamin penerimaan stok lama dapat diputar ulang. Periksa alasan pemblokiran, jurnal, dan pergerakan sumber. Penyesuaian stok tidak otomatis menghapus utang atau kewajiban penerimaan PO.

Finance dapat membalik jurnal manual melalui Jurnal umum; seluruh baris dibalik persis dan jurnal asal tetap tertaut. Warehouse dapat membalik penerimaan stok manual yang memiliki snapshot sebelum penerimaan dan belum diikuti mutasi stok pada produk yang sama. Penerimaan lama tanpa snapshot tidak ditebak biaya historisnya. Lihat [panduan koreksi dan chart](./CORRECTIONS_AND_CHARTS.md).

Saldo awal hanya dapat dibalik sebelum jurnal atau mutasi lain. Pembalikan memulihkan saldo ke nol, menyimpan alasan/snapshot, dan mengembalikan dokumen pembukaan yang sama menjadi draf. Setelah operasional berjalan, koreksi tidak dilakukan dengan mereset saldo.

Penutupan periode menolak posting baru sampai tanggal yang ditutup, menurut zona waktu perusahaan. Tanggal tutup hanya maju dan tidak dapat dibuka kembali melalui UI. Jurnal memakai waktu saat transaksi diposting; tanggal invoice lampau tidak melakukan backdating. Menutup hari ini menghentikan posting sampai hari berikutnya. Periksa pembukuan dan buat backup sebelum penutupan.

## Laporan keuangan dan pemeriksaan saldo

**Akuntansi → Laporan keuangan** menyediakan laba rugi, neraca, arus kas, CSV, dan rincian sumber. Laba rugi serta arus kas memakai rentang tanggal posting dalam zona waktu perusahaan. Neraca memakai seluruh jurnal sampai tanggal akhir, termasuk saldo awal dan transaksi sebelum tanggal awal filter.

Laba rugi mencakup seluruh akun pendapatan/beban, termasuk akun tambahan dan selisih retur pembelian. Pembayaran utang tidak menjadi beban untuk kedua kali. Neraca memeriksa aset terhadap liabilitas, ekuitas tercatat, dan laba/rugi akumulasi. Arus kas metode langsung mencakup Kas 1100; akun bank/aset tambahan belum otomatis termasuk kas/setara kas.

Finance/Admin dapat memberi klasifikasi arus kas dengan alasan dan versi. Klasifikasi tidak mengubah jurnal atau nominal dan menyimpan riwayat, termasuk ketika periode sudah tertutup. Kelompok belum terklasifikasi tetap masuk rekonsiliasi kas. CSV laporan tidak dibatasi 500 dokumen operasional. Baca [panduan laporan keuangan](./FINANCIAL_REPORTS.md) untuk cakupan dan cara menelusuri angka.

## Backup

Lampiran bukti disimpan dalam PostgreSQL bersama metadata dan riwayatnya; tidak ada folder upload terpisah. Backup database yang sama menyertakan file aktif/arsip, preview/riwayat impor, serta master hasil impor. Ukuran dan durasi backup bertambah mengikuti file yang disimpan. Setelah restore, cocokkan juga ukuran dan SHA-256 contoh lampiran dengan unduhannya, bukan hanya nama file. Lihat [impor](./IMPORTS.md) dan [lampiran](./ATTACHMENTS.md).

Jalankan `node scripts/database.mjs backup` dari root repository. Script menjalankan `pg_dump -Fc`, menulis arsip pada folder `backups/`, dan membuat metadata waktu serta versi aplikasi. Nama file dapat diberikan sebagai argumen kedua. Arsip ditulis ke file `.partial` secara eksklusif dan nama final baru diterbitkan setelah dump selesai. File final atau `.partial` yang sudah ada tidak ditimpa atau dihapus. Gunakan filesystem yang mendukung hard link untuk lokasi backup; file sementara dan final berada di direktori yang sama.

Cadangkan arsip, versi/tag atau digest image aplikasi, dan konfigurasi yang diperlukan ke lokasi lain yang terenkripsi. `.env` berisi secrets dan harus disimpan dengan perlindungan terpisah. Backup dalam disk yang sama tidak melindungi dari kegagalan disk.

Operator menentukan jadwal dan retensi, misalnya backup harian serta sebelum upgrade. Jadwal otomatis dan PITR tidak dikonfigurasi oleh instalasi default. Dokumentasikan kehilangan data maksimum yang dapat diterima sebelum menetapkan interval.

## Restore ke instalasi pengujian

1. Siapkan checkout dan image versi yang sesuai backup pada mesin/direktori lain, dengan nama proyek Compose dan volume yang berbeda. Buat `.env` untuk instalasi tujuan.
2. Jalankan PostgreSQL tujuan. Jangan menjalankan aplikasi penerima penulisan selama restore.
3. Jalankan `node scripts/database.mjs restore path/arsip.backup --confirm-restore`. Script menghentikan aplikasi tujuan, melakukan restore satu transaksi, dan membiarkan aplikasi berhenti.
4. Pastikan command berhasil. Kegagalan restore tidak boleh diabaikan; jangan menyalakan aplikasi sebelum hasilnya jelas.
5. Jalankan `docker compose --env-file .env -f deploy/community/compose.yml up -d app`. Perintah ini juga membuat container aplikasi bila instalasi tujuan baru menjalankan PostgreSQL.
6. Periksa `/health/ready`, login, dokumen dan riwayatnya, jumlah/nilai stok, piutang, kewajiban penerimaan belum ditagih, utang bersih, kas, serta debit/kredit neraca saldo terhadap sumber backup. Cocokkan alokasi pengiriman/penerimaan/invoice/tagihan sebagian, pembayaran, kredit/refund retur, dan jurnal pembaliknya. Periksa juga laporan keuangan serta riwayat klasifikasi arus kas.
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

Migrasi 0013–0018 melengkapi pembayaran, transaksi sebagian, laporan, dan retur. Jalankan berurutan dengan aplikasi berhenti setelah backup; jangan menyalin executable lama di atas database yang telah menerima jenis posting baru.

- `0013_supplier_payments.sql` menambahkan saldo terbayar, status sebagian/lunas, dokumen pembayaran supplier, dan perlindungan riwayat. Tagihan lama tetap mempunyai pembayaran nol dan tidak dibayar otomatis.
- `0014_partial_sales.sql` menambahkan alokasi pengiriman dan invoice per baris serta versi pesanan. Dokumen penuh yang sudah ada diisi alokasinya tanpa mengubah jumlah/nilai stok, nomor, jurnal, pembayaran, atau status lama. Snapshot biaya sebelum pengiriman yang memang tidak tersimpan pada versi lama tidak direkayasa sebagai transaksi baru.
- `0015_partial_purchasing.sql` menambahkan status Diterima sebagian, kemajuan baris PO, alokasi tagihan per baris penerimaan, serta beberapa tagihan per penerimaan. Tagihan penuh lama diisi baris alokasinya, termasuk yang sudah dibatalkan, tanpa memposting ulang jurnal atau stok.
- `0016_financial_reporting.sql` menambahkan keterangan dan riwayat klasifikasi arus kas. Tidak mengubah nominal, akun, tanggal posting, atau saldo jurnal yang ada. Laporan membaca ledger yang sama.
- `0017_supplier_returns.sql` menambahkan kredit pada tagihan, dokumen/baris retur supplier, snapshot untuk pembalikan aman, referensi mutasi, dan akun peran sistem Selisih nilai retur pembelian. Akun pengguna pada kode default tidak diambil alih; kredit lama tetap nol, tanpa retur atau refund otomatis.
- `0018_sales_returns.sql` menambahkan kredit/versi invoice, retur pelanggan, alokasi biaya historis, referensi mutasi, serta metadata satuan pengiriman untuk pemeriksaan retur. Dokumen dan jurnal lama dipertahankan; kredit/retur/refund tidak dibuat otomatis.

Upgrade native dari skema 0012 ke 0018 telah diperiksa dengan membandingkan snapshot stok, dokumen, dan jurnal lama. Smoke native pada database uji terpisah juga menjalankan seed → backup → mutasi → restore → verifikasi, termasuk pengiriman/penerimaan dan tagihan sebagian, pembayaran, retur, refund, serta pembalikannya; fixture hasil restore memuat 50 jurnal dan 25 mutasi. Jumlah ini milik data uji, bukan target jumlah transaksi instalasi pengguna. Operator tetap perlu mencocokkan hasil latihan restore dengan backup dan versi instalasinya sendiri.

Setelah upgrade, uji satu alur representatif pada database pengujian: kirim/terima sebagian, tagih sisanya, bayar sebagian/penuh, baca umur utang dan laporan, lalu uji koreksi yang memenuhi syarat. Pastikan dokumen lama, hubungan sumber, alasan, jurnal asal/pembalik, dan saldo tidak berubah oleh migrasi. Penurunan versi setelah fitur baru digunakan memerlukan pemulihan backup beserta aplikasi yang sesuai; tidak ada migrasi turun otomatis.

Migrasi `0019_master_imports.sql` menambahkan preview/riwayat impor yang hanya dapat berubah dari siap menjadi tersimpan. Migrasi `0020_document_attachments.sql` menambahkan isi lampiran, referensi dokumen, kuota aktif, serta riwayat arsip/pemulihan. Keduanya tidak mengubah master, dokumen, stok, atau jurnal lama. Jalankan setelah backup dan dengan aplikasi berhenti. Lampiran memakai PostgreSQL sehingga Compose tetap dapat menjalankan aplikasi dengan filesystem read-only; tidak diperlukan volume upload tambahan. Untuk menurunkan versi setelah fitur ini dipakai, pulihkan backup serta aplikasi yang sesuai agar file dan riwayat tetap dapat diakses.

Kedua database preview telah di-upgrade 0018 → 0020 setelah backup dengan jumlah baris serta hash seluruh isi tabel lama tetap sama. Smoke native terbaru juga membuktikan riwayat impor, master hasil impor, 11 lampiran aktif/arsip, 14 event file, dan kecocokan byte/SHA-256 setelah backup → perubahan data → restore. Pemeriksaan ini melengkapi saldo dan riwayat transaksi; bukan pengganti latihan restore instalasi pengguna.

Migrasi `0021_community_user_limit.sql` membatasi penambahan/pengaktifan akun ketika sudah ada 10 akun aktif, termasuk administrator. Jalankan setelah backup dengan aplikasi berhenti. Akun lama yang sudah melebihi 10 tetap aktif; tinjau dan nonaktifkan akun yang tidak diperlukan sebelum menambah pengguna lagi. Perubahan peran dan pemulihan password tetap tersedia saat penuh. Backup/restore mempertahankan status aktif/nonaktif beserta audit. Lihat [pengelolaan slot dan batas perlindungan kode](./USER_LIMIT.md).

## Health, log, dan troubleshooting

- `/health/live`: proses HTTP hidup.
- `/health/ready`: koneksi database dapat melakukan query.
- `haruflow healthcheck`: probe readiness untuk container.
- `docker compose --env-file .env -f deploy/community/compose.yml logs --tail=100 app`: log aplikasi.

Jika tabel tidak ditemukan, jalankan migrasi sesuai prosedur. Jika setup ditolak, periksa token lokal dan apakah perusahaan sudah dibuat. Jika login berhasil tetapi sesi tidak bertahan pada HTTP lokal, pastikan cookie Secure tidak diaktifkan untuk lingkungan HTTP itu. Jika posting ditolak karena periode tertutup, periksa tanggal tutup; sistem tidak mengubahnya diam-diam.

Saat melaporkan bug sertakan versi commit/image, langkah reproduksi, request ID bila tersedia, dan pesan error yang sudah disanitasi. Jangan melampirkan `.env`, password, cookie sesi, atau dump data pelanggan ke issue publik.
