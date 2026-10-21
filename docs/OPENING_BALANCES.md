# Saldo awal Community

Saldo awal adalah posisi kas, persediaan, aset lain, kewajiban, dan modal ketika perusahaan mulai mencatat transaksi di Haruflow. Proses ini tersedia pada **Akuntansi → Saldo awal** untuk Administrator dan Finance. Pengguna lain dapat membaca hasilnya.

## Sebelum mulai

- Buat produk barang, pelanggan, supplier, dan akun yang diperlukan terlebih dahulu. Produk jasa tidak memiliki saldo persediaan.
- Gunakan proses ini sebelum ada jurnal atau pergerakan stok. Membuat pelanggan, produk, akun, dan draf pesanan tidak menambah saldo.
- Instalasi yang sudah mencatat penerimaan stok, invoice jasa, jurnal manual, atau transaksi lain tidak dapat menambahkan saldo awal lagi. Pembatasan ini mencegah penggandaan saldo yang sudah dibukukan; upgrade tidak mengubah transaksi lama.
- Ini satu konteks pembukaan perusahaan, bukan proses mengulang saldo setiap tahun. Posting memakai waktu saat ini dan zona waktu perusahaan, tanpa backdating, serta mengikuti penutupan periode.

## Contoh draf yang seimbang

Misalnya kas awal Rp100.000 dan persediaan 10 unit dengan biaya Rp50.000 per unit:

1. Isi keterangan pembukaan.
2. Tambahkan barang sebanyak 10 unit, biaya per unit `50000`. Nilai persediaan Rp500.000 otomatis masuk debit akun `1300` (Inventory).
3. Pada saldo akun, isi debit `100000` dan kredit `0` untuk akun `1100` (Cash).
4. Isi debit `0` dan kredit `600000` untuk akun `3000` (Opening equity).
5. Simpan draf, lalu periksa: total debit dan kredit masing-masing Rp600.000. Draf boleh belum seimbang; menyimpan draf belum mengubah stok atau laporan.
6. Setelah rinciannya benar dan seimbang, tinjau dan posting draf tersimpan. Haruflow memperbarui stok, biaya rata-rata, jurnal, dan audit dalam satu transaksi.

Jangan memasukkan akun persediaan `1300` lagi pada saldo akun; nilainya berasal dari rincian barang. Akun piutang `1200`, utang supplier, uang muka/kredit pelanggan, serta uang muka/kredit supplier berasal dari rincian pihak. Akun kontrol **barang diterima belum ditagih** tidak dapat diisi langsung pada pembukaan. Sistem memilih kode kosong 2110–2199 untuk penampung penerimaan dan 2200–2299 untuk utang supplier, dengan cadangan kode sepuluh digit kosong bila rentang terkait penuh; akun pengguna tetap utuh. Akun pendapatan dan beban tidak digunakan pada proses saldo awal ini.

Jumlah barang mendukung tiga angka desimal, biaya satuan empat, dan nominal akun dua. Gunakan titik untuk desimal, tanpa pemisah ribuan. Nilai setiap baris stok dibulatkan ke dua desimal sebelum dijumlahkan. Draf menolak barang atau akun yang sama dua kali dan mendukung maksimal 100 baris barang serta 100 baris saldo akun.

## Piutang, utang, dan saldo kredit lama

Bagian **Piutang, utang, dan saldo kredit lama** menyimpan sisa saldo per pihak dan referensi dokumen. Pilih salah satu jenis:

- **Piutang pelanggan** (`receivable`): sisa invoice lama yang belum diterima, masuk debit piutang.
- **Utang supplier** (`payable`): sisa tagihan lama yang belum dibayar, masuk kredit utang supplier.
- **Kredit pelanggan** (`customer_credit`): sisa DP atau kredit yang menjadi kewajiban kepada pelanggan, masuk kredit akun uang muka/kredit pelanggan.
- **Kredit supplier** (`supplier_credit`): sisa uang muka/kredit pada supplier, masuk debit akun uang muka/kredit supplier.

Isi pihak, referensi, tanggal dokumen, jatuh tempo opsional, dan sisa nominal positif. Tanggal dokumen tidak boleh di masa depan; jatuh tempo tidak boleh mendahului tanggal dokumen. Gunakan sisa yang belum selesai, bukan total awal berikut pembayaran lama. Pembayaran lama tidak perlu dicatat ulang.

Contoh: piutang Rp2.000.000 dan utang Rp3.000.000 memerlukan saldo akun lain yang menyeimbangkan debit/kredit keseluruhan. Bila hanya dua saldo tersebut yang dibawa, debit modal Rp1.000.000 menyeimbangkannya. Gunakan posisi keuangan usaha yang sebenarnya ketika menggabungkan kas, stok, modal, serta saldo lainnya. Draf tetap harus seimbang sebelum posting.

Maksimal 1.000 rincian pihak dalam satu draf; referensi yang sama untuk jenis saldo dan pihak yang sama ditolak tanpa membedakan huruf besar/kecil. Nominal memakai dua desimal dan total jurnal per sisi maksimal Rp1 triliun, termasuk seluruh rincian lainnya. Akun kontrol terkait dihasilkan otomatis dari rincian pihak, sehingga tidak diisi lagi sebagai saldo akun umum.

### Impor CSV ke draf

Unduh template pada bagian rincian pihak. Pilih pemisah koma atau titik koma sesuai file, lalu gunakan kolom berikut:

`kind,party_id,party,reference,document_date,due_date,debit,credit`

`kind` memakai empat nilai teknis di atas. Piutang dan kredit supplier memakai debit positif; utang dan kredit pelanggan memakai kredit positif. Sisi lainnya nol. Tanggal memakai `YYYY-MM-DD`, nominal memakai titik desimal tanpa pemisah ribuan. `party_id` dapat diisi UUID pihak; jika tidak diisi, `party` memakai nama pelanggan atau kode supplier. Nama ambigu harus diselesaikan dengan identitas yang tepat, bukan dipilih otomatis.

CSV maksimal 256 KiB dan 1.000 baris data. **Preview** memeriksa semua baris. Bila ada kesalahan, perbaiki file dan ulangi; jangan menganggap baris yang tampak benar sudah diposting. Setelah preview valid, gabungkan ke draf dan simpan. Penggabungan juga memeriksa duplikasi terhadap rincian yang sudah ada. Preview, penggabungan, dan menyimpan draf tidak mengubah kas, persediaan, atau jurnal. Posting dilakukan terpisah setelah memeriksa keseimbangan keseluruhan.

### Menyelesaikan tagihan lama

Setelah posting, rincian piutang/utang muncul pada **Saldo pihak → Buku pembantu dan aging**. Buka tagihan pembukaan, catat dana pembayaran yang benar-benar berpindah, lalu alokasikan dana tersebut ke sisa tagihan. Pembayaran penuh/cicilan mengurangi piutang atau utang tanpa membuat penjualan/pembelian lama kembali. Kredit pembukaan dapat dialokasikan ke tagihan pihak yang sama atau diselesaikan melalui refund. Lihat [saldo pihak](./PARTY_BALANCES.md).

Tanggal dokumen lama dipakai untuk umur tagihan, sedangkan jurnal pembukaan tetap memakai waktu posting saat ini. Laporan sebelum waktu pembukaan tidak memuat saldo ini sebagai transaksi masa lalu. Bukti tagihan lama dapat dilampirkan pada rincian saldo pembukaan.

## Setelah posting dan koreksi

Saldo awal yang sudah diposting tampil di jurnal umum, buku besar, neraca saldo, kartu stok, dan saldo kas pada dashboard. Mengulang permintaan posting yang sama tidak menggandakan saldo. Jika pengguna lain telah mengubah draf, muat versi terbaru sebelum meninjau ulang.

Jika belum ada jurnal atau pergerakan stok operasional setelah pembukaan, gunakan **Balikkan saldo awal** dengan alasan. Pembalikan membuat jurnal dan pergerakan stok lawan; catatan awal tetap tersimpan. Dokumen kembali menjadi draf agar dapat diperbaiki dan diposting kembali, dengan riwayat setiap posting dan pembalikannya.

Pembalikan juga ditolak bila tagihan/kredit pembukaan sudah dipakai alokasi, refund, atau permintaan refund aktif. Beresi transaksi turunannya terlebih dahulu. Setelah transaksi operasional dimulai, pembalikan saldo awal tetap mengikuti pemeriksaan riwayat jurnal/stok dan dapat ditolak walaupun transaksi lanjutan sudah dibalik. Koreksi harus mengikuti transaksi yang terkait dan periode yang masih terbuka. Jangan membuat pembukaan kedua atau mengubah ledger melalui SQL untuk menutupi selisih.
