# Pengiriman dan invoice bertahap

Pesanan menyimpan jumlah yang disepakati. Setiap pengiriman dan invoice menyimpan
alokasi tersendiri; jumlah sisanya tetap tersedia pada pesanan yang sama.

Contoh: pesanan 10 unit dikirim 4 unit hari ini. Gudang masih harus mengirim 6 unit.
Keuangan dapat menagih 2 dari 4 unit tersebut, sehingga ada 2 unit siap ditagih dan
6 unit masih menunggu pengiriman. Pengiriman selanjutnya tidak memerlukan pesanan
baru. Jasa dapat ditagih setelah konfirmasi tanpa pengiriman stok.

Pada **Penjualan → Pesanan → Detail**, kolom Dikirim, Sisa kirim, Ditagihkan, dan
Siap ditagih menampilkan kemajuan tiap produk. Gunakan **Kirim sebagian / sisa**
atau **Tagihkan sebagian / sisa**, lalu masukkan jumlah yang diproses. Nilai 0
melewati produk tersebut. Riwayat pengiriman dan invoice tetap ditampilkan,
termasuk dokumen yang dibatalkan.

Invoice dan hasil cetaknya berisi jumlah yang dialokasikan ke invoice itu saja.
Pembulatan uang dihitung kumulatif terhadap total baris pesanan, sehingga seluruh
invoice aktif yang menagih seluruh pesanan berjumlah tepat sama dengan total
pesanan. Harga pokok memakai nilai persediaan saat tiap pengiriman; pengiriman
terakhir yang menghabiskan stok memakai seluruh sisa nilai persediaan.

Pengiriman dapat dibatalkan per dokumen setelah invoice aktif yang memakai
pengiriman tersebut dibatalkan. Invoice dengan pembayaran aktif memerlukan
pembatalan pembayaran terlebih dahulu. Pembatalan mencatat transaksi balik dan
memulihkan jumlah yang tersedia; dokumen asli tidak dihapus. Pengembalian komersial
pelanggan memakai alur retur tersendiri dan tidak membuka ulang kewajiban pesanan.

## API

- `POST /api/orders/{id}/ship` dan `POST /api/orders/{id}/invoice` menerima
  `items: [{line_id, quantity}]`, `version` pesanan, serta `idempotency_key`.
- Jika `items` tidak dikirim, server memilih seluruh jumlah tersisa yang tersedia.
  Permintaan penuh pada pesanan campuran tetap memerlukan pengiriman barang
  terlebih dahulu; pemilihan baris jasa secara eksplisit dapat dilakukan mandiri.
- `POST /api/orders/{id}/shipments/{shipment_id}/reverse` membalik pengiriman
  tertentu dengan `reason`, opsional `version`, dan `idempotency_key`.
- Endpoint lama `POST /api/orders/{id}/reverse-shipment` memilih pengiriman aktif
  terbaru dan menjalankan aturan alokasi yang sama.
- Detail pesanan menyertakan `version`, `can_ship`, `can_invoice`, kemajuan per
  baris, `shipments`, dan `invoices`. Daftar/detail invoice menyertakan `items`
  dari alokasi invoice, bukan seluruh pesanan.

Nominal dan jumlah menggunakan string desimal. Perubahan yang bersamaan dikunci
pada pesanan, sedangkan stok dikunci dengan urutan produk yang tetap. Server
menolak pengiriman berlebih, penagihan atas barang yang belum dikirim, versi lama,
periode tutup, dan penggunaan ulang kunci untuk permintaan yang berbeda.

Migrasi 0014 mengisi alokasi untuk dokumen penuh yang sudah ada tanpa mengubah
stok, jurnal, pembayaran, nomor dokumen, atau status dokumen lama.
