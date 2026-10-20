# Rencana Haruflow Community

Status: fondasi Community, lima perluasan operasional utama, impor master CSV, dan lampiran bukti transaksi telah diimplementasikan; aplikasi masih merupakan versi pengembangan, belum rilis stabil.
Tanggal pembaruan: 20 Oktober 2026.
Dasar: arah produk dalam `haruflow.md` dan implementasi aktual repository. Rincian hasil pengujian serta batas fitur dicatat di [status implementasi](./IMPLEMENTATION_STATUS.md).

## 1. Arah produk

Haruflow Community adalah ERP gratis yang dijalankan sendiri oleh usaha kecil. Sasaran saat ini adalah usaha dagang dengan satu perusahaan, satu gudang logis, dan mata uang IDR. Produk jasa dapat dipakai pada alur penjualan tanpa persediaan.

Pengguna dapat menyiapkan perusahaan dan pengguna, memposting saldo awal, mengelola penjualan serta pembelian, mencatat pembayaran dan retur, lalu memeriksa stok, piutang/utang, dan laporan keuangan dari jurnal yang sama.

Community tidak memerlukan akun Haruflow, layanan lisensi, atau koneksi ke Haruflow Cloud. Instalasi resmi membatasi 10 akun aktif, termasuk administrator; akun nonaktif tetap menyimpan riwayat dan melepaskan slot. Lisensi tetap mengikuti `LICENSE` dalam repository. Lihat [batas pengguna dan perlindungan kode](./USER_LIMIT.md), termasuk ketentuan upgrade akun lama serta keterbatasan perlindungan instalasi self-hosted.

Pemilik instalasi mengelola server, akses jaringan/TLS, backup, secrets, dan upgrade. Saat ini tersedia source, Dockerfile/Compose, serta panduan operasional. Image rilis pada registry, installer pengguna akhir, pilot bisnis, dan SLA belum tersedia.

## 2. Lingkup yang sudah diimplementasikan

### Fondasi

- Setup perusahaan/admin, autentikasi lokal, sesi yang dapat dicabut, pengelolaan pengguna, role Administrator/Sales/Warehouse/Finance/Viewer, dan pemulihan admin melalui perintah lokal.
- Profil perusahaan, kontak, serta awalan nomor dokumen; zona waktu dapat diubah sebelum jurnal pertama. Mata uang tetap IDR.
- Pelanggan, supplier, produk barang/jasa, satuan dasar, bagan akun dasar serta akun tambahan, dan audit aktivitas penting.
- Saldo awal stok serta akun yang diperbolehkan melalui draf, pemeriksaan keseimbangan, posting atomik, dan pembalikan sebelum transaksi operasional.
- Persediaan dengan biaya rata-rata bergerak, penerimaan stok manual, adjustment, riwayat mutasi, dan koreksi berbasis snapshot ketika belum ada mutasi lanjutan.
- Jurnal berpasangan otomatis/manual, buku besar, neraca saldo, penutupan periode, pencarian, pagination UI, CSV, dan cetak invoice melalui browser.
- Satu aplikasi Rust melayani API dan frontend; Compose aplikasi/PostgreSQL, migrasi eksplisit, health check, CI, serta utilitas backup/restore tersedia.

### Lima perluasan Community yang sudah selesai diimplementasikan

1. **Pembayaran supplier sebagian/penuh.** Pembayaran dari tagihan mengurangi Kas 1100 dan utang supplier, menolak kelebihan pembayaran, menyimpan idempotensi/audit/jurnal, serta menyediakan pembalikan beralasan dengan pemeriksaan dependensi.
2. **Umur utang dan chart pembayaran.** Umur utang memakai kelompok belum lewat jatuh tempo (termasuk hari ini), terlambat 1–30/31–60/61–90/lebih dari 90 hari, rincian supplier, dan pergerakan pembayaran. Ringkasan jatuh tempo juga membedakan tagihan hari ini dari yang belum jatuh tempo. Ringkasan memakai seluruh dokumen yang relevan, bukan hanya daftar dokumen terbaru.
3. **Retur pelanggan dan supplier sebagian/penuh.** Retur berasal dari invoice/tagihan, mengurangi nilai tagihan, memperbarui persediaan bila relevan, dan mencatat refund langsung jika pembayaran melebihi nilai bersih baru. Riwayat dan jurnal pembalik tetap tersimpan. [Panduan retur](./RETURNS.md) menjelaskan biaya, pembulatan, refund aktual, dan batas koreksi.
4. **Pemenuhan serta penagihan bertahap.** Sales order dapat dikirim dan diinvoiskan sebagian/penuh dengan sisa per baris tetap terlacak. PO dapat diterima dan ditagih sebagian/penuh; satu penerimaan dapat memiliki beberapa tagihan untuk bagian jumlah yang berbeda. Alokasi dan pembulatan mencegah jumlah atau nilai ditagih dua kali. Lihat [pemenuhan penjualan](./SALES_FULFILLMENT.md) dan [pembelian](./PURCHASING.md).
5. **Laporan keuangan.** Laba rugi periode, neraca kumulatif, dan arus kas Kas 1100 tersedia dengan tanggal perusahaan, chart, CSV, serta rincian akun/jurnal. Arus kas menyediakan klasifikasi manual beralasan dengan versi dan riwayat. Lihat [panduan laporan](./FINANCIAL_REPORTS.md) untuk dasar perhitungan serta cakupannya.

Alur operasional sekarang mencakup penjualan → pengiriman → invoice → pembayaran, serta supplier → PO → penerimaan → tagihan → pembayaran. Retur merupakan transaksi komersial tersendiri; retur tidak membuka ulang jumlah pesanan yang sudah dipenuhi atau ditagih.

## 3. Batas produk saat ini

Selesainya lima perluasan di atas belum berarti seluruh rencana ERP atau persiapan rilis stabil selesai. Batas berikut masih berlaku:

- Satu perusahaan, satu gudang logis, dan IDR. Belum ada banyak gudang/cabang, transfer gudang, multicurrency, atau konsolidasi perusahaan.
- Pembelian berfokus pada barang stok dari PO. Tagihan mengikuti jumlah dan nilai penerimaan; pembelian jasa, selisih harga invoice, ongkir, diskon, dan pajak otomatis belum tersedia.
- Belum ada uang muka, overpayment, refund tertunda, atau saldo kredit pelanggan/supplier yang digunakan pada dokumen lain. Refund pada retur saat ini harus mencerminkan uang yang benar-benar sudah berpindah.
- Impor CSV saat ini hanya membuat master pelanggan/supplier/produk; belum ada impor dokumen transaksi. Saldo awal piutang per pelanggan dan utang per supplier belum tersedia. Saldo awal akun umum tidak menggantikan rincian kewajiban per pihak.
- Satuan dasar belum mempunyai konversi. Lot/batch, serial number, tanggal kedaluwarsa, dan reservasi stok belum tersedia.
- Arus kas terbatas pada Kas 1100. Akun aset lain belum otomatis menjadi kas/setara kas. Klasifikasi manual berlaku untuk satu jurnal secara utuh; pembagian satu jurnal ke beberapa aktivitas belum tersedia. Laporan belum menyatakan kepatuhan terhadap format pelaporan atau perpajakan tertentu.
- Belum ada HR/payroll, produksi, POS offline, aplikasi mobile native, integrasi bank/pajak, SSO, marketplace plugin, email, atau jobs/outbox. Lampiran berada di PostgreSQL; storage eksternal dan retensi otomatis belum tersedia.
- Listing master belum mempunyai pagination server. Beberapa daftar operasional dibatasi pada dokumen terbaru; rincian buku besar memberi penanda bila tampilan terpotong, sementara saldo dan ringkasan tetap dihitung dari seluruh data yang sesuai.

Fitur lanjutan dapat tetap dikembangkan dalam Community. Batas di atas menjelaskan lingkup aktual serta urutan pekerjaan, bukan keputusan permanen pemisahan lisensi Enterprise.

## 4. Arsitektur yang dipakai

- Frontend React + TypeScript + Vite; Bun untuk dependensi, pengujian, dan build.
- Backend Rust + Axum + Tokio, dengan Tower/tower-http untuk middleware.
- PostgreSQL + SQLx, migrasi tersimpan di repository, dan transaksi database bersama dalam modular monolith.
- Aplikasi Rust menyajikan API HTTP/JSON dan hasil build frontend. Runtime Compose menjalankan aplikasi serta PostgreSQL; Redis, Kubernetes, dan message broker tidak diperlukan.
- Implementasi utama berada di `apps/web/`, `crates/server/`, `migrations/`, `deploy/community/`, `scripts/`, dan `docs/`.

OpenAPI dan client TypeScript yang dihasilkan dari kontrak belum tersedia. Antrean pekerjaan, outbox, atau worker terpisah belum diimplementasikan; pengembangannya menunggu kebutuhan integrasi yang konkret. Versi dependensi serta image dasar mengikuti berkas kunci dan konfigurasi repository.

## 5. Aturan domain yang dipertahankan

### Transaksi dan angka

- Mutasi bisnis melalui backend, bukan penulisan database langsung dari browser.
- Nominal serta jumlah menggunakan decimal dengan skala dan pembulatan eksplisit. Label serta preview finansial frontend yang baru memakai perhitungan sen yang presisi.
- Posting menyimpan dokumen, stok/ledger, saldo, audit, dan hasil idempotensi dalam satu transaksi. Request ulang tidak boleh membuat posting baru; versi lama tidak boleh menimpa dokumen yang sudah berubah.
- Constraint database dan penguncian terurut melindungi mutasi bersamaan. Stok tidak boleh negatif; sales order belum mereservasi stok sebelum pengiriman.

### Dokumen, stok, dan akuntansi

- Pengiriman, penerimaan, dan invoice mempunyai alokasi jumlah masing-masing. Jumlah terakumulasi tidak boleh melampaui pesanan atau sumbernya. Barang yang belum dikirim tidak dapat diinvoiskan; jasa dapat ditagih setelah konfirmasi tanpa mutasi stok.
- Penerimaan PO mendebit persediaan dan mengkredit barang diterima belum ditagih. Tagihan memindahkan kewajiban ke utang supplier; pembayaran mengurangi utang serta kas. Penerimaan manual tetap mengikuti akun lawan alurnya sendiri.
- Invoice penjualan mencatat piutang dan pendapatan; pengiriman mencatat HPP dan pengurangan persediaan; pembayaran mengurangi piutang dan menambah kas.
- Retur pelanggan atas barang memakai biaya historis pengiriman. Retur supplier memakai biaya rata-rata stok saat retur; selisih terhadap kredit tagihan dicatat pada akun selisih retur pembelian. Jasa tidak membuat mutasi persediaan.
- Pembayaran dibatasi sisa tagihan bersih setelah retur. Pengembalian uang dicatat bersama retur bila pembayaran yang sudah ada melebihi nilai bersih baru.
- Setiap jurnal posted seimbang. Posting dan pembalikan memakai tanggal saat ini menurut zona waktu perusahaan; periode tertutup menolak posting baru. Tanggal invoice supplier tidak memundurkan jurnal.
- Dokumen posted tidak diedit atau dihapus. Koreksi memakai reversal beralasan yang merujuk dokumen asal. Retur aktif harus dibalik sebelum pembalikan pembayaran/invoice/tagihan; dependensi alokasi serta mutasi stok lanjutan tetap diperiksa.
- Pembalikan stok harus cocok dengan snapshot dan belum mempunyai mutasi berikutnya. Membalik transaksi lanjutan tidak menghapus riwayat mutasinya.
- Saldo awal dapat dikoreksi melalui reversal sebelum aktivitas operasional. Persediaan awal harus cocok dengan akun kontrol persediaan; akun kontrol operasional lainnya dilindungi dari jurnal manual/saldo awal yang memutus rincian sumbernya.
- Laporan memakai ledger dan dokumen posted dengan tanggal serta zona waktu yang jelas. Laba rugi memakai periode, neraca memakai akumulasi sampai tanggal akhir, dan arus kas mencakup seluruh pergerakan Kas 1100 dalam cakupannya.

## 6. Prioritas Community yang masih perlu dikerjakan

Urutan berikut menjadi acuan setelah pembayaran, umur utang, retur, pemenuhan bertahap, dan tiga laporan keuangan tersedia. Prioritas akhirnya mengikuti kebutuhan pilot pengguna; daftar ini tidak menambah implementasi pada upgrade yang sedang diselesaikan.

### Prioritas 1 — Kesiapan rilis dan pemakaian nyata

- Selesaikan review kandidat rilis, catatan perubahan, pengelompokan batas fitur, dan panduan upgrade dari versi sebelumnya.
- Terbitkan artefak/image bertag versi tetap beserta checksum; tentukan kebutuhan installer untuk pengguna sasaran.
- Jalankan pilot dengan usaha dagang dan dataset yang mewakili transaksi nyata. Catat perangkat, ukuran data, serta hasil pengukuran baca/posting dan akses bersamaan.
- Jadwalkan latihan pemulihan pada lingkungan terpisah dan pemeriksaan upgrade rutin. Backup/restore dasar sudah mempunyai utilitas serta smoke test; keberhasilan itu belum menggantikan latihan operasional dan pengukuran kapasitas.
- Uji lebih lanjut kegagalan koneksi, timeout, restart ketika posting, dan kebutuhan beberapa instance. Penambahan worker hanya diperlukan ketika ada pekerjaan latar belakang.

### Prioritas 2 — Migrasi data dan bukti transaksi

- Impor master pelanggan/supplier/produk sudah menyediakan template, preview, validasi per baris, duplikasi, commit atomik, dan riwayat. Lihat [panduan impor](./IMPORTS.md). Impor dokumen transaksi masih perlu dikembangkan sesuai kebutuhan pilot.
- Saldo awal piutang per pelanggan dan utang per supplier yang terhubung ke ledger serta pembayaran berikutnya.
- Lampiran bukti transaksi PDF/PNG/JPEG sudah tersedia dengan hak tulis per jenis dokumen, unduhan terautentikasi, arsip/pemulihan, serta isi file dalam backup PostgreSQL. Lihat [panduan lampiran](./ATTACHMENTS.md). Retensi/penghapusan permanen dan storage eksternal belum tersedia.
- Pagination server serta pencarian riwayat yang lebih besar tanpa mengubah kebenaran total laporan.

### Prioritas 3 — Kelengkapan transaksi dagang

- Uang muka, kredit lintas dokumen, dan refund tertunda dengan saldo pihak serta jejak penyelesaian yang dapat direkonsiliasi.
- Pembelian jasa, selisih harga invoice terhadap penerimaan, ongkir/diskon, dan perhitungan pajak sesuai kebijakan usaha yang dipilih.
- Penanganan barang pengganti serta retur yang lebih kompleks berdasarkan kebutuhan pilot, tanpa membuka ulang pesanan secara diam-diam.

### Prioritas 4 — Inventori dan pelaporan lanjutan

- Banyak gudang, transfer stok, konversi satuan, batch/serial/kedaluwarsa, dan reservasi bila diperlukan usaha sasaran.
- Perluasan cakupan kas/bank, rekonsiliasi bank, dan pembagian aktivitas arus kas per bagian jurnal.
- Pengelompokan laporan serta proses tahun buku sesuai kebutuhan pengguna, tanpa mengubah ledger historis yang telah diposting.

Integrasi, HR/payroll, produksi, POS, multi-company/multicurrency, SSO, dan plugin tetap menjadi perluasan jangka berikutnya yang memerlukan lingkup tersendiri. Tidak ada jadwal rilis atau SLA yang dijanjikan dalam rencana ini.

## 7. Skenario penerimaan yang harus tetap terjaga

Gunakan transaksi tanpa pajak dan periksa melalui dokumen, mutasi, serta jurnal:

1. Saldo awal 10 unit × Rp50.000, penjualan 2 unit × Rp80.000, pengiriman, invoice Rp160.000, dan pembayaran Rp60.000 + Rp100.000 harus menghasilkan stok 8 unit senilai Rp400.000, piutang nol, kas masuk Rp160.000, pendapatan Rp160.000, serta HPP Rp100.000.
2. Pesanan 10 unit yang baru dikirim 4 unit dan diinvoiskan 2 unit harus menyimpan sisa kirim 6 unit serta 2 unit terkirim yang masih siap ditagih. Posting berikutnya memakai pesanan yang sama dan tidak menagih jumlah yang sama dua kali.
3. PO 10 unit yang diterima 4 unit dan ditagih 2 unit harus mempertahankan sisa penerimaan 6 unit dan 2 unit diterima yang belum ditagih, dengan persediaan, kewajiban sementara, serta utang supplier yang sesuai.
4. Tagihan supplier Rp180.000 yang dibayar Rp60.000 lalu Rp120.000 harus berakhir tanpa sisa utang. Pembalikan yang diizinkan harus memulihkan saldo melalui jurnal lawan dan menyimpan riwayat pembayaran asal.
5. Tagihan Rp180.000 yang telah dibayar Rp120.000 lalu memperoleh kredit retur Rp80.000 harus menjadi nilai bersih Rp100.000, refund Rp20.000, pembayaran bersih Rp100.000, dan sisa nol. Arah kas mengikuti retur pelanggan atau supplier; pemenuhan pesanan asal tidak dibuka ulang.
6. Laba rugi harus mencakup seluruh akun pendapatan/beban, neraca harus seimbang dengan laba akumulasi, dan saldo awal + masuk − keluar Kas 1100 harus sama dengan saldo akhirnya. Akun tambahan, pembalikan, periode kosong, serta batas hari zona waktu tetap diperhitungkan.
7. Retry dengan kunci yang sama, versi lama, perebutan stok terakhir, jumlah berlebih, dan periode tertutup tidak boleh membuat transaksi ganda atau mutasi parsial.
8. Backup yang dipulihkan ke lingkungan terpisah harus mempertahankan dokumen, alokasi, retur, pembayaran, audit, stok, dan saldo ledger yang sesuai.

Skenario ini menjadi acuan regresi, bukan pernyataan bahwa seluruh skenario pilot, performa, dan kegagalan produksi telah diuji. Bukti pengujian aktual mengikuti [status implementasi](./IMPLEMENTATION_STATUS.md).

## 8. Kriteria rilis stabil

- Alur utama, hak akses, idempotensi, pembulatan, konkurensi, upgrade, dan backup/restore mempunyai hasil pengujian yang dapat diperiksa.
- Tidak ada bug diketahui yang menggandakan transaksi, merusak saldo, membocorkan akses, atau kehilangan data tanpa prosedur pemulihan.
- Instalasi bersih serta restart mempertahankan data; tidak ada kredensial default atau keharusan mengedit source untuk operasi biasa.
- Dokumentasi mencakup konfigurasi produksi/TLS, pengelolaan secrets, migrasi, pemulihan, serta batas fitur yang masih ada.
- Uji performa menggunakan dataset dan perangkat yang dicatat. Sasaran waktu respons ditetapkan dari pengukuran workload, bukan dianggap sebagai SLA sebelum ada bukti.
- Pengguna pilot dapat menyelesaikan alur bisnis utama berdasarkan panduan dan memahami laporan, koreksi, serta batas fitur.

Lima perluasan operasional kini sudah masuk implementasi Community. Fokus berikutnya adalah membuat pengguna dapat memindahkan data, menjalankan alur yang tersedia, dan mengoperasikannya secara konsisten sebelum memperluas domain lagi.
