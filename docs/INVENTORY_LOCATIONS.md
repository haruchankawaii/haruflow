# Gudang, lot, seri dan reservasi

Buka Inventori → Gudang, lot & reservasi. Buat gudang tambahan hanya jika diperlukan. Gudang utama otomatis menerima seluruh stok lama saat upgrade; jumlah, nilai, dan jurnal asal tetap tersimpan. Riwayat lokasi dimulai dengan penanda migrasi, tanpa lot atau serial historis rekaan.

Posisi menampilkan stok, reservasi aktif, stok bebas, nilai, dan versi. Transfer memilih lokasi sumber/tujuan dan jumlah bebas; transfer tidak memposting jurnal persediaan baru. Nilai lokasi direkonsiliasi ke biaya rata-rata gabungan produk, sehingga nilai lokasi dapat berubah ketika penerimaan/penjualan mengubah rata-rata produk.

Produk dapat memakai pelacakan none/lot/serial serta kewajiban kedaluwarsa. Bersihkan atau tetapkan lot untuk stok tanpa identitas sebelum mengaktifkan pelacakan ketat. Buat lot/seri sebelum penerimaan. Seri harus satu unit per identitas; pengeluaran otomatis menggunakan lot belum kedaluwarsa dengan tanggal terdekat (FEFO). Barang kedaluwarsa ditangani sebagai penyesuaian beralasan, bukan penjualan otomatis.

Penerimaan, saldo awal, penyesuaian, pengiriman bertahap, dan retur memiliki pilihan gudang/lot/seri. Tanpa pilihan, gudang utama dan aturan lot otomatis berlaku. Retur diterima dapat memilih lokasi masuk. Pembalikan pengiriman menerima kembali jumlah/biaya historis ke lokasi asli dan menghitung ulang biaya rata-rata, termasuk bila ada pengiriman berikutnya. Aturan seri, pelacakan dan saldo tetap berlaku. Pembalikan penerimaan/retur/penyesuaian mengikuti snapshot/dependensi sumber dan ditolak bila aktivitas berikutnya membuat pemulihan persis tidak aman.

Konversi satuan memakai pembilang/penyebut eksak, misalnya dus → pcs = 12/1. Konversi tersedia pada penerimaan lokasi dan transfer; jumlah PO/SO/form transaksi lama tetap memakai satuan dasar produk. Hasil konversi yang tidak dapat disimpan tepat dalam tiga desimal ditolak.

Reservasi mengunci stok bebas per lokasi/lot. Reservasi yang dikaitkan pesanan hanya dapat dikonsumsi oleh pengiriman pesanan tersebut. Pengiriman lain tidak boleh mengambil stok yang dicadangkan. Reservasi bisa dilepas dengan alasan, atau berakhir pada waktu yang ditentukan; riwayat konsumsi/pelepasan tetap disimpan.
