# Retur pelanggan dan retur ke supplier

Retur mencatat barang atau jasa yang dikembalikan, kredit atas tagihan asal, dan pengembalian uang bila diperlukan. Dokumen asal tetap disimpan. Retur dapat mencakup seluruh jumlah atau sebagian jumlah yang belum pernah diretur.

Semua pengguna yang sudah masuk dapat membaca riwayat dan jurnal retur. **Administrator dan Finance** dapat mencatat serta membalik retur. Alur ini memakai invoice penjualan atau tagihan supplier yang telah diposting; barang yang baru diterima tetapi belum ditagih belum mempunyai alur retur komersial tersendiri dari penerimaannya.

## Mencatat retur

Untuk retur pelanggan, buka **Penjualan → Invoice**, lalu rincian invoice dan bagian **Retur pelanggan**. Untuk retur ke supplier, buka **Pembelian → Tagihan**, lalu rincian tagihan dan bagian **Retur ke supplier**.

1. Pilih **Catat retur** pada dokumen aktif yang mempunyai jumlah tersisa untuk diretur.
2. Isi jumlah pada satu atau beberapa baris barang/jasa. Baris yang tidak diisi tidak diretur. Jumlah tidak boleh melebihi sisa pada baris sumber.
3. Periksa kredit retur, sisa tagihan setelah retur, dan pengembalian uang. Kredit mengikuti nilai pada invoice/tagihan asal, bukan harga katalog saat ini.
4. Jika timbul kelebihan pembayaran, pilih **Dana sudah dikembalikan**, **Simpan kredit pihak**, atau **Menunggu refund**. Untuk refund langsung, pastikan perpindahan uang benar-benar sudah terjadi dan centang konfirmasinya. Dua pilihan lainnya belum mencatat kas; refund tertunda menyisihkan kredit agar tidak terpakai pada tagihan lain. Aplikasi mencatat kas; aplikasi tidak melakukan transfer bank atau menyerahkan uang secara otomatis.
5. Isi alasan 3–1.000 karakter, pilih **Tinjau retur**, periksa jumlah serta nominal, lalu **Posting retur**. Gunakan **Ubah rincian** jika masih ada yang salah.

Setiap retur dapat memuat 1–100 baris yang berbeda. Jumlah memakai ketelitian sampai tiga desimal dan maksimal 1.000.000.000 per baris. Baris sumber dari dokumen lain tidak dapat digunakan. Versi dokumen diperiksa saat posting; jika dokumen berubah, muat ulang dan tinjau kembali sebelum mencoba lagi.

Pencatatan berhasil memperbarui stok yang terkait, kredit, saldo tagihan, kas bila ada refund, jurnal, dan riwayat bersama-sama. Permintaan ulang dengan identitas yang sama mengembalikan hasil sebelumnya. Jika salah satu pemeriksaan gagal, tidak ada sebagian barang atau jurnal yang diposting sendiri.

## Kredit dan pengembalian uang

**Nilai tagihan bersih = total awal − kredit retur aktif.** Total penyelesaian tagihan memperhitungkan refund langsung dan kelebihan yang dipindahkan menjadi kredit pihak. Rincian memisahkan kas aktual, pemakaian saldo, kredit yang dipindahkan, dan sisa tagihan. Riwayat pembayaran asal dan riwayat retur tetap dapat diperiksa masing-masing.

Contoh tagihan awal Rp180.000 dengan kredit retur Rp80.000:

- Belum dibayar: tagihan bersih menjadi Rp100.000; sisa tagihan Rp100.000; tidak ada pengembalian uang.
- Sudah dibayar Rp120.000: tagihan bersih Rp100.000; kelebihan Rp20.000 dapat dikembalikan langsung, disimpan sebagai kredit, atau disisihkan untuk refund. Total penyelesaian pada tagihan menjadi Rp100.000; sisa tagihan nol.
- Sudah dibayar penuh Rp180.000: kelebihan Rp80.000 memakai pilihan penyelesaian yang sama. Tagihan bersih dan total penyelesaian masing-masing Rp100.000; sisa tagihan nol.

Pada retur pelanggan, pengembalian uang berarti **kas keluar kepada pelanggan**. Pada retur supplier, pengembalian uang berarti **kas masuk dari supplier**. Bila kredit masih tertutup oleh sisa tagihan yang belum dibayar, kredit lebih dahulu mengurangi piutang atau utang tanpa mencatat kas.

**Refund langsung** mencatat kas bersama retur dan memerlukan konfirmasi dana aktual. **Simpan kredit pihak** membuat sumber kredit retur pada **Saldo pihak**, yang dapat dialokasikan sebagian/penuh ke tagihan lain milik pihak yang sama atau direfund kemudian. **Refund tertunda** membuat sumber kredit sekaligus permintaan refund atas kelebihannya; kas belum berubah dan saldo yang disisihkan tidak dapat dialokasikan ke tagihan lain.

Permintaan refund dapat dibayar sebagian/penuh setelah uang benar-benar berpindah atau dibatalkan untuk melepaskan sisa sisihannya. Pembatalan tidak menghapus pembayaran refund yang sudah terjadi. Pengembalian dana pelanggan memerlukan saldo kas yang cukup. Lihat [saldo pihak, alokasi, dan refund](./PARTY_BALANCES.md) untuk riwayat serta koreksinya.

Upgrade mempertahankan refund langsung pada retur lama. Sistem tidak membuat kredit baru untuk uang yang sudah selesai dikembalikan dan tidak memposting ulang jurnal/kas historis.

Tagihan yang seluruh nilai bersihnya telah dikreditkan dapat berstatus **Lunas** dengan nilai bersih nol. Status tersebut menunjukkan tidak ada sisa tagihan; periksa kredit dan riwayat dana untuk mengetahui penyebabnya.

## Jumlah dan pembulatan kredit

Kredit dihitung secara proporsional dari jumlah dan nilai baris asal, dengan pembulatan dua desimal. Perhitungan memakai jumlah retur kumulatif dan kredit yang sudah diberikan agar beberapa retur sebagian tidak menambah atau menghilangkan sen.

Contoh: baris 3 unit bernilai Rp1,00, diretur 1 unit per dokumen. Pembagian kreditnya menjadi Rp0,33, Rp0,34, lalu Rp0,33; seluruhnya tetap Rp1,00.

Sebelum jumlah terakhir diretur, sistem menyisakan minimal Rp0,01 untuk sisa baris. Kredit pada setiap baris retur harus positif setelah pembulatan. Retur dengan jumlah sangat kecil dapat ditolak bila kreditnya belum mencapai Rp0,01 atau akan menghabiskan seluruh kredit sementara masih menyisakan jumlah. Gabungkan jumlah retur atau gunakan seluruh sisa yang sesuai. Retur terakhir mengambil sisa kredit baris secara tepat.

## Dampak persediaan dan akuntansi

**Retur pelanggan atas barang** menambah persediaan memakai biaya historis pengiriman yang terkait dengan jumlah invoice tersebut. Pendapatan berkurang sebesar kredit, dan HPP berkurang sesuai biaya historis barang yang kembali. Nilai tersebut tidak dihitung dari harga jual atau biaya rata-rata katalog terbaru. Jumlah dan nilai stok setelah penerimaan retur tetap harus memenuhi batas persediaan.

**Retur pelanggan atas jasa** mengurangi pendapatan dan tagihan tanpa membuat mutasi persediaan. Kredit mengikuti baris jasa yang sudah diinvoiskan.

**Retur ke supplier** mengurangi stok memakai biaya rata-rata persediaan saat retur diposting. Kredit tagihannya tetap mengikuti nilai pembelian asal. Jika nilai stok yang keluar berbeda dari kredit, selisih dicatat pada akun sistem **Selisih nilai retur pembelian** dan ikut dalam laba rugi. Selisih tersebut dapat menambah atau mengurangi beban. Tagihan supplier dalam alur pembelian saat ini bersumber dari barang stok, sehingga retur pembelian jasa belum tersedia.

Stok saat ini harus cukup untuk barang yang dikembalikan ke supplier. Sistem juga menolak bila satuan barang sekarang tidak cocok dengan satuan sumber, agar jumlah tidak diam-diam dianggap sebagai konversi satuan. Pada retur pelanggan atas barang, satuan harus cocok dengan pengiriman asalnya.

Semua posting memakai waktu saat pencatatan dan pemeriksaan periode akuntansi perusahaan. Retur atas invoice lama dapat diposting pada periode saat ini yang masih terbuka; tanggal invoice lama tidak memundurkan tanggal jurnal retur.

## Pesanan asal tetap menyimpan pemenuhannya

Retur tidak membuka kembali sisa pengiriman, penerimaan PO, atau jumlah yang telah ditagih pada pesanan asal. Dokumen asal tetap memperlihatkan jumlah yang sudah dipenuhi dan ditagih; kredit serta jumlah retur mempunyai riwayat sendiri.

Jika perlu barang pengganti atau pembelian kembali, catat transaksi penggantinya sesuai kesepakatan bisnis. Jangan menganggap jumlah retur otomatis menjadi jumlah yang dapat dikirim, diterima, atau ditagih ulang pada pesanan lama.

## Membetulkan retur yang salah

Pilih **Koreksi retur** pada riwayat yang masih memenuhi syarat, isi alasan, tinjau, lalu pilih **Balikkan retur**. Pembalikan menambah jurnal lawan dan, untuk barang stok, mutasi lawan. Retur asal, alasan awal, alasan koreksi, serta tautan jurnal asal/pembalik tetap tersimpan; retur yang sama tidak dapat dibalik dua kali.

Bila retur sebelumnya mencatat refund langsung, pembalikan juga mencatat arus uang yang berlawanan. Konfirmasikan perpindahan dana tersebut: retur pelanggan dibalik dengan menerima uang kembali dari pelanggan, sedangkan retur supplier dibalik dengan mengembalikan dana refund kepada supplier. Pembalikan refund supplier memerlukan kas yang cukup.

Bila retur menghasilkan kredit tersimpan/refund tertunda, selesaikan dependensinya terlebih dahulu: balikkan alokasi dan refund aktual dari kredit tersebut, kemudian batalkan sisa permintaan refund. Setelah tidak ada pemakaian atau sisihan aktif, pembalikan retur juga membalik sumber kreditnya. Pembalikan sumber tidak mencatat ulang kas refund yang telah ditangani melalui dokumen refund tersendiri.

Pembalikan barang stok hanya dapat dilakukan ketika setiap produk belum mempunyai mutasi setelah retur, dan posisi stok masih cocok dengan snapshot retur. Mutasi yang kemudian telah dibalik tetap merupakan riwayat berikutnya. Untuk retur jasa, tidak ada pemeriksaan mutasi stok, tetapi persyaratan dokumen, saldo, dan periode tetap berlaku. Periode saat pembalikan diposting harus terbuka.

Alasan pemblokiran ditampilkan pada riwayat. Bila satu baris tidak memenuhi syarat, seluruh pembalikan ditolak. Setelah pembalikan berhasil, jumlah dan kredit tersebut dapat diretur kembali melalui dokumen baru bila masih memenuhi syarat.

## Urutan koreksi dokumen yang saling terkait

Retur aktif mencegah pembatalan invoice/tagihan dan pembalikan pembayaran asal. Balikkan retur aktif terlebih dahulu, kemudian pembayaran bila perlu, lalu invoice/tagihan. Penerimaan PO yang memiliki tagihan aktif harus menunggu pembalikan tagihan; pengiriman yang masih mempunyai alokasi invoice juga mengikuti dokumen tagihannya.

Pembalikan tidak menghapus mutasi yang sudah terjadi. Karena itu, membalik retur dan tagihan tidak menjamin penerimaan atau pengiriman lama dapat diputar ulang: adanya mutasi lanjutan tetap dapat memblokir pembalikan stok. Gunakan alasan yang ditampilkan dan periksa jurnal/pergerakan sumber sebelum menentukan koreksi berikutnya.

Jurnal retur operasional dibuka melalui **Jurnal retur** atau **Jurnal pembalikan** pada dokumennya. Koreksi transaksi tersebut mengikuti alur retur, bukan pembalikan jurnal manual. Lihat juga [laporan keuangan](./FINANCIAL_REPORTS.md), [pemenuhan penjualan](./SALES_FULFILLMENT.md), dan [panduan pembelian](./PURCHASING.md).
