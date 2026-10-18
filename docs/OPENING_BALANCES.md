# Saldo awal Community

Saldo awal adalah posisi kas, persediaan, aset lain, kewajiban, dan modal ketika perusahaan mulai mencatat transaksi di Haruflow. Proses ini tersedia pada **Akuntansi → Saldo awal** untuk Administrator dan Finance. Pengguna lain dapat membaca hasilnya.

## Sebelum mulai

- Buat produk barang dan akun yang diperlukan terlebih dahulu. Produk jasa tidak memiliki saldo persediaan.
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

Jangan memasukkan akun persediaan `1300` lagi pada saldo akun; nilainya berasal dari rincian barang. Akun piutang `1200` juga tidak dapat diisi langsung karena harus cocok dengan rincian tagihan pelanggan. Pembukaan piutang per pelanggan/tagihan belum tersedia. Akun pendapatan dan beban tidak digunakan pada proses saldo awal ini.

Jumlah barang mendukung tiga angka desimal, biaya satuan empat, dan nominal akun dua. Gunakan titik untuk desimal, tanpa pemisah ribuan. Nilai setiap baris stok dibulatkan ke dua desimal sebelum dijumlahkan. Draf menolak barang atau akun yang sama dua kali dan mendukung maksimal 100 baris barang serta 100 baris saldo akun.

## Setelah posting dan koreksi

Saldo awal yang sudah diposting tampil di jurnal umum, buku besar, neraca saldo, kartu stok, dan saldo kas pada dashboard. Mengulang permintaan posting yang sama tidak menggandakan saldo. Jika pengguna lain telah mengubah draf, muat versi terbaru sebelum meninjau ulang.

Jika belum ada jurnal atau pergerakan stok operasional setelah pembukaan, gunakan **Balikkan saldo awal** dengan alasan. Pembalikan membuat jurnal dan pergerakan stok lawan; catatan awal tetap tersimpan. Dokumen kembali menjadi draf agar dapat diperbaiki dan diposting kembali, dengan riwayat setiap posting dan pembalikannya.

Setelah transaksi operasional dimulai, pembalikan saldo awal ditolak. Koreksi harus mengikuti transaksi yang terkait dan periode yang masih terbuka. Jangan membuat pembukaan kedua atau mengubah ledger melalui SQL untuk menutupi selisih.
