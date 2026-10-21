# Saldo pihak, uang muka, dan refund

Halaman **Saldo pihak** menyatukan buku pembantu pelanggan dan supplier. Pilih jenis pihak dan nama pihak, lalu periksa **Buku pembantu dan aging** atau **Uang muka, kredit, dan refund**. Piutang/utang, kredit tersedia, dan dana yang disisihkan untuk refund ditampilkan terpisah. Kredit yang belum dialokasikan tidak otomatis melunasi tagihan atau menyembunyikan keterlambatannya.

Semua pengguna yang masuk dapat membaca. Administrator dan Finance dapat mencatat dana, mengalokasikan saldo, meminta/membatalkan refund, mencatat perpindahan kas, dan membalik transaksi. Setiap operasi tulis memakai pemeriksaan periode terbuka, versi dokumen, transaksi database, audit, dan kunci idempotensi. Bila versi berubah, muat ulang data lalu tinjau kembali.

## Memindahkan saldo lama

Gunakan **Akuntansi → Saldo awal** sebelum transaksi operasional pertama untuk memasukkan sisa piutang, utang, kredit pelanggan, dan kredit supplier. Buat pelanggan/supplier terlebih dahulu. Nomor tagihan lama, tanggal, jatuh tempo, dan sisa nominal disimpan sebagai rincian pembukaan; Haruflow tidak membuat ulang penjualan, pembelian, stok, atau kas masa lalu. Lihat [saldo awal](./OPENING_BALANCES.md) untuk input manual dan CSV.

Tagihan pembukaan dapat dibuka dari buku pembantu. Tombol pencatatan pembayaran membuat sumber dana aktual; setelah disimpan, alokasikan nominalnya ke tagihan pembukaan. Pembayaran sebagian menyisakan tagihan yang belum selesai. Kredit pembukaan memakai alur alokasi/refund yang sama dengan sumber dana lain, tetapi sumbernya tetap dapat ditelusuri ke pembukaan.

## Mencatat uang muka atau kelebihan pembayaran

1. Pada **Saldo pihak**, pilih pelanggan atau supplier yang bersangkutan.
2. Pilih **Catat uang muka**, isi dana yang benar-benar berpindah, referensi bukti, dan keterangan. Pesanan sumber bersifat opsional dan harus milik pihak yang sama.
3. Pastikan dana sudah diterima dari pelanggan atau dibayarkan kepada supplier. Tinjau lalu simpan.
4. Buka sumber dana tersebut untuk memeriksa jurnal, saldo, riwayat, dan menambahkan bukti pada **Bukti sumber dana**.

Uang pelanggan menambah kas serta kewajiban uang muka/kredit pelanggan. Uang muka ke supplier mengurangi kas serta menambah aset uang muka/kredit supplier. Pencatatan ini belum menjadi penjualan, beban, atau persediaan. Pembayaran kepada supplier memerlukan kas yang cukup.

Jika dana melebihi sisa satu tagihan, catat seluruh dana melalui alur ini kemudian alokasikan bagian pelunasannya. Contoh: dana Rp1.000.000 untuk invoice Rp600.000 dicatat sebagai sumber Rp1.000.000, dipakai Rp600.000, dan menyisakan Rp400.000. Jangan memasukkan Rp1.000.000 sebagai pembayaran langsung invoice yang sisanya hanya Rp600.000.

## Memakai saldo untuk tagihan

Pada rincian sumber dana, pilih **Alokasikan ke tagihan**. Isi nominal pada satu atau beberapa tagihan aktif milik pihak yang sama, tinjau totalnya, lalu simpan. Tujuan dapat berupa invoice penjualan, tagihan supplier, piutang pembukaan, atau utang pembukaan sesuai jenis pihak.

Nominal setiap tujuan tidak boleh melebihi sisa tagihannya. Jumlah seluruh alokasi tidak boleh melebihi saldo sumber yang tersedia. Setiap permintaan mendukung 1–100 tujuan yang berbeda. Dana pelanggan tidak dapat dipakai untuk supplier atau pelanggan lain, meskipun namanya sama.

Alokasi memindahkan saldo akun kontrol ke piutang/utang dan **tidak mencatat kas kedua kali**. Rincian invoice/tagihan membedakan pembayaran kas, pemakaian saldo, kredit yang dipindahkan dari retur, dan total penyelesaian. Riwayat alokasi tetap tersimpan saat kemudian dibalik.

## Kredit retur

Retur lebih dahulu mengurangi tagihan sesuai nilai historis sumber. Jika tagihan telah dibayar sehingga timbul kelebihan, pilih salah satu penyelesaian berikut pada retur:

- **Dana sudah dikembalikan**: refund langsung; konfirmasikan perpindahan kas yang benar-benar sudah terjadi.
- **Simpan sebagai kredit**: kelebihan menjadi saldo pihak yang tersedia untuk tagihan berikutnya atau refund kemudian.
- **Menunggu refund**: kelebihan menjadi saldo pihak sekaligus disisihkan untuk refund; belum ada perpindahan kas.

Contoh: tagihan Rp180.000 dibayar Rp120.000 lalu mendapat kredit retur Rp80.000. Tagihan bersih menjadi Rp100.000 dan kelebihan Rp20.000. Menyimpan kredit atau memilih refund tertunda mempertahankan kas pada saat retur. Refund langsung mencatat perpindahan Rp20.000. Dokumen lama yang sudah mencatat refund sebelum upgrade tidak dibuatkan kredit baru untuk uang yang sudah selesai dikembalikan.

Lihat [panduan retur](./RETURNS.md) untuk aturan jumlah, pembulatan, stok, dan pembalikan.

## Menyisihkan dan membayar refund

Pada sumber dana yang masih tersedia, pilih **Sisihkan untuk refund** dan isi nominal serta alasan. Penyisihan mengurangi saldo yang dapat dialokasikan, tetapi tidak mengurangi kas. Satu dana tidak dapat sekaligus dipakai untuk tagihan dan dijanjikan pada refund yang sama.

Di riwayat permintaan refund:

- **Catat refund sebagian / penuh** mencatat dana aktual sebagian atau seluruh sisa permintaan. Pelanggan menerima kas keluar; refund dari supplier menjadi kas masuk. Konfirmasi hanya setelah dana benar-benar berpindah.
- **Batalkan sisa permintaan** melepaskan sisa yang belum dibayar. Pembatalan tidak menghapus pembayaran refund yang sudah terjadi atau mengubah kasnya.

**Catat refund aktual** pada sumber dana dipakai untuk pengembalian langsung dari saldo yang belum disisihkan. Untuk dana yang sudah disisihkan, catat melalui permintaan refund terkait. Nominal tidak boleh melebihi anggaran yang tersedia. Refund kepada pelanggan memerlukan kas yang cukup. Aplikasi mencatat perpindahan uang; transfer bank dilakukan di luar aplikasi.

Saldo tersedia = nilai asal − alokasi aktif − refund aktual aktif − sisa refund yang masih disisihkan. Sumber dana yang dibalik tidak dapat dipakai lagi.

## Buku pembantu dan chart

Pilih **Posisi saldo per tanggal** untuk membaca saldo pada akhir tanggal tersebut menurut zona waktu perusahaan. **Aktivitas dari** membatasi baris aktivitas yang ditampilkan, sementara saldo awal rentang dan posisi akhirnya tetap memperhitungkan aktivitas sebelumnya. Pembayaran, alokasi, retur, dan pembalikan yang terjadi sesudah tanggal posisi tidak mengubah saldo historis sebelumnya.

Tanggal tagihan lama dipakai untuk penelusuran dan umur tagihan; saldo baru mulai masuk pembukuan Haruflow pada waktu posting pembukaan. Melihat posisi sebelum pembukaan tidak memutar ulang saldo tersebut ke tahun lama.

Chart memperlihatkan sisa tagihan, kredit tersedia, dan refund yang disisihkan. Kelompok umur mencakup belum jatuh tempo, terlambat 1–30, 31–60, 61–90, lebih dari 90 hari, serta jatuh tempo belum diketahui. Rincian tagihan dan aktivitas membantu menelusuri angka chart ke dokumen dan jurnalnya.

Jatuh tempo invoice baru mengikuti tanggal invoice dan jangka pembayaran pelanggan, dengan nilai awal 0 hari. Administrator/Finance dapat mengubah jangka pembayaran pelanggan untuk invoice berikutnya atau mengoreksi tanggal/jatuh tempo invoice tertentu dengan alasan; perubahan tanggal tidak membuat jurnal keuangan baru. Invoice lama yang tidak mempunyai tanggal tetap ditandai belum diketahui sampai dilengkapi, bukan diberi tanggal perkiraan saat upgrade.

Bagian rekonsiliasi membandingkan total seluruh buku pembantu pelanggan/supplier dengan saldo akun kontrol ledger pada tanggal posisi. Status cocok berarti kedua pasangan saldo sama: piutang/utang dan uang muka/kredit. Angka tersebut membandingkan seluruh pihak pada jenis yang dipilih, bukan hanya satu pihak yang sedang dibuka.

Ekspor CSV buku pembantu dan cetak ringkasan memakai tanggal posisi serta rentang aktivitas yang dipilih. Periksa judul filter ketika membagikannya. Lampiran dapat ditambahkan pada sumber dana, refund aktual, dan rincian saldo/tagihan pembukaan sesuai [panduan lampiran](./ATTACHMENTS.md).

## Koreksi tanpa menghapus riwayat

Gunakan tindakan pembalikan pada sumber dana, alokasi, atau refund yang salah dan berikan alasan. Pembalikan membuat jurnal lawan; catatan asal tetap ada. Pembalikan sumber kredit retur/pembukaan dilakukan melalui dokumen sumbernya.

Beresi dependensi terlebih dahulu: balikkan alokasi/refund aktif dan batalkan sisa permintaan refund sebelum membalik sumber dananya. Retur yang bergantung pada penyelesaian tagihan dapat menghalangi pembalikan alokasi. Pembalikan refund supplier atau uang muka pelanggan memerlukan kas cukup karena menghasilkan kas keluar; pastikan pencatatan koreksi sesuai perpindahan uang sebenarnya.

Kredit pembukaan mengikuti aturan pembalikan pembukaan. Pembukaan tidak dapat diulang untuk mengganti saldo setelah usaha sudah bertransaksi. Periode tertutup, sumber yang sudah dibalik, versi lama, dan pemakaian silang pihak ditolak. Permintaan yang gagal tidak menyimpan sebagian kas, jurnal, atau alokasi.

## Contoh pemeriksaan

Pelanggan memberi DP Rp1.000.000. Pakai Rp600.000 untuk invoice pertama dan Rp250.000 untuk invoice kedua. Saldo dana tersisa Rp150.000. Sisihkan lalu refund Rp150.000 ketika uang sudah dikirim. Akhirnya alokasi Rp850.000, refund Rp150.000, saldo tersedia nol. Penerimaan kas hanya terjadi satu kali pada DP; kedua alokasi tidak menambah kas.

Community menggunakan satu perusahaan, satu gudang logis, IDR, dan maksimal 10 akun aktif. Fitur ini tidak mencakup transfer antar pihak, konversi mata uang, transfer bank otomatis, rekonsiliasi bank, ataupun posting historis dengan tanggal jurnal yang dimundurkan.
