# Pembelian, penerimaan, dan tagihan supplier

Pesanan pembelian atau purchase order (PO) mencatat barang yang ingin dibeli dari supplier, jumlahnya, biaya beli yang disepakati, dan perkiraan kedatangan. Biaya beli diisi sendiri; harga jual dari katalog bukan biaya pembelian.

Modul menyediakan supplier, PO, penerimaan barang penuh dari PO, serta tagihan supplier dan jatuh tempo. Membuat atau mengonfirmasi PO belum menambah stok, membukukan utang, atau mengeluarkan kas. Penerimaan menambah persediaan dan kewajiban barang diterima belum ditagih; tagihan memindahkannya ke utang supplier. Pembayaran supplier belum tersedia. Nilai PO dikonfirmasi yang belum diterima masih merupakan komitmen pembelian.

## Hak akses dan supplier

Semua pengguna yang sudah masuk dapat membaca menu **Pembelian**, chart, tagihan, riwayat penerimaan, dan jurnal terkait. Administrator dan Warehouse dapat mengelola supplier, PO, penerimaan, serta koreksi penerimaan yang memenuhi syarat. Administrator dan Finance dapat mencatat serta membalik tagihan supplier; Warehouse tidak dapat melakukannya. Role pembelian tersendiri belum ditambahkan.

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

Pilih PO dikonfirmasi, tinjau jumlah dan biaya seluruh baris, lalu isi referensi surat jalan bila ada. Jumlah dan biaya berasal dari PO tersimpan; tahap ini belum menerima sebagian, mengubah biaya saat menerima, menambah ongkos, atau menghitung pajak. Satuan setiap barang harus tetap sama dengan snapshot PO. Jika satuan katalog berubah, seluruh penerimaan ditolak agar tidak terjadi konversi diam-diam; periksa perubahan katalog atau buat PO pengganti dengan satuan yang benar. Supplier yang belakangan dinonaktifkan tetap dapat memenuhi PO yang sebelumnya sudah dikonfirmasi.

Posting penerimaan menyimpan seluruh perubahan secara atomik:

- Dokumen penerimaan bernomor `GR-000001`, snapshot barang/biaya, dan audit.
- Mutasi stok masuk, nilai persediaan, dan biaya rata-rata baru untuk setiap barang.
- Jurnal debit Persediaan (1300) dan kredit **Barang diterima belum ditagih** sesuai total PO.
- PO berstatus **Diterima**, dengan tepat satu penerimaan aktif.

Kas, pendapatan, dan HPP tidak berubah ketika menerima barang. Contoh: 3 unit × Rp60.000 menambah persediaan serta kewajiban sementara sebesar Rp180.000. Kewajiban ini belum merupakan tagihan supplier dengan nomor invoice dan tanggal jatuh tempo.

Akun penampung tersebut merupakan akun kontrol sistem. Migrasi memilih kode bebas mulai 2110–2199; bila semuanya terpakai, dipilih kode sepuluh digit yang belum dipakai. Akun pengguna yang sudah ada tidak diambil alih atau diubah. Periksa kode aktual pada Daftar akun. Akun ini tidak dapat diisi melalui jurnal manual atau saldo awal, agar saldonya cocok dengan penerimaan aktif yang belum ditagih.

Penerimaan dan koreksi memakai waktu posting saat ini, zona waktu perusahaan, serta pemeriksaan periode akuntansi. Periode tertutup menolak posting baru. Request berulang dengan idempotency key sama mengembalikan hasil semula tanpa penerimaan atau jurnal tambahan.

## Koreksi penerimaan

Pilih **Balikkan penerimaan** pada riwayat PO, tinjau dampaknya, dan isi alasan 3–1.000 karakter. Jika penerimaan sudah ditagih, balikkan tagihan supplier aktif terlebih dahulu. Pembalikan penerimaan hanya tersedia bila penerimaan masih aktif, tidak memiliki tagihan aktif, periode terbuka, dan **setiap barang** belum memiliki mutasi stok lanjutan. Mutasi yang kemudian sudah dibalik tetap dihitung sebagai riwayat lanjutan. Pemeriksaan juga mencocokkan jumlah, nilai, serta biaya rata-rata dengan snapshot penerimaan.

Bila memenuhi syarat, pembalikan memulihkan jumlah/nilai/biaya rata-rata sebelumnya secara persis, menambah mutasi dan jurnal lawan, serta mengembalikan PO ke Dikonfirmasi. Dokumen asal, alasan, jurnal asal/pembalik, dan semua riwayat tetap tersimpan. Seluruh barang dikoreksi bersama; jika satu barang tidak memenuhi syarat, tidak ada perubahan parsial.

PO tersebut dapat diterima ulang sehingga muncul dokumen GR baru. Isi PO dikonfirmasi tetap terkunci: bila biaya atau jumlah PO salah, balik penerimaan bila aman, batalkan PO, lalu buat PO pengganti. Jika pembalikan diblokir oleh mutasi lanjutan, periksa stok, HPP, dan kewajiban terkait sebelum menentukan koreksi. Penyesuaian stok saja tidak otomatis menghapus kewajiban penerimaan PO.

## Tagihan supplier dan jatuh tempo

Pada tab **Tagihan**, pilih **Catat tagihan** atau mulai dari penerimaan aktif pada rincian PO:

1. Pilih penerimaan aktif dari PO berstatus Diterima yang belum memiliki tagihan aktif. Satu penerimaan hanya ditagih penuh satu kali. Barang, jumlah, satuan, biaya, dan total berasal dari snapshot penerimaan; tidak diisi ulang atau diubah saat penagihan. Mutasi stok setelah penerimaan tidak menghalangi penagihan, dan supplier yang sudah nonaktif tetap dapat ditagih atas barang yang telah diterima.
2. Isi nomor invoice supplier, tanggal invoice, tanggal jatuh tempo, dan catatan opsional. Nomor invoice berisi 1–100 karakter; kombinasi supplier dan nomor invoice aktif harus unik setelah spasi tepi dihapus dan tanpa membedakan huruf besar/kecil. Misalnya `INV-123` dan ` inv-123 ` milik supplier yang sama dianggap sama. Nomor yang sama pada supplier berbeda diperbolehkan.
3. Tanggal menggunakan kalender `YYYY-MM-DD`, tahun 0001–9999. Tanggal invoice tidak boleh melewati hari ini menurut zona waktu perusahaan; jatuh tempo tidak boleh sebelum tanggal invoice. Invoice bertanggal lampau boleh dicatat, tetapi **jurnal tetap diposting saat ini** dan hanya pada periode terbuka. Tanggal invoice tidak membuat backdating jurnal atau membuka periode lama.
4. Tinjau lalu posting. Nomor internal memakai `PB-000001` dan urutan berikutnya, terpisah dari nomor invoice supplier. Nomor terlewati tidak dipakai ulang. Dokumen, jurnal, perubahan versi PO, audit, dan hasil idempotensi disimpan bersama.

Jurnal tagihan adalah **debit Barang diterima belum ditagih / kredit Utang supplier** sebesar total penerimaan. Contoh penerimaan Rp180.000 menghasilkan tagihan Rp180.000: kewajiban sementara berkurang Rp180.000 dan utang supplier bertambah Rp180.000. Stok, nilai persediaan, biaya rata-rata, kas, pendapatan, serta HPP tetap sama. Status PO tetap Diterima.

Akun Utang supplier dibuat sebagai akun kontrol sistem pada kode kosong 2200–2299; bila rentang penuh, sistem memilih kode sepuluh digit kosong. Akun yang telah ada dan akun penampung penerimaan tidak digunakan ulang. Periksa kode aktual di Daftar akun. Akun ini juga dilindungi dari jurnal manual dan saldo awal, karena saldonya harus cocok dengan tagihan aktif. Pembukaan utang historis per supplier belum tersedia.

Status tagihan aktif adalah **Belum dibayar**. Status jatuh tempo membedakan **Belum jatuh tempo**, **Jatuh tempo hari ini**, dan **Lewat jatuh tempo** menurut hari perusahaan. Saat ini nominal terbayar selalu nol dan sisa utang sama dengan total tagihan aktif; kas belum dapat dibayarkan dari modul ini. Daftar, pencarian, filter, dan CSV memakai maksimal 500 tagihan terbaru, sedangkan ringkasan/chart dan utang per supplier memakai seluruh tagihan dalam satu snapshot database.

## Koreksi tagihan

Administrator atau Finance dapat memilih **Tinjau pembatalan**, memberi alasan 3–1.000 karakter, lalu **Batalkan tagihan** selama periode saat ini terbuka. Versi dokumen diperiksa untuk mencegah koreksi terhadap data lama. Pembalikan membuat jurnal lawan persis dari jurnal asal: debit Utang supplier / kredit Barang diterima belum ditagih. Tagihan menjadi **Dibatalkan** (`void`) dengan sisa utang nol; dokumen asal, snapshot penerimaan, alasan, dan jurnal asal/pembalik tetap tersimpan. Tidak ada perubahan stok atau kas.

Setelah tagihan dibalik, penerimaan dapat ditagih ulang dengan dokumen PB baru; nomor invoice supplier yang telah dibatalkan boleh digunakan lagi. Jika yang salah hanya nomor/tanggal/catatan tagihan, buat tagihan pengganti atas penerimaan yang sama. Jika jumlah/biaya penerimaan yang salah, pembalikan tagihan lebih dahulu tidak otomatis membolehkan koreksi stok: penerimaan tetap harus memenuhi syarat tanpa mutasi lanjutan. Pengulangan request posting atau pembalikan dengan kunci idempotensi yang sama mengembalikan hasil semula, bukan dokumen tambahan.

## Membaca chart pembelian

Chart PO menunjukkan jumlah seluruh PO menurut status Draf, Dikonfirmasi, Diterima, dan Dibatalkan, beserta nominal tersimpan. **Menunggu penerimaan** adalah total PO Dikonfirmasi; **Diterima belum ditagih** adalah total penerimaan aktif yang belum memiliki tagihan aktif. **Utang supplier** berasal dari seluruh tagihan aktif. Penagihan memindahkan nominal dari Diterima belum ditagih ke Utang supplier, sedangkan jumlah PO Diterima tetap sama. Pembalikan tagihan memindahkannya kembali ke kewajiban sementara; pembalikan penerimaan yang aman mengembalikan PO ke Dikonfirmasi.

Chart pada tab tagihan menunjukkan nominal sisa utang menurut tiga kelompok jatuh tempo. Ringkasan per supplier menampilkan jumlah tagihan aktif, total utang, dan bagian yang terlambat. Chart ini menunjukkan posisi saat ini, bukan kas keluar atau riwayat pembayaran. Tagihan yang dibatalkan tidak menambah utang atau kelompok jatuh tempo.

## Tahap berikutnya

Pembayaran supplier sebagian/penuh, koreksi pembayaran, laporan umur utang dengan kelompok hari keterlambatan, dan chart pembayaran menyusul. Penerimaan/tagihan sebagian, variasi biaya invoice, pajak, ongkos tambahan, dan retur komersial belum tersedia. Penerimaan stok manual tetap mengkredit modal awal; gunakan hanya bila memang sesuai transaksi, bukan sebagai pengganti penerimaan PO dan tagihan supplier.
