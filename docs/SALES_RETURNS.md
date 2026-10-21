# Retur pelanggan

Retur dibuat dari invoice aktif oleh Keuangan atau Administrator. Pilih jumlah
barang atau jasa pada invoice yang dikembalikan, isi alasan, lalu periksa kredit
dan pengembalian kas. Jumlah yang sudah diretur tidak dapat diretur dua kali.

Retur mengurangi pendapatan dan piutang sesuai nilai historis invoice. Barang
masuk kembali ke stok dengan harga pokok pengiriman asalnya, sehingga perubahan
harga beli setelah penjualan tidak mengubah nilai retur. Retur jasa mengurangi
tagihan tanpa pergerakan stok. Pesanan tetap menyimpan status pemenuhan semula;
retur komersial tidak membuka ulang jumlah pesanan untuk dikirim atau ditagih.

Contoh: invoice Rp100.000 sudah dibayar Rp70.000, kemudian pelanggan mengembalikan
barang senilai Rp50.000. Tagihan bersih menjadi Rp50.000. Kelebihan Rp20.000 dapat
direfund langsung, disimpan sebagai kredit pihak, atau disisihkan untuk refund
tertunda. Hanya refund langsung yang mencatat kas bersama retur dan memerlukan
konfirmasi dana aktual serta saldo kas yang cukup. Dua pilihan lainnya belum
mengubah kas; kredit dan penyelesaiannya terlihat pada **Saldo pihak**.

Riwayat invoice menampilkan nilai asli, kredit retur, tagihan bersih, pembayaran
bersih, dan sisa piutang. Bukti retur menyimpan jurnal serta nilai kredit, harga
pokok, dan pengembalian kas secara terpisah. Semua dokumen dan transaksi balik
tetap ada untuk pemeriksaan.

Pembatalan retur memerlukan alasan dan versi invoice terbaru. Untuk barang,
pembatalan hanya tersedia sebelum ada pergerakan stok berikutnya, agar jumlah,
nilai persediaan, dan rata-rata biaya sebelum retur dapat dipulihkan persis.
Invoice dan pembayaran dengan retur aktif memerlukan pembatalan retur terlebih
dahulu sebelum pembatalan dokumen asalnya.

## Batas alur saat ini

- Kredit memakai harga invoice asal, termasuk pembulatan kumulatif. Pemecahan
  nominal yang terlalu kecil ditolak; gabungkan jumlah tersebut dengan sisanya.
- Kredit tersimpan dapat dialokasikan ke invoice atau piutang pembukaan milik
  pelanggan yang sama. Dana refund tertunda disisihkan sampai dibayar sebagian/
  penuh atau permintaannya dibatalkan; tidak dapat dipakai sekaligus untuk
  invoice lain. Lihat [saldo pihak](./PARTY_BALANCES.md).
- Pembalikan retur menunggu pembalikan alokasi/refund kredit dan pembatalan sisa
  permintaan refund. Dokumen asal serta semua jurnal koreksi tetap tersimpan.
- Retur stok memeriksa satuan pengiriman. Satuan katalog yang berubah harus
  dikembalikan ke satuan asal terlebih dahulu. Pengiriman sebelum migrasi 0018
  belum menyimpan satuan historis, sehingga satuannya diisi dari katalog saat
  upgrade; operator perlu memeriksa kesesuaian satuan untuk dokumen lama.
- Pembayaran bank nyata, pajak, ongkir, dan biaya lain di luar invoice tidak
  diproses melalui retur dasar ini.

## API

`GET /api/invoices/{id}` mengembalikan invoice lengkap dengan `version`,
`credit_total`, `net_total`, `outstanding`, `returns`, serta jumlah yang sudah dan
belum diretur pada setiap item.

`POST /api/invoices/{id}/returns` menerima `version`, `reason`,
`items: [{invoice_line_id, quantity}]`, `refund_confirmed`, `refund_mode`, dan
`idempotency_key`. `invoice_line_id` adalah `id` item pada invoice tersebut.

`refund_mode` menerima `immediate` (refund langsung), `credit` (simpan kredit),
atau `deferred` (sisihkan untuk refund). Jika tidak dikirim, nilainya `immediate`
untuk kompatibilitas klien lama. `refund_confirmed` wajib bernilai benar bila
ada refund kas langsung. Nilai `refund` adalah kas yang berpindah saat retur,
sedangkan `settlement_credit` adalah kelebihan yang menjadi saldo kredit pihak.

`GET /api/sales-returns/{id}` menampilkan dokumen retur tersimpan.
`POST /api/sales-returns/{id}/reverse` menerima versi invoice terkini, alasan,
dan kunci idempotensi. Kedua operasi tulis mengembalikan invoice lengkap yang
sudah diperbarui.

Keuangan memiliki akses menulis, sedangkan seluruh pengguna masuk dapat membaca.
Kunci idempotensi, kunci transaksi akuntansi dan invoice, pemeriksaan periode,
serta batas stok/kas menjaga permintaan bersamaan tetap konsisten.
