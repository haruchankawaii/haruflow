# Rencana Haruflow Community

Status: penyelesaian Community difokuskan pada usaha kecil dan penerimaan melalui simulasi. Hasil pengujian, identitas schema, dan paket yang benar-benar diverifikasi dicatat terpisah pada status implementasi.
Tanggal pembaruan: 23 Oktober 2026.
Dasar: arah produk dalam `haruflow.md` dan implementasi aktual repository. Rincian hasil pengujian serta batas fitur dicatat di [status implementasi](./IMPLEMENTATION_STATUS.md).

## 1. Arah produk

Haruflow Community adalah ERP gratis yang dijalankan sendiri oleh usaha kecil. Sasaran saat ini adalah satu perusahaan dengan mata uang IDR dan maksimal 10 akun aktif. Gudang utama dan Kas 1100 menjadi bawaan; beberapa gudang serta rekening dapat dipakai bila diperlukan. Barang/jasa tersedia pada penjualan dan pembelian.

Pengguna dapat menyiapkan perusahaan dan pengguna, memposting saldo awal, mengelola penjualan serta pembelian, mencatat pembayaran dan retur, lalu memeriksa stok, piutang/utang, dan laporan keuangan dari jurnal yang sama.

Community tidak memerlukan akun Haruflow, layanan lisensi, atau koneksi ke Haruflow Cloud. Instalasi resmi membatasi 10 akun aktif, termasuk administrator; akun nonaktif tetap menyimpan riwayat dan melepaskan slot. Lisensi tetap mengikuti `LICENSE` dalam repository. Lihat [batas pengguna dan perlindungan kode](./USER_LIMIT.md), termasuk ketentuan upgrade akun lama serta keterbatasan perlindungan instalasi self-hosted.

Pemilik instalasi mengelola server, akses jaringan/TLS, backup, secrets, dan upgrade. Source, Dockerfile/Compose, installer Windows per pengguna, panduan bergambar, identitas build, manifest backup dan kandidat ber-checksum tersedia. PostgreSQL resmi tetap diperlukan. Simulasi cukup untuk penerimaan pengerjaan Community ini; penggunaan usaha nyata bukan pekerjaan yang diwajibkan. Registry publik dan SLA belum disediakan.

## 2. Lingkup yang sudah diimplementasikan

### Fondasi

- Setup perusahaan/admin, autentikasi lokal, sesi yang dapat dicabut, pengelolaan pengguna, role Administrator/Sales/Warehouse/Finance/Viewer, dan pemulihan admin melalui perintah lokal.
- Profil perusahaan, kontak, serta awalan nomor dokumen; zona waktu dapat diubah sebelum jurnal pertama. Mata uang tetap IDR.
- Pelanggan, supplier, produk barang/jasa, satuan dasar, bagan akun dasar serta akun tambahan, dan audit aktivitas penting.
- Saldo awal stok, akun umum yang diperbolehkan, piutang/utang per dokumen, serta uang muka/kredit lama per pelanggan/supplier melalui draf seimbang dan posting atomik sebelum transaksi operasional. Rincian pihak mendukung input manual dan CSV dengan preview.
- Persediaan dengan biaya rata-rata bergerak, penerimaan stok manual, adjustment, riwayat mutasi, dan koreksi berbasis snapshot ketika belum ada mutasi lanjutan.
- Jurnal berpasangan otomatis/manual, buku besar, neraca saldo, penutupan periode, pencarian, CSV, dan cetak invoice melalui browser. Pusat **Riwayat lengkap** menambahkan pagination/pencarian server serta ekspor seluruh filter dengan batas yang jelas.
- Satu aplikasi Rust melayani API dan frontend; Compose aplikasi/PostgreSQL, migrasi eksplisit, readiness yang memeriksa schema, identitas versi/commit, CI, serta utilitas backup/restore dengan manifest dan SHA-256 tersedia.

### Lima perluasan Community yang sudah selesai diimplementasikan

1. **Pembayaran supplier sebagian/penuh.** Pembayaran dari tagihan mengurangi rekening kas/bank pilihan dan utang supplier, menolak kelebihan pembayaran, menyimpan idempotensi/audit/jurnal, serta menyediakan pembalikan ke rekening sumber dengan pemeriksaan dependensi.
2. **Umur utang dan chart pembayaran.** Umur utang memakai kelompok belum lewat jatuh tempo (termasuk hari ini), terlambat 1–30/31–60/61–90/lebih dari 90 hari, rincian supplier, dan pergerakan pembayaran. Ringkasan jatuh tempo juga membedakan tagihan hari ini dari yang belum jatuh tempo. Ringkasan memakai seluruh dokumen yang relevan, bukan hanya daftar dokumen terbaru.
3. **Retur pelanggan dan supplier sebagian/penuh.** Retur berasal dari invoice/tagihan, mengurangi nilai tagihan, serta memperbarui persediaan bila relevan. Bila terjadi kelebihan pembayaran, pilih refund aktual, simpan kredit pihak, atau tunggu refund. Riwayat dan jurnal pembalik tetap tersimpan. [Panduan retur](./RETURNS.md) menjelaskan biaya, pembulatan, refund aktual, dan batas koreksi.
4. **Pemenuhan serta penagihan bertahap.** Sales order dapat dikirim dan diinvoiskan sebagian/penuh dengan sisa per baris tetap terlacak. PO dapat diterima dan ditagih sebagian/penuh; satu penerimaan dapat memiliki beberapa tagihan untuk bagian jumlah yang berbeda. Alokasi dan pembulatan mencegah jumlah atau nilai ditagih dua kali. Lihat [pemenuhan penjualan](./SALES_FULFILLMENT.md) dan [pembelian](./PURCHASING.md).
5. **Laporan keuangan.** Laba rugi periode, neraca kumulatif dan arus kas seluruh rekening terdaftar tersedia dengan tanggal perusahaan, chart, CSV dan rincian akun/jurnal. Transfer internal dikecualikan dari arus eksternal; jurnal tutup tahun dikecualikan dari aktivitas laba rugi. Klasifikasi manual beralasan memiliki versi dan riwayat.

Alur operasional sekarang mencakup penjualan → pengiriman → invoice → pembayaran, serta supplier → PO → penerimaan → tagihan → pembayaran. Retur merupakan transaksi komersial tersendiri; retur tidak membuka ulang jumlah pesanan yang sudah dipenuhi atau ditagih.

### Saldo pihak dan persiapan operasional

- **Saldo pihak** menampilkan buku pembantu, posisi per tanggal, aging termasuk tagihan pembukaan, rekonsiliasi dengan akun kontrol, CSV, dan cetak. Jatuh tempo invoice baru mengikuti jangka pembayaran pelanggan; tanggal lama yang belum diketahui tetap ditandai sampai dilengkapi.
- Dana aktual pelanggan/supplier dicatat sebagai uang muka. Dana dapat dialokasikan sebagian/penuh ke beberapa tagihan pihak yang sama, termasuk tagihan pembukaan, tanpa mencatat kas kembali. Kelebihan dana tetap menjadi saldo yang dapat ditelusuri.
- Kredit retur dan kredit lama dapat dipakai lintas tagihan atau direfund. Permintaan refund menyisihkan dana, pembayaran refund dapat sebagian, dan pembatalan melepaskan sisa yang belum dibayar. Koreksi mengikuti dependensi dan mempertahankan riwayat.
- Lampiran mencakup sumber dana, refund aktual, dan rincian pembukaan selain dokumen sebelumnya. Alat pengukuran memakai database sementara; hasil pengukuran dan hasil pilot dilaporkan terpisah.
- Kandidat rilis dapat disiapkan dari source bersih dan commit yang sama dengan binary. Paket menyertakan source, web, lisensi, identitas build, dan checksum; workflow memeriksa CI pada commit yang dipaketkan. Lihat [saldo pihak](./PARTY_BALANCES.md), [riwayat](./HISTORY.md), dan [kandidat rilis](./RELEASE_CANDIDATE.md).

## 3. Batas produk saat ini

Implementasi di atas belum berarti seluruh rencana ERP atau persiapan rilis stabil selesai. Batas berikut masih berlaku:

- Satu perusahaan, IDR dan 10 akun aktif. Beberapa gudang/transfer tersedia; multicurrency dan konsolidasi perusahaan belum tersedia.
- Pembelian mendukung barang/jasa, harga invoice berbeda, ongkir/diskon dan pajak eksplisit. Penyelesaian **Biaya perolehan** memindahkan biaya pembelian yang memenuhi syarat ke persediaan/HPP melalui preview dan jurnal terpisah; hasil validasinya mengikuti status implementasi. Pelaporan pajak otomatis belum tersedia.
- Pembayaran langsung invoice/tagihan dibatasi sisa dokumen. Untuk dana berlebih gunakan pencatatan uang muka lalu alokasikan bagian pelunasan secara eksplisit; sisa menjadi saldo pihak. Transfer antar pihak, netting pelanggan/supplier, dan transfer bank otomatis belum tersedia.
- Impor CSV mencakup master, draf saldo awal dan transaksi finansial historis. Impor historis tidak membuat ulang mutasi/pesanan lama. Pembukaan tetap hanya sebelum operasi; hindari saldo ganda dengan dokumen yang telah diimpor.
- Lot/seri, kedaluwarsa, reservasi dan konversi eksak tersedia pada alur inventori lanjutan. PO/SO dan form lama tetap memakai satuan dasar.
- Arus kas mencakup rekening terdaftar; akun aset lain tidak otomatis dianggap kas. Klasifikasi berlaku per jurnal; pembagian satu jurnal ke beberapa aktivitas belum tersedia. Laporan tidak menyatakan kepatuhan terhadap format perpajakan tertentu.
- Belum ada HR/payroll, produksi, POS offline, aplikasi mobile native, integrasi bank/pajak, SSO, marketplace plugin, email, atau jobs/outbox. Lampiran berada di PostgreSQL; storage eksternal dan retensi otomatis belum tersedia.
- Daftar ringkas operasional tetap membatasi jumlah dokumen terbaru. Gunakan **Riwayat lengkap** untuk pencarian dan pagination server di luar batas tersebut; ekspor maksimal 10.000 hasil filter dan menolak hasil lebih besar tanpa memotongnya. Rincian buku besar tetap memberi penanda bila terpotong; saldo dan ringkasan memakai seluruh data yang sesuai.

HR/payroll, produksi, POS khusus, multi-company, multicurrency, serta AI lanjutan seperti chat tertanam atau MCP jarak jauh adalah **rencana Enterprise**, sesuai keputusan produk. Fitur tersebut bukan kekurangan yang harus ditambahkan agar Community selesai. MCP lokal baca tetap bagian Community. Rencana Enterprise belum berarti modul-modul itu sudah tersedia; lisensi Community tidak berubah.

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
- Pembayaran langsung dibatasi sisa tagihan bersih setelah retur. Uang muka/kredit dialokasikan ke sisa tersebut tanpa kas baru. Kelebihan setelah retur diselesaikan dengan refund aktual, kredit tersimpan, atau refund tertunda; hanya perpindahan uang aktual yang mengubah kas.
- Setiap jurnal posted seimbang. Posting dan pembalikan memakai tanggal saat ini menurut zona waktu perusahaan; periode tertutup menolak posting baru. Tanggal invoice supplier tidak memundurkan jurnal.
- Dokumen posted tidak diedit atau dihapus. Koreksi memakai reversal beralasan yang merujuk dokumen asal. Retur aktif harus dibalik sebelum pembalikan pembayaran/invoice/tagihan; dependensi alokasi serta mutasi stok lanjutan tetap diperiksa.
- Pembalikan stok harus cocok dengan snapshot dan belum mempunyai mutasi berikutnya. Membalik transaksi lanjutan tidak menghapus riwayat mutasinya.
- Saldo awal dapat dikoreksi melalui reversal sebelum aktivitas operasional dan pemakaian rinciannya. Persediaan serta saldo pihak awal berasal dari rincian yang cocok dengan akun kontrol; jurnal manual dan saldo akun umum tidak dapat melewati rincian sumber tersebut.
- Laporan memakai ledger dengan tanggal serta zona waktu jelas. Laba rugi memakai aktivitas periode, neraca memakai akumulasi sampai tanggal akhir, dan arus kas mencakup rekening terdaftar dengan transfer internal dikecualikan.

## 6. Penyelesaian Community usaha kecil

Enam prioritas utama telah berkembang menjadi alur pembelian, kas/bank, stok lokasi, periode/tahun buku, impor historis dan operasi Windows. Penerimaan pekerjaan ini memakai data demo serta database sementara. Usaha nyata, payroll, produksi, POS khusus, multi-company, multicurrency dan chat AI tertanam tidak termasuk syarat penyelesaian Community.

Paket penyelesaian meliputi:

- **Biaya perolehan:** preview alokasi biaya final yang sudah dibukukan, bagian persediaan/HPP, pembulatan eksak, sumber tidak dipakai ganda, pembalikan beralasan dan pemeriksaan dependensi.
- **Database:** migrasi eksplisit tanpa mengubah migrasi lama, indeks pada pencarian anak, serta diagnostik snapshot baca untuk mendeteksi ketidakcocokan schema, guard, stok, jurnal dan akun kontrol. Diagnosis tidak memperbaiki angka otomatis.
- **Windows:** wizard per pengguna, database privat dari binaries PostgreSQL resmi atau koneksi tersedia, akun aplikasi terbatas, startup opsional, pilihan jam backup dan retensi arsip yang dimiliki instalasi.
- **Dokumentasi:** README bertema, panduan bergambar aplikasi asli, panduan database, backup/pemulihan dan penjelasan akses Codex.
- **Validasi:** regresi transaksi/konkurensi/periode, instalasi serta upgrade, eksekusi task backup, pemulihan ke lingkungan terpisah, checksum paket, CI dan percakapan Codex simulasi.

Status dan hasil aktual mengikuti [status implementasi](IMPLEMENTATION_STATUS.md) dan [identitas paket](RELEASE_CANDIDATE.md). Daftar ini bukan pernyataan bahwa semua validasi sudah lulus sebelum buktinya dicatat. [Skenario latihan](SMALL_BUSINESS_PILOT.md) memisahkan hasil simulasi dari pemakaian usaha nyata.

Pengembangan setelah paket ini mengikuti kebutuhan Community usaha kecil. Integrasi bank/pajak, SSO, storage eksternal, outbox dan pemecahan aktivitas arus kas masih memerlukan lingkup tersendiri. Modul besar yang telah diputuskan untuk Enterprise tetap berada pada roadmap Enterprise; tidak ada jadwal atau SLA yang dijanjikan.

## 7. Skenario penerimaan yang harus tetap terjaga

Gunakan transaksi tanpa pajak dan periksa melalui dokumen, mutasi, serta jurnal:

1. Saldo awal 10 unit × Rp50.000, penjualan 2 unit × Rp80.000, pengiriman, invoice Rp160.000, dan pembayaran Rp60.000 + Rp100.000 harus menghasilkan stok 8 unit senilai Rp400.000, piutang nol, kas masuk Rp160.000, pendapatan Rp160.000, serta HPP Rp100.000.
2. Pesanan 10 unit yang baru dikirim 4 unit dan diinvoiskan 2 unit harus menyimpan sisa kirim 6 unit serta 2 unit terkirim yang masih siap ditagih. Posting berikutnya memakai pesanan yang sama dan tidak menagih jumlah yang sama dua kali.
3. PO 10 unit yang diterima 4 unit dan ditagih 2 unit harus mempertahankan sisa penerimaan 6 unit dan 2 unit diterima yang belum ditagih, dengan persediaan, kewajiban sementara, serta utang supplier yang sesuai.
4. Tagihan supplier Rp180.000 yang dibayar Rp60.000 lalu Rp120.000 harus berakhir tanpa sisa utang. Pembalikan yang diizinkan harus memulihkan saldo melalui jurnal lawan dan menyimpan riwayat pembayaran asal.
5. Tagihan Rp180.000 yang telah dibayar Rp120.000 lalu memperoleh kredit retur Rp80.000 harus menjadi nilai bersih Rp100.000, refund Rp20.000, pembayaran bersih Rp100.000, dan sisa nol. Arah kas mengikuti retur pelanggan atau supplier; pemenuhan pesanan asal tidak dibuka ulang.
6. Laba rugi harus mencakup aktivitas akun pendapatan/beban, neraca harus seimbang termasuk laba ditahan/sisa laba, dan saldo awal + masuk − keluar rekening terdaftar harus sama dengan saldo akhirnya. Transfer internal dan tutup tahun tidak menggandakan arus kas atau menghapus aktivitas laba rugi.
7. Retry dengan kunci yang sama, versi lama, perebutan stok terakhir, jumlah berlebih, dan periode tertutup tidak boleh membuat transaksi ganda atau mutasi parsial.
8. Backup yang dipulihkan ke lingkungan terpisah harus mempertahankan dokumen, alokasi, retur, pembayaran, audit, stok, dan saldo ledger yang sesuai.

Skenario ini menjadi acuan regresi, bukan pernyataan bahwa seluruh skenario pilot, performa, dan kegagalan produksi telah diuji. Bukti pengujian aktual mengikuti [status implementasi](./IMPLEMENTATION_STATUS.md).

## 8. Kriteria rilis stabil

- Alur utama, hak akses, idempotensi, pembulatan, konkurensi, upgrade, dan backup/restore mempunyai hasil pengujian yang dapat diperiksa.
- Tidak ada bug diketahui yang menggandakan transaksi, merusak saldo, membocorkan akses, atau kehilangan data tanpa prosedur pemulihan.
- Instalasi bersih serta restart mempertahankan data; tidak ada kredensial default atau keharusan mengedit source untuk operasi biasa.
- Dokumentasi mencakup konfigurasi produksi/TLS, pengelolaan secrets, migrasi, pemulihan, serta batas fitur yang masih ada.
- Uji performa menggunakan dataset dan perangkat yang dicatat. Sasaran waktu respons ditetapkan dari pengukuran workload, bukan dianggap sebagai SLA sebelum ada bukti.
- Simulasi dapat menyelesaikan alur bisnis utama berdasarkan panduan serta memeriksa laporan, koreksi dan batas fitur. Bukti usaha nyata tidak diwajibkan untuk lingkup pengerjaan ini.

Fokus penyelesaian adalah validasi paket Community, panduan yang dapat diikuti, dan latihan pemulihan. Penyempurnaan sesudahnya didasarkan pada kebutuhan usaha kecil dan hasil simulasi yang dapat direproduksi.
