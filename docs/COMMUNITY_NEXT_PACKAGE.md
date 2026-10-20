# Paket pengembangan Community berikutnya

Status: rencana lingkup, 20 Oktober 2026. Fitur dalam dokumen ini belum diimplementasikan. Dasar rencana adalah implementasi sampai batas 10 akun aktif pada migrasi 0021.

Tujuan paket adalah membantu usaha berpindah ke Haruflow dengan saldo yang benar, mengelola dana pelanggan/supplier sampai selesai, serta menyiapkan kandidat rilis yang dapat diuji. Paket dikerjakan sebagai satu rangkaian dengan beberapa checkpoint yang diuji, di-commit, dan di-push; bukan satu perubahan besar tanpa pemeriksaan antara.

## 1. Saldo awal piutang dan utang

- Perluas draf saldo awal dengan rincian pelanggan/supplier, nomor atau referensi tagihan lama, tanggal dokumen, jatuh tempo, dan sisa yang belum dibayar.
- Sediakan input manual dan template CSV dengan preview, validasi pihak/nominal/tanggal/duplikasi, serta penyimpanan ke draf. Impor tidak langsung memposting saldo.
- Posting membuat rincian tagihan pembukaan dan jurnal kontrol yang cocok dalam satu transaksi. Rincian stok, akun umum, piutang, dan utang harus seimbang sebagai satu pembukaan.
- Tanggal tagihan lama menjadi metadata untuk penelusuran dan aging. Pembukaan memakai kebijakan waktu posting yang jelas; tidak memutar ulang penjualan, HPP, stok, kas, atau pembayaran historis.
- Tagihan pembukaan dapat dibayar/dicicil dan ditelusuri ke jurnal, pihak, lampiran, serta sumber pembukaannya.
- Sertakan sisa uang muka/kredit lama pelanggan dan supplier sebagai sumber saldo pembukaan pada akun kontrol yang sesuai, sehingga kewajiban pengembalian dana lama tidak hilang saat pindah sistem.
- Tetap lindungi instalasi yang sudah bertransaksi dari pembukaan kedua. Pembalikan pembukaan harus ditolak bila rincian sudah dipakai pembayaran, alokasi, atau transaksi operasional lain; koreksi mempertahankan riwayat.

Contoh penerimaan: piutang lama Rp2.000.000 dibayar Rp500.000 menghasilkan sisa Rp1.500.000; utang lama Rp3.000.000 dibayar Rp1.000.000 menghasilkan sisa Rp2.000.000. Pembukaan tidak menjadi pendapatan/beban periode berjalan, dan pembayaran hanya memindahkan kas serta saldo pihak yang terkait.

## 2. Buku pembantu pelanggan dan supplier

- Satu rincian per pihak untuk tagihan pembukaan, invoice/tagihan operasional, pembayaran, retur, uang muka, kredit, pemakaian saldo, refund, dan pembalikan.
- Tampilkan piutang/utang belum selesai, saldo uang muka/kredit yang belum dipakai, serta refund tertunda secara terpisah. Saldo kredit belum dialokasikan tidak boleh diam-diam menyembunyikan tagihan terlambat.
- Filter tanggal/status, tautan dokumen dan jurnal, riwayat alokasi, ekspor CSV, serta tampilan cetak ringkasan transaksi pihak.
- Total buku pembantu harus cocok dengan akun kontrol ledger. Sediakan pemeriksaan selisih yang dapat ditelusuri ke dokumennya.

## 3. Uang muka pelanggan dan supplier

- Catat uang pelanggan yang diterima sebelum invoice dan uang muka yang dibayar kepada supplier sebelum tagihan.
- Kaitkan dengan pihak dan, bila ada, pesanan sumber; simpan referensi, keterangan, bukti, pembuat, serta jurnal.
- Alokasikan sebagian/penuh ke satu atau beberapa tagihan milik pihak yang sama. Tampilkan saldo asal, terpakai, dan tersedia.
- Penerimaan uang muka pelanggan belum merupakan penjualan; uang muka supplier belum merupakan beban atau persediaan. Alokasi ke tagihan tidak mencatat kas kedua kali.
- Sisa uang muka dapat dikembalikan/diminta kembali melalui pencatatan perpindahan kas aktual. Koreksi wajib mengikuti dependensi alokasi dan refund yang sudah terjadi.
- Penerimaan/pembayaran yang melebihi tagihan hanya boleh dilakukan melalui pilihan yang jelas: bagian yang dipakai melunasi tagihan, sisanya menjadi saldo pihak. Jangan diam-diam menganggap seluruh uang sebagai pelunasan satu dokumen.

Contoh penerimaan: uang muka pelanggan Rp1.000.000 dipakai untuk invoice Rp600.000 menyisakan Rp400.000 dan tidak mencatat penerimaan kas baru saat alokasi.

## 4. Kredit lintas dokumen dan refund tertunda

- Retur tetap menghitung kredit sesuai nilai historis dokumen dan aturan stok yang berlaku. Kelebihan pembayaran setelah kredit dapat menjadi saldo kredit pihak untuk dipakai kemudian atau refund tertunda.
- Sediakan pilihan penyelesaian yang eksplisit: kas sudah berpindah, simpan kredit, atau menunggu refund. Jangan mencatat kas keluar/masuk sebelum perpindahan aktual dikonfirmasi.
- Kredit dapat dialokasikan sebagian ke tagihan lain dari pihak yang sama, disertai sumber, jumlah, saldo tersisa, dan riwayat pembalikan.
- Refund dapat dilakukan sebagian/penuh; tampilkan diminta/tersisa/selesai dan riwayat perpindahan kas. Refund pelanggan adalah kas keluar; refund supplier adalah kas masuk.
- Dana yang telah disisihkan untuk permintaan refund tidak dapat dipakai untuk alokasi lain; pembatalan permintaan melepaskan sisihannya. Permintaan yang belum dibayar tidak mengubah kas.
- Uang muka dan kredit retur harus dapat dibedakan asal dan perlakuan jurnalnya. Jangan memakai saldo pihak lain atau menyamakan pelanggan dengan supplier hanya karena namanya sama.
- Pertahankan retur lama yang sudah mencatat refund: upgrade tidak membuat kredit/refund baru untuk dana yang sudah selesai. Koreksi memerlukan pembalikan dependensi yang sesuai.

Contoh penerimaan: tagihan Rp180.000, pembayaran Rp120.000, kredit retur Rp80.000 menghasilkan tagihan bersih Rp100.000 dan kelebihan Rp20.000. Jika refund belum terjadi, kas tidak berubah dan Rp20.000 tetap terlacak sebagai kewajiban/hak penyelesaian. Pemakaian kredit dan refund tidak dapat sama-sama menghabiskan saldo yang sama.

Jika Rp15.000 dari kredit tersebut dipakai pada tagihan lain, sisa yang tersedia untuk refund adalah Rp5.000. Rincian tagihan membedakan pembayaran kas, pemakaian kredit, pengurangan karena retur, dan sisa kewajiban; semuanya tidak boleh diberi label pembayaran kas.

## 5. Aging, chart, dan laporan

- Lengkapi jatuh tempo invoice pelanggan dan metadata tagihan pembukaan. Tanggal yang tidak diketahui pada dokumen lama ditandai sebagai belum diisi; jangan mengarang jatuh tempo historis saat upgrade.
- Umur piutang/utang mencakup tagihan pembukaan serta operasional: belum jatuh tempo, 1–30, 31–60, 61–90, dan lebih dari 90 hari, dengan kelompok tersendiri bila jatuh tempo belum diketahui.
- Laporan per tanggal memakai riwayat kejadian yang berlaku sampai tanggal tersebut menurut zona waktu perusahaan. Pembayaran atau pembalikan setelah tanggal laporan tidak boleh mengubah posisi historis sebelumnya hanya karena status dokumen saat ini sudah berubah.
- Tambahkan chart posisi per pihak, umur tagihan, uang muka/kredit tersedia, dan refund tertunda. Bedakan posisi saat ini dari grafik aktivitas selama rentang tanggal.
- Dashboard, laba rugi, neraca, arus kas, CSV, dan rincian jurnal tetap memakai sumber yang sesuai. Alokasi saldo tidak menghasilkan arus kas; penerimaan/pengembalian dana aktual masuk arus kas sesuai jurnalnya.
- Sediakan rincian yang menjelaskan angka chart dan rekonsiliasi saldo awal + perubahan = saldo akhir, tanpa menjumlahkan kas atau kredit dua kali.

## 6. Riwayat besar dan persiapan operasional

- Tambahkan pagination dan pencarian server pada daftar master/operasional serta riwayat yang masih terpotong. Pertahankan akses ke dokumen lama melalui tautan rinciannya.
- Urutan harus stabil saat beberapa dokumen memiliki waktu yang sama. Perubahan filter mengulang halaman/cursor; ringkasan dan chart tetap menghitung seluruh data yang sesuai.
- Ekspor mengambil seluruh hasil filter yang dipilih, bukan hanya halaman yang sedang terlihat. Terapkan batas ekspor yang jelas bila diperlukan, tanpa pemotongan diam-diam.
- Sediakan dataset dan runner pengukuran yang hanya memakai database sementara. Catat perangkat, versi, jumlah data, jumlah permintaan, akses bersamaan, latensi p50/p95, serta error; angka merupakan hasil pengukuran, bukan SLA.
- Ukur pembacaan daftar/laporan, posting bersamaan, lampiran, dan pemulihan sebagai beban yang berbeda. Uji putus koneksi/retry/restart agar pengguna tidak menerima transaksi ganda.

## 7. Kandidat rilis Community

- Perbarui panduan awal untuk alur saldo awal → pesanan → tagihan → pembayaran/retur → penyelesaian saldo → laporan, beserta dataset demonstrasi yang jelas terpisah dari data usaha.
- Lengkapi catatan perubahan, matriks fitur/batas, panduan upgrade, prosedur backup/restore, serta paket instalasi Docker Compose dengan instruksi singkat.
- Siapkan metadata versi/commit, artefak kandidat rilis, checksum, dan workflow pembuatannya. Verifikasi isi/hash backup dan kecocokan aplikasi/schema sebelum pemulihan; pertahankan prosedur kompatibel untuk backup lama.
- Readiness memeriksa kecocokan migrasi/schema selain koneksi database dan menolak status siap bila upgrade belum diterapkan; migrasi tetap merupakan langkah eksplisit. Informasi versi tidak memaparkan secrets atau konfigurasi koneksi.
- Jalankan instalasi bersih, upgrade dari versi sebelumnya, restart, backup → perubahan → restore, dan pemulihan administrator ketika kuota akun penuh.
- Persiapkan skenario pilot dan lembar hasil pengujian. Pilot usaha nyata baru dapat dinyatakan selesai setelah dijalankan oleh usaha dengan dataset serta hasil yang dicatat. Dataset sintetis tidak membuktikan pilot bisnis selesai.
- Publikasi versi/tag/registry dan installer native memerlukan target distribusi yang konkret. Siapkan hasil yang dapat diperiksa sebelum langkah publikasi tersebut.

## Urutan pengerjaan dan kriteria selesai

Urutan dependensi: kontrak serta ledger saldo pihak → saldo awal piutang/utang → uang muka dan alokasi → kredit/refund tertunda → buku pembantu/chart/laporan → pagination dan pengukuran → kandidat rilis. Pekerjaan UI, dokumentasi, dan pengujian dapat berjalan bersamaan setelah kontraknya jelas.

Setiap checkpoint harus menyertakan migrasi yang aman, API, UI berbahasa Indonesia, hak akses, validasi uang berbasis decimal, audit, koreksi beralasan, dokumentasi, dan pengujian yang relevan. Retry, dua permintaan bersamaan, saldo tidak cukup, periode tertutup, versi lama, dan kegagalan parsial tidak boleh menggandakan transaksi atau merusak ledger.

Pemetaan pembayaran/retur lama ke model penyelesaian baru mempertahankan identitas, referensi, jurnal, jumlah kas, serta tautan pembalikannya. Migrasi tidak memposting ulang dana yang sudah diterima/dibayar. Pembalikan sumber dana menunggu pembalikan alokasi dan penyelesaian refund yang bergantung padanya.

Paket dinyatakan selesai secara teknis setelah:

1. Seluruh skenario angka pada bagian 1, 3, dan 4 cocok antara rincian pihak, dokumen, kas, dan jurnal.
2. Riwayat lama tetap utuh, dan hash/jumlah data historis yang semestinya tidak berubah tetap cocok setelah upgrade.
3. Alokasi bersamaan tidak melampaui saldo sumber atau sisa tagihan; koreksi mengikuti dependensi dan tidak menghapus bukti.
4. Impor tidak valid tidak menyimpan sebagian saldo; commit/posting ulang tidak menggandakan dokumen.
5. Laporan/aging/ekspor mencakup data di luar halaman daftar, serta chart menyediakan rincian sumbernya.
6. CI, smoke container, QA browser, instalasi bersih, upgrade, dan backup/restore mempertahankan semua saldo, lampiran, audit, dan batas akun.
7. Artefak, checksum, panduan, hasil pengukuran, batas fitur, serta hasil pilot yang benar-benar telah dilaksanakan dilaporkan secara terpisah dan jujur.

## Batas paket

Community tetap satu perusahaan, satu gudang logis, IDR, dan 10 akun aktif dengan lisensi AGPL yang berlaku. Pembelian jasa, variasi harga tagihan pembelian, ongkir/diskon/pajak otomatis, banyak gudang, konversi satuan, rekonsiliasi bank, HR/payroll, manufaktur, POS, multi-company/multicurrency, serta integrasi eksternal adalah perluasan lain. Tidak ada janji bahwa seluruh ERP atau kesiapan produksi semua usaha akan selesai melalui satu paket ini.
