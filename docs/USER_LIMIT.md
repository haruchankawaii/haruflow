# Batas 10 pengguna aktif Community

Haruflow Community membatasi **10 akun aktif per instalasi/database**, termasuk administrator pertama. Semua peran dihitung: Administrator, Sales, Warehouse, Finance, dan Viewer. Instalasi saat ini memuat satu perusahaan.

Batas ini menghitung akun, bukan sesi, perangkat, tab browser, atau jumlah orang yang sedang online. Satu akun aktif tetap memakai satu slot meskipun belum pernah login. Beberapa sesi dari akun yang sama tidak menambah slot akun.

## Mengelola slot

Buka **Pengaturan → Pengguna** untuk melihat jumlah aktif, batas, dan slot tersisa. Angka memakai seluruh pengguna dan tidak berubah ketika daftar dicari atau difilter.

- Saat aktif kurang dari 10, administrator dapat menambah pengguna atau mengaktifkan kembali pengguna nonaktif.
- Saat aktif mencapai 10, pembuatan akun aktif dan pengaktifan kembali akun nonaktif ditolak.
- Menonaktifkan pengguna melepaskan satu slot. Akun, nama pelaku pada dokumen, dan riwayat audit tetap tersimpan; sesi pengguna tersebut dicabut mengikuti alur pengelolaan akses.
- Mengubah peran, mengganti/reset password, dan mengelola akun aktif tetap mengikuti izin yang ada. Tindakan yang tidak menambah akun aktif tidak memerlukan slot baru.
- Administrator tetap tidak dapat menonaktifkan atau menurunkan perannya sendiri melalui alur pengelolaan pengguna.

Antarmuka membantu mencegah tindakan yang melebihi batas, tetapi keputusan akhir berada di server. Backend memeriksa kembali jumlah aktif dalam transaksi dan database juga memiliki guard. Dua permintaan bersamaan yang memperebutkan slot terakhir tidak boleh sama-sama berhasil. Jika server mengembalikan konflik karena slot sudah dipakai, daftar diperbarui dan formulir tetap terbuka agar pengguna dapat meninjau kembali tindakannya.

## Upgrade instalasi lama

Migrasi `0021_community_user_limit.sql` tidak menghapus atau menonaktifkan akun lama. Jika instalasi sebelum upgrade memiliki lebih dari 10 akun aktif, akun tersebut tetap dapat digunakan dan administrator masih dapat mengelola akses. Penambahan/pengaktifan akun ditolak sampai jumlah aktif turun di bawah 10.

Tinjau pengguna bersama pemilik usaha dan nonaktifkan akun yang tidak diperlukan. Sistem tidak memilih sendiri siapa yang harus kehilangan akses. Backup/restore tetap mempertahankan status akun serta riwayatnya.

## Perlindungan kode dan batas teknis

Batas ini merupakan aturan pada aplikasi Community resmi. Repo tetap memakai lisensi AGPL-3.0 yang tercantum pada `LICENSE`. Tidak ada perubahan lisensi, aktivasi online, panggilan ke server lisensi, atau rahasia antipembajakan baru.

Backend distribusi dibangun sebagai executable Rust release; profil repository sudah memakai LTO dan penghilangan simbol (`strip = true`). Frontend dibangun dan diminifikasi oleh Vite. Ini berbeda dari enkripsi kode yang membuat pemeriksaan batas tidak dapat diubah.

Pemilik instalasi self-hosted mengendalikan executable dan database. Pemilik tersebut tetap dapat memodifikasi instalasinya; pemeriksaan backend/database dan stripping bukan jaminan antipengubahan oleh administrator server. Mengenkripsi kode dengan kunci yang turut dikirim ke mesin yang sama tidak memberikan jaminan tersebut.

Model distribusi tertutup, lisensi bertanda tangan, atau layanan cloud yang dikelola Haruflow memerlukan keputusan produk dan peninjauan lisensi tersendiri. Belum ada fasilitas tersebut dalam perubahan ini.
