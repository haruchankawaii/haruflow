# Biaya perolehan persediaan

Biaya perolehan (`landed cost`) memindahkan biaya pembelian yang **sudah dibukukan** ke persediaan dan HPP. Dokumen ini tidak membuat pembayaran, utang baru, atau mengubah penerimaan, tagihan, pengiriman, maupun jurnal lama. Community mendukung satu perusahaan dengan mata uang IDR.

Gunakan menu **Biaya perolehan** untuk memilih bukti biaya, nominal yang belum dialokasikan, dan barang penerimaan yang menerima biaya. Tinjau pembagian persediaan/HPP sebelum membukukan. Finance dan admin dapat membuat serta membalik dokumen; pengguna lain yang sudah masuk dapat membaca riwayat.

## Sumber biaya yang dapat dipilih

- Ongkir dari tagihan supplier aktif yang sudah dibukukan.
- Selisih harga pembelian positif dari tagihan supplier aktif.
- Beban jasa dari penerimaan pembelian, setelah seluruh penerimaan tersebut ditagihkan dengan nilai dasar yang sama.

Satu baris debit biaya dapat dipakai beberapa kali sampai seluruh nominalnya dialokasikan. Dokumen yang dibalik mengembalikan kapasitas sumber. Pemeriksaan ini menggunakan sumber jurnal tertentu, bukan sekadar saldo akun beban, sehingga biaya yang sama tidak dapat dikapitalisasi dua kali.

Sumber yang sudah dibalik atau mempunyai retur supplier aktif tidak dapat dipakai. Tagihan dengan diskon atau pajak inklusif memerlukan bukti biaya neto terpisah: pemisahan komponen pada tagihan tersebut belum cukup untuk memastikan biaya neto yang layak dikapitalisasi. Untuk sumber jasa, seluruh penerimaan wajib sudah ditagihkan tanpa selisih harga, diskon, atau pajak inklusif. Pajak masukan tidak menjadi sumber biaya perolehan. Tagihan dengan pajak eksklusif tetap dapat dipakai karena pajak dicatat pada akun tersendiri.

Biaya belum ditagihkan, biaya dari jurnal manual, dan biaya baru yang belum dibukukan tidak dapat langsung ditambahkan lewat fitur ini. Catat bukti pembeliannya terlebih dahulu. Pemilihan sumber tetap merupakan keputusan akuntansi pengguna: sistem memeriksa sumber dan batas nominal, sementara alasan serta referensi menyimpan dasar alokasi.

## Tujuan dan metode pembagian

Pilih 1–100 baris **barang** dari penerimaan PO yang masih aktif. Jasa tidak dapat menjadi tujuan. Satu dokumen hanya boleh memilih satu penerimaan untuk setiap produk, agar snapshot perubahan nilai setiap produk jelas.

Metode **nilai** memakai nilai asli setiap baris penerimaan sebagai bobot. Metode **kuantitas** memakai jumlah satuan dasar, dan hanya tersedia jika satuan seluruh tujuan sama. Pembagian memakai akumulasi bobot dengan pembulatan dua desimal; baris terakhir menerima sisa agar total selalu tepat. Baris dengan bagian nol sen tetap muncul dalam snapshot.

Nilai asli penerimaan dan satuannya dipertahankan. Bila satuan produk sudah berubah atau terdapat penyesuaian, retur, maupun pembalikan stok setelah penerimaan, tujuan diblokir. Sistem tidak menebak ulang asal stok dari riwayat tersebut. Penerimaan tambahan dan pengiriman biasa didukung; transfer gudang tidak mengubah kuantitas global.

## Barang yang sudah terjual

Persediaan menggunakan rata-rata bergerak global. Biaya tambahan yang terlambat dicatat dibagi berdasarkan proporsi biaya yang masih berada dalam kumpulan kuantitas produk, bukan berdasarkan dugaan lot fisik barang terjual.

Mulai dari kuantitas persediaan tepat setelah penerimaan tujuan. Penerimaan berikutnya menambah kumpulan kuantitas. Setiap pengiriman mengurangi bagian biaya yang masih tersisa secara proporsional terhadap kuantitas sebelum dan sesudah pengiriman. Setelah persediaan habis, bagian biaya lama tetap nol meskipun ada penerimaan baru. Riwayat kuantitas harus menghasilkan saldo produk saat ini secara tepat.

Contoh: menerima 10 unit senilai Rp100, lalu mengirim 4 unit. Biaya tambahan Rp100 terbagi menjadi Rp60 persediaan dan Rp40 HPP. Nilai 6 unit yang tersisa menjadi Rp120. Jika kemudian ada penerimaan tambahan sebelum biaya dibukukan, perhitungan memperhitungkan pengenceran kumpulan rata-rata tersebut. Persediaan yang sudah habis menerima Rp0 kapitalisasi; seluruh tambahan menjadi HPP.

Perhitungan ini membuat jurnal penyesuaian pada periode berjalan. HPP pengiriman lama tidak diubah dan tidak dibukukan ulang ke tanggal historis. Snapshot menyimpan bobot, proporsi tersisa, pembagian sen, kuantitas, nilai, biaya rata-rata sebelum/sesudah, dan batas urutan mutasi.

Jurnal pembukuan:

- Debit persediaan `1300` sebesar bagian yang masih tersisa.
- Debit HPP `5000` sebesar bagian yang sudah keluar.
- Kredit akun biaya sumber sebesar seluruh alokasi.

Nilai tambahan persediaan disebarkan kembali secara proporsional ke saldo gudang/lot produk yang ada. Jumlah barang dan reservasi tidak berubah. Kejadian penilaian lokasi ditautkan ke dokumen biaya perolehan. Total kuantitas/nilai gudang tetap sama dengan saldo produk, dan kenaikan nilai produk sama dengan debit jurnal persediaan.

## Koreksi dan urutan pembalikan

Dokumen yang sudah dibukukan tidak dapat diedit atau dihapus. Pembalikan membuat jurnal lawan dan kejadian penilaian lokasi baru. Alasan wajib diisi. Periode berjalan harus terbuka dan versi dokumen harus cocok.

Untuk bagian yang dikapitalisasi, pembalikan hanya tersedia jika kuantitas, nilai, biaya rata-rata, serta riwayat stok/lokasi masih sesuai snapshot setelah alokasi. Pengiriman, penerimaan, transfer, atau penyesuaian berikutnya dapat memblokir pembalikan: membalik nilai saat itu akan mengubah HPP yang sudah bergantung pada biaya baru. Alokasi biaya berikutnya harus dibalik lebih dahulu; kejadian penilaian dari alokasi yang sudah dibalik tidak mengunci alokasi sebelumnya. Bagian yang seluruhnya dibebankan ke HPP tidak memulihkan atau mengurangi stok baru.

Selama alokasi aktif:

- Tagihan/penerimaan sumber biaya dan penerimaan tujuan tidak dapat dibalik.
- Retur atas tagihan sumber biaya, termasuk tagihan yang memakai penerimaan sumber jasa, diblokir.
- Mutasi stok lama yang dipakai menghitung alokasi tidak dapat dibalik.
- Retur pelanggan dari pengiriman lama yang dipakai menghitung alokasi diblokir, karena biaya pengiriman historisnya belum mencakup tambahan landed cost. Balikkan alokasi terlebih dahulu jika masih memenuhi syarat. Retur dari pengiriman sesudah alokasi memakai biaya yang sudah mencakup tambahan tersebut.

Retur supplier atas barang tujuan setelah alokasi tetap memakai biaya rata-rata saat retur, termasuk biaya tambahan yang masih melekat pada stok, selama tagihannya bukan sumber biaya alokasi aktif. Selisih terhadap kredit supplier mengikuti akun selisih retur yang sudah ada. Pembalikan biaya perolehan kemudian diblokir oleh riwayat stok tersebut.

Jika pembalikan diblokir oleh kegiatan berikutnya, jangan mengubah basis data atau jurnal sumber. Tinjau urutan dokumen dan lakukan koreksi keuangan eksplisit yang sesuai keadaan periode berjalan. Community tidak memutar ulang seluruh sejarah harga pokok.

## Kontrak API

Semua nominal adalah string desimal; ID memakai UUID. Daftar `GET /api/landed-costs` mengembalikan `{items:[...]}` untuk 200 dokumen terbaru. `GET /api/landed-costs/options` mengembalikan `{sources:[...],targets:[...]}` dengan `eligible` dan `blocked_reason`; batas pembacaan pilihan adalah 500 sumber dan 1.000 baris penerimaan terbaru. Dokumen lama tetap dapat dibaca langsung dengan ID.

`POST /api/landed-costs/preview` menerima:

```json
{
  "source_line_id": "UUID baris jurnal biaya",
  "amount": "100.00",
  "allocation_method": "value",
  "receipt_line_ids": ["UUID baris penerimaan barang"],
  "reference": "Nomor bukti ongkir",
  "reason": "Dasar pembagian biaya pembelian",
  "idempotency_key": "UUID baru"
}
```

Pratinjau mengembalikan sumber, kapasitas sumber, rincian alokasi, peringatan, dan `preview_token`. Pembukuan `POST /api/landed-costs` memakai isi yang sama ditambah `expected_preview` berisi token tersebut. Perubahan nominal, sumber, stok, lokasi, atau dasar pembagian setelah peninjauan menghasilkan `409`; buat pratinjau ulang. Pratinjau tidak membuat posting atau audit keuangan.

`GET /api/landed-costs/{id}` menyertakan `version`, `status` (`posted`/`reversed`), `journal_id`, `items`, `location_events`, `can_reverse`, `reversal_blocked_reason`, dan bukti pembalikan bila ada. Untuk membalik, kirim `POST /api/landed-costs/{id}/reverse` dengan `{version,reason,idempotency_key}`.

Pembukuan dan pembalikan bersifat atomik, memakai kunci periode akuntansi yang sama dengan posting lain, mengunci produk dalam urutan ID, serta memakai idempotensi tersimpan. Pengulangan kunci dengan isi sama mengembalikan hasil awal; isi berbeda menghasilkan konflik. Tabel dokumen/rincian/tautan penilaian dilindungi dari perubahan, penghapusan, dan truncate, dengan pemeriksaan tertunda atas total, kapasitas sumber, jurnal, dan perubahan nilai lokasi. Pelindung penambahan rincian juga memeriksa transaksi fisik header agar tetap aman setelah restore logis.

## Bukti pengujian

`crates/server/src/landed_costs_tests.rs` mencakup pembagian stok terjual, persamaan produk/lokasi/jurnal, pelestarian posting lama, pembalikan, pembulatan sen, urutan beberapa alokasi, konkurensi kapasitas sumber, token pratinjau usang, hak akses, periode tertutup, ketergantungan retur, sumber jasa final, dan persediaan yang sudah habis. Uji aritmetika pada `landed_costs.rs` memeriksa penyusutan kumpulan, penerimaan tambahan, penghabisan stok, dan penolakan riwayat ambigu. Hasil eksekusi aktual dicatat pada laporan QA/release; keberadaan pengujian bukan klaim kelulusan rilis.
