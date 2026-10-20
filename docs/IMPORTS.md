# Impor master dari CSV

Impor membantu memindahkan **pelanggan, supplier, dan produk** ke Haruflow. Buka **Pengaturan → Impor data**, pilih jenis data, lalu unduh template. Hak impor mengikuti pengelolaan master: Administrator dapat mengimpor semuanya, Sales pelanggan, serta Warehouse supplier dan produk.

Impor hanya **membuat data baru**. Impor tidak menimpa master yang sudah ada, membuat transaksi penjualan/pembelian, atau mengubah stok, biaya persediaan, kas, dan jurnal. Masukkan stok awal melalui alur saldo awal atau penerimaan yang sesuai, bukan kolom tambahan dalam CSV.

## Menyiapkan file

Gunakan template dari aplikasi dan simpan sebagai CSV **UTF-8**, dengan atau tanpa BOM. Pilih pemisah koma atau titik koma sesuai file; pemisah kolom berbeda dari pemisah desimal angka.

- Pelanggan: `name` wajib; `email`, `phone`, dan `address` opsional.
- Supplier: `code` dan `name` wajib; `email`, `phone`, dan `address` opsional. Supplier baru aktif secara default.
- Produk: `sku`, `name`, dan `price` wajib; `kind` dan `unit` opsional. Jenis adalah `stock` atau `service`; nilai kosong memakai `stock`. Satuan kosong memakai `pcs`.

Nama header memakai huruf kecil persis seperti template. Urutan kolom boleh berbeda. Header wajib yang hilang, header ganda, dan kolom yang tidak dikenal ditolak agar salah nama kolom tidak diam-diam menghilangkan data.

Angka harga memakai titik desimal, misalnya **`25000.50`**, tanpa pemisah ribuan atau simbol Rp. Memilih CSV titik koma tidak mengubah aturan angka tersebut. Harga harus positif, paling besar Rp1 miliar, dan paling banyak dua desimal. Nomor telepon sebaiknya disimpan sebagai teks di spreadsheet agar awalan nol tidak hilang sebelum ekspor.

Teks yang memuat pemisah, tanda petik, atau baris baru harus mengikuti aturan CSV: bungkus dengan tanda petik ganda dan tulis tanda petik di dalamnya dua kali. Contoh nama `Kopi, susu` ditulis `"Kopi, susu"`. Alamat boleh memiliki baris baru dalam field bertanda petik.

Batas file adalah **256 KiB dan 1.000 baris data** per preview, dengan nama file 1–120 karakter tanpa karakter kontrol. Gunakan beberapa file untuk data lebih besar. Kolom tidak dapat dipakai untuk mengirim saldo stok atau nilai akun.

## Preview sebelum penyimpanan

1. Pilih jenis master dan pemisah, lalu pilih file CSV.
2. Jalankan preview. Sistem membaca dan memvalidasi seluruh file tanpa membuat master.
3. Periksa data hasil pembacaan, jumlah baris valid, dan kesalahan menurut baris/kolom. Nomor baris mengikuti posisi pada file; alamat beberapa baris dapat membuat nomor berikutnya melompat.
4. Perbaiki file bila ada kesalahan dan jalankan preview baru.
5. Jika semua baris valid, tinjau lalu simpan impor. Seluruh baris disimpan dalam satu transaksi, beserta audit dan riwayat impor.

Baris yang valid tidak dipisahkan untuk disimpan ketika file masih mempunyai kesalahan. Jika satu baris gagal saat penyimpanan, seluruh impor dibatalkan. Permintaan ulang yang sama tidak membuat master tambahan.

## Duplikasi dan perubahan setelah preview

Kode supplier dan SKU dinormalisasi menjadi huruf besar. Duplikasi dalam file maupun terhadap data yang sudah ada ditolak. Karena pelanggan belum mempunyai kode master tersendiri, impor pelanggan memakai nama yang dibandingkan tanpa membedakan kapitalisasi dan spasi tepi sebagai identitas duplikat. Pelanggan yang memang berbeda tetapi memiliki nama sama perlu ditinjau dan dikelola melalui master sebelum mengimpor; aplikasi tidak menebak kecocokannya dari email atau nomor telepon.

Preview bukan reservasi nama/kode. Saat penyimpanan, server memeriksa kembali data terbaru dalam transaksi. Jika pengguna lain sudah menambahkan identitas yang sama, impor ditolak tanpa sebagian baris tersimpan. Muat data terbaru, perbaiki file, lalu buat preview baru.

Hanya pembuat preview yang dapat menyimpannya, dengan hak master yang masih sesuai. Mengubah pilihan jenis data, file, atau pemisah memerlukan preview baru.

## Riwayat dan backup

Riwayat menyimpan nama file, jenis master, pembuat, waktu, hasil validasi, data yang dipreview, dan hasil penyimpanan. Pengguna dengan hak impor jenis tersebut dapat meninjau riwayat; preview milik orang lain tidak dapat disimpan atas namanya.

Preview yang tersimpan tidak berarti master sudah dibuat. Periksa status serta jumlah data yang benar-benar dibuat. Riwayat bukan fasilitas pembatalan massal: koreksi master mengikuti alur pengelolaan master dan larangan penghapusan bila telah digunakan dokumen.

Riwayat serta master berada di PostgreSQL dan ikut backup/restore yang sama. Tidak perlu menyimpan file CSV di direktori server untuk menjalankan impor. Arsip sumber di luar Haruflow dikelola sesuai kebutuhan usaha.

Impor dokumen transaksi serta saldo awal piutang/utang per pihak belum termasuk alur ini. Lihat [status implementasi](./IMPLEMENTATION_STATUS.md) dan [panduan saldo awal](./OPENING_BALANCES.md).
