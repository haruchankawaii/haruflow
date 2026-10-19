# Pembelian, utang, dan pembayaran supplier

Alur pembelian adalah **PO → penerimaan barang → tagihan supplier → pembayaran → lunas**. Penerimaan dan tagihan dapat dibuat sebagian, sehingga jumlah yang belum diterima atau belum ditagih tetap terlacak. Retur memakai dokumen dan kredit tersendiri.

PO mencatat jumlah serta biaya yang disepakati, bukan harga jual katalog. Membuat atau mengonfirmasi PO belum mengubah stok, utang, atau kas. Penerimaan menambah persediaan dan kewajiban barang diterima belum ditagih; tagihan memindahkan kewajiban itu ke utang supplier; pembayaran mengurangi utang dan Kas 1100.

## Hak akses dan supplier

Semua pengguna yang sudah masuk dapat membaca Pembelian, chart, riwayat, dan jurnal. Administrator dan Warehouse mengelola supplier, PO, penerimaan, serta koreksi penerimaan. Administrator dan Finance mencatat atau membalik tagihan, pembayaran, dan retur supplier. Role pembelian tersendiri belum tersedia.

Pada tab **Supplier**, isi kode unik, nama, dan kontak yang diperlukan. Kode dinormalisasi menjadi huruf besar. Supplier aktif diperlukan saat membuat atau mengubah draf PO dan saat konfirmasi. Penonaktifan tidak mengubah riwayat: PO yang sudah dikonfirmasi tetap dapat diterima, ditagih, dan dibayar sesudah supplier nonaktif.

## Membuat dan memenuhi PO

1. Buat PO dengan supplier aktif dan 1–100 produk barang yang berbeda. Pembelian jasa belum tersedia.
2. Isi jumlah dan biaya per unit. Perkiraan kedatangan serta catatan bersifat opsional; perkiraan ini bukan tanggal posting atau jatuh tempo.
3. Simpan sebagai draf dan tinjau rincian. Draf dapat diubah; nomor otomatis memakai urutan `PO-000001` dan seterusnya.
4. Konfirmasi kesepakatan. Isi PO kemudian dikunci. Kode/nama supplier, SKU/nama/satuan barang, jumlah, serta biaya tersimpan pada dokumen; perubahan nama katalog tidak mengubah riwayat.
5. Buka **Tinjau penerimaan**, isi jumlah yang benar-benar datang, dan isi 0 untuk melewati barang yang belum datang. Referensi surat jalan bersifat opsional. Terima seluruh sisa bila semuanya telah tiba.

Kolom **Diterima / sisa** memperlihatkan kemajuan tiap baris. Contoh PO 10 unit: menerima 4 unit membuat status **Diterima sebagian**, dengan sisa 6 unit pada PO yang sama. Sesudah semua baris lengkap, status menjadi **Diterima**. Setiap penerimaan memiliki dokumen `GR-000001` tersendiri dan tetap terlihat dalam riwayat.

PO yang belum mempunyai penerimaan aktif dapat dibatalkan dengan alasan 3–1.000 karakter. PO dibatalkan tidak dapat dibuka kembali. PO dengan penerimaan aktif tidak dapat langsung dibatalkan; penerimaannya harus memenuhi syarat pembalikan terlebih dahulu.

Versi dokumen mencegah perubahan pengguna lain tertimpa. Jika muncul konflik, muat ulang dan tinjau data terbaru. Kunci idempotensi pada penulisan mencegah request yang sama membuat transaksi tambahan. Nomor yang terlewati tidak dipakai ulang.

## Jumlah, biaya, dan pembulatan

Kuantitas mendukung tiga desimal, biaya unit empat, dan uang dua desimal. Jumlah dan biaya unit paling besar 1 miliar; nilai baris/dokumen paling besar Rp1 triliun. Total PO dihitung dari jumlah × biaya per baris, dibulatkan ke dua desimal dengan nilai tengah menjauhi nol, lalu dijumlahkan. Contoh `2,5 × Rp60.000,1234` menghasilkan **Rp150.000,31**.

Penerimaan sebagian memakai pembulatan kumulatif. Bagian terakhir mengambil sisa nilai baris agar seluruh penerimaan aktif tepat sama dengan nilai PO ketika jumlahnya lengkap. Misalnya 3 unit dengan biaya Rp3,3333 memiliki total Rp10,00; penerimaan 1 unit per dokumen bernilai Rp3,33, Rp3,34, lalu Rp3,33. Penagihan sebagian membagi nilai penerimaan tersimpan dengan prinsip yang sama.

Setiap baris yang diproses harus bernilai positif setelah pembulatan. Jika masih ada jumlah tersisa, sistem menyisakan minimal Rp0,01 untuk bagian terakhir. Jumlah yang terlalu kecil dapat ditolak; perbesar jumlah yang diproses atau isi 0 untuk melewati barang tersebut. Sistem menolak penerimaan atau penagihan melebihi sisa, produk/baris dari dokumen lain, serta baris ganda.

## Pembukuan penerimaan

Jumlah dipilih dari sisa PO; biaya dan satuan mengikuti snapshot PO. Biaya tidak diubah saat menerima barang. Jika satuan katalog sekarang berbeda, penerimaan ditolak agar tidak terjadi konversi diam-diam. Harga invoice yang berbeda, ongkir, pajak, dan biaya tambahan belum mempunyai alur alokasi tersendiri.

Penerimaan menyimpan dokumen GR, mutasi masuk, jumlah/nilai persediaan, biaya rata-rata baru, jurnal, perubahan kemajuan PO, dan audit secara atomik. Jurnalnya **debit Persediaan 1300 / kredit Barang diterima belum ditagih** sebesar nilai bagian yang diterima. Kas, pendapatan, dan HPP tidak berubah.

Akun penampung penerimaan merupakan akun kontrol sistem: kode bebas 2110–2199, atau kode sepuluh digit kosong bila rentang itu penuh. Akun pengguna tidak diambil alih; periksa kode aktual pada Daftar akun. Akun tersebut tidak dapat diisi melalui jurnal manual atau saldo awal. Penerimaan stok manual tetap mengkredit modal awal; gunakan alur PO untuk pembelian yang menimbulkan kewajiban supplier.

## Tagihan supplier sebagian atau penuh

Buka **Pembelian → Tagihan → Catat tagihan**, atau mulai dari penerimaan tertentu pada rincian PO:

1. Pilih penerimaan aktif yang masih mempunyai jumlah belum ditagih. PO boleh berstatus Diterima sebagian atau Diterima. Satu penerimaan dapat memiliki beberapa tagihan; setiap baris hanya boleh ditagih sampai jumlah yang diterima.
2. Isi jumlah yang ditagih pada setiap baris. Barang, satuan, biaya, dan nilai alokasi berasal dari snapshot penerimaan. Mutasi stok setelah penerimaan tidak menghalangi penagihan.
3. Isi nomor invoice supplier, tanggal invoice, jatuh tempo, dan catatan opsional. Nomor invoice berisi 1–100 karakter dan harus unik untuk supplier yang sama di antara tagihan yang belum dibatalkan, termasuk yang sudah lunas. Spasi tepi dihapus dan huruf besar/kecil tidak dibedakan. Nomor sama pada supplier berbeda diperbolehkan.
4. Gunakan tanggal kalender `YYYY-MM-DD`, tahun 0001–9999. Invoice tidak boleh melewati hari ini pada zona waktu perusahaan; jatuh tempo tidak boleh sebelum invoice. Invoice lampau boleh dicatat, tetapi jurnal tetap memakai waktu posting saat ini pada periode terbuka.
5. Tinjau dan posting. Nomor internal memakai urutan `PB-000001`, terpisah dari nomor invoice supplier. Riwayat penerimaan menautkan seluruh tagihan aktifnya.

Jurnal tagihan adalah **debit Barang diterima belum ditagih / kredit Utang supplier** sebesar bagian yang ditagih. Stok, biaya rata-rata, kas, pendapatan, dan HPP tidak berubah. Status pemenuhan PO juga tidak berubah karena penagihan.

Akun kontrol Utang supplier memakai kode kosong 2200–2299 atau kode sepuluh digit kosong; akun pengguna dan akun penampung penerimaan tidak digunakan ulang. Jurnal manual dan saldo awal tidak dapat mengisi akun ini. Pembukaan utang historis per supplier belum tersedia.

Contoh: PO 10 unit, diterima 4, lalu ditagih 2. Sisa penerimaan PO masih 6 unit; dari 4 unit yang sudah diterima masih ada 2 unit belum ditagih. Menerima atau menagih bagian selanjutnya tidak memerlukan PO baru.

## Pembayaran supplier

Pada detail tagihan, buka bagian **Pembayaran supplier**. Isi nominal positif sampai sisa utang dan referensi opsional, lalu catat pembayaran. Pembayaran dapat sebagian atau penuh, dengan dua desimal dan maksimal Rp1 triliun. Saldo Kas 1100 harus cukup. Aplikasi mencatat dana yang sudah dibayarkan; tidak melakukan transfer bank otomatis.

Posting menyimpan dokumen `SP-000001`, jurnal **debit Utang supplier / kredit Kas 1100**, pembaruan saldo/status tagihan, dan audit bersama-sama. Stok dan beban tidak berubah. Status tagihan adalah **Belum dibayar**, **Dibayar sebagian**, **Lunas**, atau **Dibatalkan**. Contoh tagihan Rp180.000 dibayar Rp60.000 menyisakan utang Rp120.000; membayar sisanya membuat tagihan lunas.

Pembayaran berlebih, saldo kas tidak cukup, versi lama, dan penggunaan ulang kunci untuk request berbeda ditolak. Riwayat pembayaran serta jurnal asal dapat dibuka dari tagihan.

Pembayaran yang salah dibalik dengan alasan 3–1.000 karakter pada periode terbuka. Pembalikan menambah jurnal lawan persis, mengembalikan kas, dan membuka kembali sisa utang yang sesuai. Pembayaran asal tidak dihapus. Retur aktif pada tagihan harus dibalik terlebih dahulu sebelum pembayaran asal dapat dibalik.

## Retur dan nilai tagihan bersih

Pilih **Retur ke supplier** pada detail tagihan untuk mencatat sebagian atau seluruh barang yang belum pernah diretur. Stok saat ini harus cukup dan satuan harus cocok. Kredit mengikuti nilai tagihan asal; stok keluar memakai biaya rata-rata saat retur. Selisih dicatat pada akun kontrol beban **Selisih nilai retur pembelian**, memakai kode kosong 5900–5999 atau kode sepuluh digit kosong tanpa mengambil alih akun pengguna.

**Nilai bersih = total awal − kredit retur aktif. Sisa utang = nilai bersih − pembayaran bersih.** Bila pembayaran yang sudah ada melebihi nilai bersih baru, retur mencatat kas masuk dari supplier sebesar kelebihan tersebut dan mengurangi pembayaran bersih. Konfirmasi dana aktual diperlukan sebelum posting refund. Refund tertunda atau saldo kredit lintas tagihan belum tersedia.

Contoh tagihan Rp180.000 sudah dibayar Rp120.000, kemudian diretur Rp80.000: nilai bersih menjadi Rp100.000, supplier mengembalikan Rp20.000, pembayaran bersih menjadi Rp100.000, dan sisa utang nol. Riwayat pembayaran Rp120.000 tetap tersimpan bersama riwayat refund. Tagihan dengan nilai bersih nol dapat berstatus Lunas walaupun penyelesaiannya berasal dari kredit retur.

Retur komersial tidak membuka kembali jumlah yang sudah diterima atau ditagih pada PO. Barang pengganti atau pembelian kembali dicatat melalui transaksi pengganti. Lihat [panduan retur](./RETURNS.md) untuk jurnal, persediaan, refund, dan pembalikan aman.

## Koreksi dan urutan ketergantungan

Tagihan yang sudah diposting tidak dapat diedit atau dihapus. Jika perlu membatalkannya, balikkan **retur aktif → pembayaran aktif → tagihan**, sesuai dokumen yang ada dan syarat masing-masing. Pembalikan tagihan membuat jurnal **debit Utang supplier / kredit Barang diterima belum ditagih**, mengubah status menjadi Dibatalkan, dan memulihkan jumlah penerimaan yang dapat ditagih. Dokumen PB pengganti dapat memakai nomor invoice supplier yang sudah dibatalkan.

Penerimaan dikoreksi dari riwayat PO setelah semua tagihan aktif yang menggunakannya dibalik. Setiap produk harus belum mempunyai mutasi setelah penerimaan, dan jumlah/nilai/biaya rata-rata harus cocok dengan snapshot. Mutasi yang kemudian telah dibalik tetap merupakan riwayat lanjutan. Jika satu barang gagal, seluruh pembalikan penerimaan ditolak.

Pembalikan GR yang memenuhi syarat memulihkan stok dan biaya sebelumnya secara persis serta menambah jurnal/mutasi lawan. Jumlah diterima dan sisa PO dihitung ulang; PO menjadi Dikonfirmasi bila tidak ada penerimaan aktif, atau Diterima sebagian bila masih ada bagian diterima. GR asal tetap tersimpan dan bagian yang dipulihkan dapat diterima kembali melalui GR baru.

Pembalikan retur barang juga memerlukan keadaan stok yang masih cocok dan tanpa mutasi berikutnya. Bila retur sebelumnya menerima refund, pembalikannya mengeluarkan kas kembali kepada supplier sehingga saldo kas harus cukup. Membalik dokumen turunan tidak menjamin mutasi stok lama dapat diputar ulang; gunakan alasan pemblokiran dan periksa riwayat sumbernya.

Penerimaan, tagihan, pembayaran, retur, dan pembalikannya memakai waktu posting saat ini serta pemeriksaan periode akuntansi perusahaan. Semua perubahan satu transaksi disimpan bersama atau ditolak bersama. Penyesuaian stok tidak otomatis menghapus kewajiban supplier.

## Membaca chart dan laporan

Chart PO menunjukkan jumlah seluruh PO menurut status Draf, Dikonfirmasi, Diterima sebagian, Diterima, dan Dibatalkan. Nominal **Menunggu penerimaan** menghitung nilai bagian PO dikonfirmasi yang belum diterima; **Barang diterima, belum ditagih** menghitung sisa nilai penerimaan aktif yang belum ditagih; **Utang supplier** menghitung sisa setelah kredit retur serta pembayaran. Memindahkan kewajiban dari penerimaan ke tagihan tidak mengubah jumlah barang yang diterima.

Chart jatuh tempo membagi sisa utang menjadi Belum jatuh tempo, Jatuh tempo hari ini, dan Lewat jatuh tempo. Bagian **Umur utang dan pembayaran** mengelompokkan sisa menjadi belum terlambat, 1–30, 31–60, 61–90, dan lebih dari 90 hari. Hari jatuh tempo belum dianggap terlambat. Perhitungan memakai hari perusahaan saat ini; pembayaran sebagian dan kredit retur mengurangi nominal yang ditampilkan. Tagihan lunas atau dibatalkan tidak menambah umur utang.

Riwayat kas supplier dapat dipilih untuk 3, 6, atau 12 bulan. Angka keluar dan kembali mencakup pembayaran, pembalikan pembayaran, refund retur, serta pembalikan refund menurut bulan posting; angka neto adalah kas keluar dikurangi kas kembali. Ini bukan umur utang historis pada tanggal lampau.

Daftar PO/tagihan, pencarian, filter, dan CSV daftar memakai paling banyak 500 dokumen terbaru. Ringkasan/chart dan analisis umur utang memakai seluruh dokumen. Rincian umur utang dapat difilter per kelompok dan diekspor CSV. Tautan tagihan, penerimaan, PO, dan jurnal membantu memeriksa sumber angka.

Buka **Akuntansi → Laporan keuangan** untuk laba rugi, neraca, dan arus kas beserta CSV dan rincian jurnal. Laba rugi serta arus kas memakai rentang tanggal posting; neraca memakai akumulasi sampai tanggal akhir. Arus kas mencakup akun Kas 1100. Lihat [panduan laporan keuangan](./FINANCIAL_REPORTS.md).

## Batas yang masih berlaku

Pembelian jasa, perubahan biaya invoice terhadap penerimaan, pajak, ongkir/biaya tambahan, uang muka supplier, refund tertunda, serta kredit lintas tagihan belum mempunyai alur khusus. Instalasi tetap satu perusahaan, satu gudang logis, dan IDR. Untuk pengiriman serta invoice penjualan sebagian, baca [pemenuhan penjualan](./SALES_FULFILLMENT.md).
