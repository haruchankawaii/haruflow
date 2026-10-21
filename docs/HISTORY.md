# Mencari riwayat lengkap

Gunakan **Riwayat lengkap** untuk mencari catatan lama yang berada di luar daftar ringkas Penjualan, Pembelian, Inventori, atau master. Halaman ini mengambil hasil pencarian dan jumlah data dari server, 50 baris per halaman. Catatan diurutkan dari waktu pencatatan terbaru, lalu identitas dokumen agar urutan tetap pasti bila waktunya sama.

## Mencari dan membuka sumber

1. Pilih **Jenis catatan**: pesanan penjualan, invoice pelanggan, pesanan pembelian, tagihan supplier, pelanggan, supplier, produk, mutasi persediaan, jurnal, uang muka/kredit, impor data, atau aktivitas.
2. Isi nomor, nama, atau referensi yang sesuai dengan jenis catatan. Pencarian maksimal 200 karakter; tanda `%` atau `_` dibaca sebagai karakter biasa.
3. Pilih status/jenis bila tersedia serta tanggal pencatatan awal/akhir bila diperlukan. Tanggal memakai zona waktu perusahaan, termasuk kedua tanggal batas.
4. Pilih **Cari catatan**. Filter baru kembali ke halaman pertama. Gunakan tombol halaman untuk melanjutkan hasil.
5. Buka tautan sumber pada baris dokumen atau rincian jurnal bila tersedia. Beberapa master/mutasi tidak mempunyai halaman rincian tersendiri; barisnya tetap dapat ditemukan dan diekspor.

Rentang ini memakai **tanggal pencatatan**, bukan tanggal invoice pemasok atau jatuh tempo. Untuk posisi piutang/utang pada tanggal tertentu, gunakan **Saldo pihak**; untuk laba rugi, neraca, atau arus kas, gunakan **Laporan keuangan**. Riwayat lengkap merupakan sarana penelusuran, bukan pengganti perhitungan laporan.

Nominal mengikuti jenis sumber: invoice/tagihan menampilkan sisa, uang muka/kredit menampilkan saldo tersedia, pesanan menampilkan total, dan produk menampilkan harga. Nominal yang tidak berlaku ditampilkan kosong. Jangan menjumlahkan jenis catatan berbeda sebagai saldo keuangan.

## Ekspor seluruh hasil

Pilih **Ekspor seluruh hasil** setelah menerapkan pencarian. CSV memakai filter yang sudah diterapkan dan mengambil seluruh halaman, maksimal 10.000 baris. Bila hasil melebihi batas, server menolak ekspor dan meminta filter dipersempit; file tidak dipotong diam-diam pada halaman atau baris tertentu.

CSV mempertahankan nilai dan quoting teks. Perlindungan terhadap teks yang dapat dibaca sebagai formula spreadsheet diterapkan pada ekspor. Buka file sesuai kebijakan aplikasi spreadsheet yang digunakan.

## Hak akses dan batas

Pengguna yang sudah masuk dapat membaca jenis catatan umum. **Aktivitas** hanya tersedia untuk Administrator/Finance. Pada **Impor data**, Administrator dapat melihat seluruh riwayat, sedangkan pengguna lain hanya preview yang dibuatnya sendiri. Memilih halaman riwayat tidak memberi hak mengubah dokumen; tindakan tulis tetap mengikuti peran dan aturan sumber.

Daftar ringkas dan CSV pada halaman operasional tetap memakai batasnya masing-masing. Untuk arsip yang lebih besar gunakan halaman ini dan perhatikan judul filter sebelum membagikan ekspor. Jumlah hasil dan baris satu respons dibaca dari satu snapshot; perubahan pengguna lain setelah respons dapat terlihat saat pencarian berikutnya.
