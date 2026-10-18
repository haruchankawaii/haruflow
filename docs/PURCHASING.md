# Supplier dan pesanan pembelian

Pesanan pembelian atau purchase order (PO) mencatat barang yang ingin dibeli dari supplier, jumlahnya, biaya beli yang disepakati, dan perkiraan kedatangan. Biaya beli diisi sendiri; harga jual dari katalog bukan biaya pembelian.

Tahap ini menyediakan master supplier serta PO. Penerimaan barang dari PO, tagihan supplier, utang usaha, dan pembayaran supplier belum tersedia. Membuat atau mengonfirmasi PO belum menambah stok, membukukan utang, atau mengeluarkan kas. Nilai PO dikonfirmasi adalah komitmen pembelian, bukan saldo utang.

## Hak akses dan supplier

Semua pengguna yang sudah masuk dapat membaca menu **Pembelian**. Administrator dan Warehouse dapat membuat/mengubah supplier, menyusun PO, mengonfirmasi, serta membatalkannya. Role pembelian tersendiri belum ditambahkan.

Pada tab Supplier, isi kode unik, nama, dan kontak yang diperlukan. Kode dinormalisasi menjadi huruf besar. Supplier dapat dinonaktifkan agar tidak dipakai pada PO baru atau perubahan draf. Riwayat supplier dan PO tetap disimpan; penonaktifan tidak membatalkan PO lama. Aktifkan kembali supplier sebelum melanjutkan konfirmasi drafnya.

## Alur PO

1. Buat PO dengan supplier aktif dan satu atau lebih produk barang dari katalog. Produk jasa belum didukung pada alur pembelian ini.
2. Masukkan jumlah dan biaya beli per unit. Perkiraan kedatangan serta catatan bersifat opsional; tanggal ini bukan tanggal posting akuntansi atau jatuh tempo pembayaran.
3. Simpan sebagai draf, lalu tinjau rincian. Draf dapat diubah. Nomor otomatis memakai urutan `PO-000001` dan seterusnya; perubahan awalan nomor penjualan tidak memengaruhinya. Nomor yang terlewati tidak dipakai ulang.
4. Konfirmasi setelah rincian disetujui. PO dikonfirmasi tidak dapat diedit. Nama/kode supplier dan SKU/nama/satuan barang pada dokumen merupakan snapshot agar penggantian nama katalog tidak mengubah dokumen historis. Menyimpan ulang draf memperbarui snapshot dari master data saat itu.
5. Bila rencana berubah, batalkan dengan alasan 3–1.000 karakter. Draf dan PO dikonfirmasi dapat dibatalkan; PO dibatalkan tidak dapat dibuka kembali. Buat PO pengganti bila masih diperlukan.

Versi dokumen mencegah perubahan pengguna lain tertimpa. Jika muncul konflik versi, muat ulang dokumen dan periksa perubahan sebelum mencoba lagi. Idempotency key pada penulisan mencegah pengulangan request yang sama membuat supplier/PO atau transisi tambahan. Alasan pembatalan dan audit tetap tersimpan.

Kuantitas mendukung tiga desimal, biaya unit empat desimal, dan total uang dua desimal. Nilai setiap baris dihitung dari jumlah × biaya, dibulatkan ke dua desimal dengan nilai tengah menjauhi nol, lalu dijumlahkan. Contoh `2,5 × Rp60.000,1234` menghasilkan total **Rp150.000,31**. Nilai harus positif; hasil pembulatan baris nol ditolak. Satu PO berisi 1–100 produk berbeda, kuantitas/biaya unit paling besar 1 miliar, serta total baris/dokumen paling besar Rp1 triliun.

Daftar PO menampilkan paling banyak 500 dokumen terbaru; pencarian, filter, ringkasan, dan ekspor CSV memakai daftar yang dimuat tersebut. Detail PO tetap dapat dibaca melalui endpoint berdasarkan ID. Supplier yang sudah dipakai dan barang pada PO tetap memiliki referensi historis; jenis barang tidak dapat diubah menjadi jasa setelah dipakai.

## Tahap berikutnya

Penerimaan barang dari PO akan menghubungkan dokumen pembelian dengan stok dan pembukuan. Sesudahnya, tagihan supplier, jatuh tempo, pembayaran sebagian/penuh, laporan utang, dan chart pembelian/pembayaran melengkapi alur. Penerimaan stok manual yang tersedia saat ini mengkredit modal awal; gunakan hanya bila memang sesuai transaksi, bukan sebagai pengganti utang supplier.
