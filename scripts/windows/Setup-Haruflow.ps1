[CmdletBinding()]
param([string]$PackageRoot,[string]$InstallRoot=(Join-Path $env:LOCALAPPDATA 'HaruflowCommunity'),[string]$PostgresBin,[ValidateSet('Private','Existing')][string]$DatabaseMode='Private',[string]$DatabaseUrl,[ValidateRange(1024,65535)][int]$Port=8080,[ValidateRange(1024,65535)][int]$PostgresPort=55433,[string]$BackupTime,[switch]$SkipSchedule,[switch]$StartAtLogin,[switch]$NoShortcuts,[switch]$NoStart,[switch]$NonInteractive)
$ErrorActionPreference='Stop';. (Join-Path $PSScriptRoot 'Windows.Common.ps1')
if(-not $PackageRoot){$PackageRoot=$PSScriptRoot};$package=(Resolve-Path -LiteralPath $PackageRoot).Path;$root=Assert-HaruflowPath $InstallRoot
if(-not(Test-Path -LiteralPath (Join-Path $package 'SHA256SUMS'))){throw 'Jalankan setup dari seluruh paket Windows yang sudah diekstrak.'}
$null=& (Join-Path $package 'Install-Haruflow.ps1') -PackageRoot $package -InstallRoot $root -VerifyOnly
if($BackupTime -and $BackupTime -notmatch '^([01][0-9]|2[0-3]):[0-5][0-9]$'){throw 'Jam backup harus HH:mm.'}
if($NonInteractive -and -not $SkipSchedule -and -not $BackupTime){throw 'Pilih -BackupTime HH:mm atau -SkipSchedule. Jam backup tidak ditentukan otomatis.'}
if(-not $NonInteractive){
 Write-Host 'Haruflow Community: satu usaha kecil, IDR, maksimal 10 akun aktif. Percakapan AI tersedia lewat Codex.'
 if(-not(Test-Path -LiteralPath (Join-Path $root 'current.json')) -and -not $PSBoundParameters.ContainsKey('DatabaseMode')){$choice=Read-Host 'Database: 1) privat di folder Haruflow (disarankan), 2) koneksi yang sudah tersedia [1]';if($choice -eq '2'){$DatabaseMode='Existing'}}
 if(-not $PSBoundParameters.ContainsKey('StartAtLogin')){$answer=Read-Host 'Buka layanan Haruflow otomatis saat akun Windows masuk? [Y/n]';$StartAtLogin=$answer -notmatch '^[nN]'}
}
$needsServer=$DatabaseMode -eq 'Private' -and (-not(Test-Path -LiteralPath (Join-Path $root '.env')) -or (Test-Path -LiteralPath (Join-Path $root 'postgres.json')))
$bin=Find-HaruflowPostgres $PostgresBin -Server:$needsServer
Write-Host ('PostgreSQL ditemukan: '+$bin)
if(-not(Test-Path -LiteralPath $root)){[IO.Directory]::CreateDirectory($root) | Out-Null;Protect-HaruflowPath $root -Directory}
$managed=Get-HaruflowManagedPostgres $root
if($managed){
 if($managed.bootstrap_pending -and -not(Test-Path -LiteralPath (Join-Path $root '.env')) -and -not $DatabaseUrl){& (Join-Path $PSScriptRoot 'Manage-Postgres.ps1') -Action Initialize -InstallRoot $root}else{& (Join-Path $PSScriptRoot 'Manage-Postgres.ps1') -Action Start -InstallRoot $root}
 if(-not $DatabaseUrl -and -not(Test-Path -LiteralPath (Join-Path $root '.env'))){$connection=Join-Path $root 'postgres/connection.txt';if(-not(Test-Path -LiteralPath $connection)){throw 'Inisialisasi cluster belum lengkap; periksa postgres/postgres.log dan panduan pemulihan.'};$DatabaseUrl=[IO.File]::ReadAllText($connection)}
}
elseif($DatabaseMode -eq 'Private' -and -not(Test-Path -LiteralPath (Join-Path $root '.env'))){
 $candidate=$PostgresPort;$available=$false
 for($attempt=0;$attempt -lt 20;$attempt++){$listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,$candidate);try{$listener.Start();$available=$true;break}catch{$candidate++}finally{$listener.Stop()}}
 if(-not $available){throw 'Tidak ada port PostgreSQL privat tersedia.'}
 & (Join-Path $PSScriptRoot 'Manage-Postgres.ps1') -Action Initialize -InstallRoot $root -PostgresBin $bin -Port $candidate
 $DatabaseUrl=[IO.File]::ReadAllText((Join-Path $root 'postgres/connection.txt'))
}
if(-not(Test-Path -LiteralPath (Join-Path $root 'current.json')) -and -not $PSBoundParameters.ContainsKey('Port')){
 $available=$false
 for($attempt=0;$attempt -lt 20;$attempt++){$listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,$Port);try{$listener.Start();$available=$true;break}catch{$Port++}finally{$listener.Stop()}}
 if(-not $available){throw 'Tidak ada port aplikasi tersedia; pilih -Port yang bebas sebelum melanjutkan.'}
}
$installArguments=@{PackageRoot=$package;InstallRoot=$root;PostgresBin=$bin;NoShortcuts=$NoShortcuts;NonInteractive=$NonInteractive}
if($DatabaseUrl){$installArguments.DatabaseUrl=$DatabaseUrl};if(-not(Test-Path -LiteralPath (Join-Path $root 'current.json')) -or $PSBoundParameters.ContainsKey('Port')){$installArguments.Port=$Port}
& (Join-Path $package 'Install-Haruflow.ps1') @installArguments
$DatabaseUrl=$null
if($StartAtLogin){& (Join-Path $root 'Schedule-Haruflow.ps1') -Action Startup -InstallRoot $root}elseif(-not $NonInteractive){& (Join-Path $root 'Schedule-Haruflow.ps1') -Action Startup -InstallRoot $root -NoStartAtLogin}
if(-not $SkipSchedule){$schedule=@{Action='Configure';InstallRoot=$root;PostgresBin=$bin};if($BackupTime){$schedule.BackupTime=$BackupTime};& (Join-Path $root 'Schedule-Haruflow.ps1') @schedule}
if(-not $NoStart){& (Join-Path $root 'Start-Haruflow.ps1') -NoBrowser:$NonInteractive}
Write-Output 'Setup selesai. Status-Haruflow.ps1 memeriksa layanan/database/backup; Schedule-Haruflow.ps1 mengubah jadwal. Setup-pertama.txt menyimpan kode awal secara pribadi.'
