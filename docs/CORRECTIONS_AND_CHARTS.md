# Koreksi transaksi dan membaca chart

## Membalik jurnal manual

Administrator atau Finance dapat membuka **Akuntansi → Jurnal umum**, membuka rincian jurnal manual, lalu memilih **Balikkan jurnal**. Periksa akun dan nominal asal, isi alasan, dan simpan pembalikan. Buat jurnal pengganti melalui **Buat jurnal** jika masih diperlukan.

Pembalikan membuat jurnal baru dengan seluruh debit dan kredit berlawanan. Jurnal asal, alasan, pengguna, waktu, dan tautan ke jurnal pembalik tetap tersimpan. Satu jurnal hanya dapat dibalik satu kali, termasuk jika permintaan dikirim ulang atau dua pengguna mencoba bersamaan. Jurnal dari invoice, pembayaran, stok, atau saldo awal mengikuti fasilitas pembalikan dokumen asalnya, bukan tombol jurnal manual.

Contoh: jurnal kas Rp1.000.000 salah dicatat. Balik jurnal itu, lalu buat jurnal pengganti Rp900.000. Riwayat memperlihatkan catatan awal, pembalikan, dan angka pengganti.

## Membatalkan penerimaan stok

Administrator atau Warehouse dapat membuka **Inventori → Pergerakan stok**, mencari penerimaan, lalu memilih **Batalkan penerimaan** jika tersedia. Periksa jumlah dan nilai asal, isi alasan, lalu catat ulang penerimaan yang benar melalui **Terima stok**.

Pembatalan menambahkan pergerakan stok keluar dan jurnal lawan yang terhubung ke penerimaan asal. Jumlah, nilai persediaan, dan biaya rata-rata dikembalikan persis ke posisi sebelum penerimaan; biaya jual barang yang sudah dikirim tidak dihitung ulang.

Batas pembatalan:

- Penerimaan harus dicatat setelah upgrade fitur ini, ketika aplikasi mulai menyimpan posisi stok sebelum penerimaan. Dokumen lama tetap ada, tetapi tidak mempunyai snapshot biaya yang cukup untuk pembatalan yang pasti.
- Tidak boleh ada pergerakan stok berikutnya untuk produk yang sama. Pengiriman, penyesuaian, penerimaan lain, dan pergerakan pembalik tetap dihitung sebagai riwayat berikutnya. Membalik transaksi berikutnya tidak menghapus riwayat tersebut.
- Saldo saat ini harus cocok dengan posisi hasil penerimaan. Permintaan yang tidak cocok ditolak tanpa mengubah stok atau jurnal.
- Penerimaan yang sudah dibatalkan tidak dapat dibatalkan lagi. Periode pembukuan saat ini harus masih terbuka, sama seperti pembalikan jurnal manual.

Alasan pemblokiran ditampilkan pada riwayat. Untuk selisih stok yang sudah melewati batas ini, periksa transaksi terkait sebelum memakai **Sesuaikan stok** dengan alasan dan biaya yang benar. Alur di atas berlaku untuk penerimaan stok manual yang mengkredit modal awal. Penerimaan dari PO dikoreksi melalui riwayat **Pembelian** dan membalik akun barang diterima belum ditagih; lihat [panduan pembelian](./PURCHASING.md). Penyesuaian stok saja tidak otomatis membalik kewajiban PO. Pembatalan penerimaan bukan retur komersial kepada supplier; tagihan dan pembayaran supplier belum tersedia.

## Membaca dashboard

Kartu utama menampilkan saldo saat ini atau akumulasi seluruh transaksi. Pilihan **3, 6, atau 12 bulan** hanya mengubah dua chart bulanan dan ringkasan periodenya. Bulan tanpa transaksi tetap ditampilkan dengan nilai nol.

- **Pendapatan dan HPP:** berasal dari jurnal akun pendapatan `4000` dan HPP `5000`, dikelompokkan menurut bulan posting di zona waktu perusahaan. Pembalikan masuk ke bulan pembalikan, sehingga nilai bersih dapat negatif. Laba kotor adalah pendapatan dikurangi HPP; angka ini belum mengurangi beban operasional lain atau pajak.
- **Mutasi kas:** kas masuk adalah debit dan kas keluar adalah kredit pada akun kas `1100`. Selisih menunjukkan perubahan kas pada rentang tersebut, bukan saldo akhir kas. Saldo awal, modal, jurnal manual, dan pembalikannya ikut dihitung; kas masuk bukan hanya pembayaran pelanggan.
- **Posisi invoice aktif:** bagian terbayar dibanding sisa piutang dari seluruh invoice aktif saat ini, beserta jumlah invoice lunas, sebagian, dan belum dibayar. Bagian ini tidak mengikuti pilihan bulan. Invoice yang dibatalkan tidak dihitung; belum ada kelompok piutang jatuh tempo karena tanggal jatuh tempo belum tersedia.

Sorot atau fokuskan batang chart untuk melihat nominal tepatnya. Rincian bulanan tersedia untuk memeriksa angka tanpa hanya mengandalkan bentuk grafik. Chart mengikuti data nyata, tanpa mengisi transaksi contoh ke dalam pembukuan pengguna.
