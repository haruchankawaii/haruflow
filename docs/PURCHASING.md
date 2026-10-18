# Supplier dan pesanan pembelian

Pesanan pembelian atau purchase order (PO) mencatat barang yang ingin dibeli dari supplier, jumlahnya, biaya beli yang disepakati, dan perkiraan kedatangan. Biaya beli diisi sendiri; harga jual dari katalog bukan biaya pembelian.

Modul menyediakan supplier, PO, serta penerimaan barang penuh dari PO. Membuat atau mengonfirmasi PO belum menambah stok, membukukan utang, atau mengeluarkan kas. Penerimaan menambah persediaan dan mencatat kewajiban barang diterima belum ditagih. Tagihan, jatuh tempo, dan pembayaran supplier belum tersedia; nilai PO dikonfirmasi yang belum diterima masih merupakan komitmen pembelian.

## Hak akses dan supplier

Semua pengguna yang sudah masuk dapat membaca menu **Pembelian**, chart, riwayat penerimaan, dan jurnal terkait. Administrator dan Warehouse dapat mengelola supplier, PO, penerimaan, serta koreksi penerimaan yang memenuhi syarat. Role pembelian tersendiri belum ditambahkan.

Pada tab Supplier, isi kode unik, nama, dan kontak yang diperlukan. Kode dinormalisasi menjadi huruf besar. Supplier dapat dinonaktifkan agar tidak dipakai pada PO baru atau perubahan draf. Riwayat supplier dan PO tetap disimpan; penonaktifan tidak membatalkan PO lama. Aktifkan kembali supplier sebelum melanjutkan konfirmasi drafnya.

## Alur PO

1. Buat PO dengan supplier aktif dan satu atau lebih produk barang dari katalog. Produk jasa belum didukung pada alur pembelian ini.
2. Masukkan jumlah dan biaya beli per unit. Perkiraan kedatangan serta catatan bersifat opsional; tanggal ini bukan tanggal posting akuntansi atau jatuh tempo pembayaran.
3. Simpan sebagai draf, lalu tinjau rincian. Draf dapat diubah. Nomor otomatis memakai urutan `PO-000001` dan seterusnya; perubahan awalan nomor penjualan tidak memengaruhinya. Nomor yang terlewati tidak dipakai ulang.
4. Konfirmasi setelah rincian disetujui. PO dikonfirmasi tidak dapat diedit. Nama/kode supplier dan SKU/nama/satuan barang pada dokumen merupakan snapshot agar penggantian nama katalog tidak mengubah dokumen historis. Menyimpan ulang draf memperbarui snapshot dari master data saat itu.
5. Terima barang melalui **Tinjau penerimaan** ketika seluruh barang benar-benar datang. Jika rencana berubah sebelum diterima, batalkan dengan alasan 3–1.000 karakter. PO dibatalkan tidak dapat dibuka kembali. PO dengan penerimaan aktif tidak dapat dibatalkan sebelum penerimaannya dibalik dengan aman.

Versi dokumen mencegah perubahan pengguna lain tertimpa. Jika muncul konflik versi, muat ulang dokumen dan periksa perubahan sebelum mencoba lagi. Idempotency key pada penulisan mencegah pengulangan request yang sama membuat supplier/PO atau transisi tambahan. Alasan pembatalan dan audit tetap tersimpan.

Kuantitas mendukung tiga desimal, biaya unit empat desimal, dan total uang dua desimal. Nilai setiap baris dihitung dari jumlah × biaya, dibulatkan ke dua desimal dengan nilai tengah menjauhi nol, lalu dijumlahkan. Contoh `2,5 × Rp60.000,1234` menghasilkan total **Rp150.000,31**. Nilai harus positif; hasil pembulatan baris nol ditolak. Satu PO berisi 1–100 produk berbeda, kuantitas/biaya unit paling besar 1 miliar, serta total baris/dokumen paling besar Rp1 triliun.

Daftar PO menampilkan paling banyak 500 dokumen terbaru; pencarian, filter, dan ekspor CSV memakai daftar tersebut. Chart serta ringkasan nominal dihitung dari **seluruh PO** dalam satu snapshot database. Detail PO tetap dapat dibaca berdasarkan ID, termasuk lewat tautan asal pada pergerakan stok. Supplier dan barang pada dokumen tetap memiliki referensi historis; jenis barang tidak dapat diubah menjadi jasa setelah dipakai.

## Penerimaan penuh dan pembukuan

Pilih PO dikonfirmasi, tinjau jumlah dan biaya seluruh baris, lalu isi referensi surat jalan bila ada. Jumlah dan biaya berasal dari PO tersimpan; tahap ini belum menerima sebagian, mengubah biaya saat menerima, menambah ongkos, atau menghitung pajak. Supplier yang belakangan dinonaktifkan tetap dapat memenuhi PO yang sebelumnya sudah dikonfirmasi.

Posting penerimaan menyimpan seluruh perubahan secara atomik:

- Dokumen penerimaan bernomor `GR-000001`, snapshot barang/biaya, dan audit.
- Mutasi stok masuk, nilai persediaan, dan biaya rata-rata baru untuk setiap barang.
- Jurnal debit Persediaan (1300) dan kredit **Barang diterima belum ditagih** sesuai total PO.
- PO berstatus **Diterima**, dengan tepat satu penerimaan aktif.

Kas, pendapatan, dan HPP tidak berubah ketika menerima barang. Contoh: 3 unit × Rp60.000 menambah persediaan serta kewajiban sementara sebesar Rp180.000. Kewajiban ini belum merupakan tagihan supplier dengan nomor invoice dan tanggal jatuh tempo.

Akun penampung tersebut merupakan akun kontrol sistem. Migrasi memilih kode bebas mulai 2110–2199; bila semuanya terpakai, dipilih kode sepuluh digit yang belum dipakai. Akun pengguna yang sudah ada tidak diambil alih atau diubah. Periksa kode aktual pada Daftar akun. Akun ini tidak dapat diisi melalui jurnal manual atau saldo awal, agar saldonya cocok dengan penerimaan aktif.

Penerimaan dan koreksi memakai waktu posting saat ini, zona waktu perusahaan, serta pemeriksaan periode akuntansi. Periode tertutup menolak posting baru. Request berulang dengan idempotency key sama mengembalikan hasil semula tanpa penerimaan atau jurnal tambahan.

## Koreksi penerimaan

Pilih **Balikkan penerimaan** pada riwayat PO, tinjau dampaknya, dan isi alasan 3–1.000 karakter. Pembalikan hanya tersedia bila penerimaan masih aktif, periode terbuka, dan **setiap barang** belum memiliki mutasi stok lanjutan. Mutasi yang kemudian sudah dibalik tetap dihitung sebagai riwayat lanjutan. Pemeriksaan juga mencocokkan jumlah, nilai, serta biaya rata-rata dengan snapshot penerimaan.

Bila memenuhi syarat, pembalikan memulihkan jumlah/nilai/biaya rata-rata sebelumnya secara persis, menambah mutasi dan jurnal lawan, serta mengembalikan PO ke Dikonfirmasi. Dokumen asal, alasan, jurnal asal/pembalik, dan semua riwayat tetap tersimpan. Seluruh barang dikoreksi bersama; jika satu barang tidak memenuhi syarat, tidak ada perubahan parsial.

PO tersebut dapat diterima ulang sehingga muncul dokumen GR baru. Isi PO dikonfirmasi tetap terkunci: bila biaya atau jumlah PO salah, balik penerimaan bila aman, batalkan PO, lalu buat PO pengganti. Jika pembalikan diblokir oleh mutasi lanjutan, periksa stok, HPP, dan kewajiban terkait sebelum menentukan koreksi. Penyesuaian stok saja tidak otomatis menghapus kewajiban penerimaan PO.

## Membaca chart pembelian

Chart menunjukkan jumlah seluruh PO menurut status Draf, Dikonfirmasi, Diterima, dan Dibatalkan, beserta nominal tersimpan. **Menunggu penerimaan** adalah total PO Dikonfirmasi; **Diterima belum ditagih** adalah total penerimaan aktif. Keduanya tidak mengukur kas keluar atau tagihan jatuh tempo. Pembalikan penerimaan memindahkan posisi PO kembali ke Dikonfirmasi dan mengurangi kewajiban sementara.

## Tahap berikutnya

Tagihan supplier akan memindahkan kewajiban penerimaan ke utang bertagihan. Jatuh tempo, pembayaran sebagian/penuh, laporan umur utang, dan chart pembayaran menyusul. Penerimaan stok manual tetap mengkredit modal awal; gunakan hanya bila memang sesuai transaksi, bukan sebagai pengganti penerimaan PO dan tagihan supplier.
