# Community untuk usaha kecil

Haruflow Community ditujukan untuk satu usaha kecil: satu perusahaan, mata uang IDR, dan maksimal 10 akun aktif termasuk administrator. Mulai dengan gudang utama dan Kas 1100; rekening tambahan, lot/seri, reservasi, dan tutup tahun digunakan saat dibutuhkan. Volume transaksi bukan batas lisensi baru; kapasitas harus diukur pada komputer yang dipakai.

[Panduan bergambar](GUIDE.md) menjelaskan urutan mulai, transaksi, laporan, AI dan backup. Penerimaan untuk pekerjaan pengembangan saat ini memakai simulasi yang hasilnya dicatat; pilot usaha nyata tidak disyaratkan untuk menyelesaikan lingkup Community ini.

## Enam prioritas operasional

1. **Pembelian barang dan jasa.** PO dapat mencampur barang/jasa. Penerimaan jasa adalah pengakuan jasa diterima dan tidak menciptakan stok. Tagihan dapat memakai harga invoice berbeda dari penerimaan, ongkir, diskon, serta pajak eksplisit inklusif/eksklusif. Nilai penerimaan tetap menjadi dasar stok; selisih harga, ongkir, diskon, dan pajak memiliki akun terpisah. Ini belum merupakan landed-cost yang membagi seluruh biaya ke persediaan. Tarif contoh bukan tarif pajak yang disarankan. Tinjau nama, dasar, dan tarif sendiri sebelum posting. Retur dan pembalikan mempertahankan snapshot serta pembulatan sen terakhir.
2. **Kas dan bank.** Menu Kas & bank menyediakan rekening, transfer internal, CSV mutasi bank, serta pencocokan/pembatalan pencocokan dengan jurnal. Pembayaran, DP, dan refund memilih rekening terdaftar. Pembalikan kembali ke rekening sumber. Transfer tidak menambah arus kas eksternal; impor mutasi bank tidak memposting kas kedua kali. Lihat [Treasury](TREASURY.md).
3. **Stok per lokasi.** Inventori → Gudang, lot & reservasi mengelola gudang, posisi stok, transfer, konversi unit rasional, lot/nomor seri/kedaluwarsa, serta reservasi pesanan. Gudang utama menampung stok lama tanpa mengarang lot historis. Nilai semua lokasi harus sama dengan nilai produk; biaya rata-rata tetap digabung per produk. Transfer tidak membuat laba/rugi. Lihat [persediaan lanjutan](INVENTORY_LOCATIONS.md).
4. **Periode dan tahun buku.** Akuntansi → Periode & tahun buku menyediakan jurnal bertanggal pada periode terbuka, preview dan tutup tahun berakhir, pembalikan tutup tahun terbaru, serta buka ulang periode oleh admin dengan alasan/konfirmasi. Jurnal penutupan memindahkan saldo pendapatan/beban ke laba ditahan; laporan laba rugi tetap menunjukkan aktivitas aslinya. Tanggal finansial dan waktu pencatatan dibedakan. Lihat [tahun buku](FISCAL_PERIODS.md).
5. **Impor transaksi historis.** Impor transaksi menyediakan CSV invoice pelanggan, tagihan supplier, pembayaran keduanya, serta jurnal. Preview dan review wajib; batch disimpan atomik setelah validasi ulang. Dokumen historis ikut buku pembantu, aging, alokasi kredit, dan rekonsiliasi akun kontrol. Ini migrasi keuangan, tidak merekayasa pengiriman/penerimaan stok lama. Lihat [impor historis](TRANSACTION_IMPORTS.md).
6. **Operasi Windows.** Paket Windows menyertakan installer per pengguna, start/stop, checksum, versi terpisah, konfigurasi pribadi, helper backup native dan backup sebelum migrasi. PostgreSQL harus tersedia. Pengujian simulasi mencakup instalasi, gangguan proses/retry serta restore. Pemilik memilih lokasi/jadwal backup ketika menerapkan aplikasi; panduan tidak menganggap pilihan perangkat telah diberikan. Lihat [Windows](WINDOWS.md), [backup](WINDOWS_BACKUP.md) dan [latihan](SMALL_BUSINESS_PILOT.md).

## Insight AI di Community

Community menyertakan MCP lokal baca untuk Codex: ringkasan usaha, kas, piutang/utang, penjualan, stok dan bukti transaksi. Admin memberi izin kepada akun yang sudah ada; pengguna membuat kunci pribadi terbatas lingkup dan waktu, dapat dicabut. Tidak ada akun ERP ke-11 untuk agen. Akses diperiksa server pada setiap permintaan dan tidak menyediakan operasi tulis. Percakapan ada di Codex; data dapat diproses penyedia AI yang dipakai pengguna. [Panduan MCP](MCP.md).

Pengujian dengan model Codex sebenarnya serta koneksi demo pribadi sudah dijalankan, terpisah dari tes data/transport. Bukti dan batasnya dicatat pada [status implementasi](IMPLEMENTATION_STATUS.md), bukan dianggap sebagai persetujuan berbagi semua data usaha.

## Batas paket

**Rencana Enterprise:** HR/payroll, produksi, POS khusus, multi-company, multicurrency dan AI lanjutan berupa chat tertanam atau MCP jarak jauh. Ini pembagian rencana produk; fitur Enterprise tersebut belum dinyatakan telah diimplementasikan dan tidak menjadi kekurangan yang wajib ditambahkan agar Community selesai. MCP lokal baca tetap tersedia di Community.

Koneksi bank otomatis, e-faktur/pelaporan pajak otomatis, aplikasi mobile native dan layanan cloud terkelola tidak tersedia dalam alur Community ini. Rekonsiliasi bank memakai impor CSV dan pencocokan; penghitungan pajak eksplisit tidak sama dengan pelaporan pajak. Pembayaran mencatat perpindahan uang yang sudah terjadi, bukan mengirim uang otomatis.

Batas 10 akun ditegakkan server/database; administrator yang menguasai source dan database tetap dapat memodifikasinya sesuai lisensi AGPL. Tidak ada klaim semua kebutuhan setiap usaha selesai hanya karena suatu modul tersedia. Hasil simulasi, kapasitas perangkat, pilihan deployment dan persetujuan penggunaan data nyata tetap dijelaskan secara terpisah.
