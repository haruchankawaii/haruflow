[CmdletBinding()]
param(
    [string]$PackageRoot = $PSScriptRoot,
    [string]$InstallRoot = (Join-Path $env:LOCALAPPDATA 'HaruflowCommunity'),
    [string]$DatabaseUrl,
    [string]$PostgresBin,
    [ValidateRange(1024,65535)][int]$Port = 8080,
    [switch]$NoShortcuts,
    [switch]$Start,
    [switch]$NonInteractive
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Installer ini untuk Windows. Gunakan panduan Docker untuk sistem lain.' }
$package = (Resolve-Path -LiteralPath $PackageRoot).Path
$install = [IO.Path]::GetFullPath($InstallRoot)
if ($install -eq [IO.Path]::GetPathRoot($install) -or $install.StartsWith($package + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or $install -eq $package) { throw 'Pilih folder instalasi terpisah dari paket dan root drive.' }
function Assert-Child([string]$Parent,[string]$Relative) {
    if ([IO.Path]::IsPathRooted($Relative) -or $Relative -match '(^|[\\/])\.\.([\\/]|$)' -or $Relative.Contains(':')) { throw 'Path paket tidak aman.' }
    $resolved = [IO.Path]::GetFullPath((Join-Path $Parent $Relative))
    if (-not $resolved.StartsWith($Parent.TrimEnd('\','/')+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Path di luar folder paket.' }
    return $resolved
}
function Verify-Package([string]$Root) {
    $manifest=Join-Path $Root 'SHA256SUMS'
    if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) { throw 'SHA256SUMS tidak ditemukan. Gunakan paket resmi yang utuh.' }
    $known=@{}
    foreach ($line in [IO.File]::ReadAllLines($manifest)) {
        if ($line -notmatch '^([a-f0-9]{64})  (.+)$') { throw 'Format SHA256SUMS tidak valid.' }
        $expected=$Matches[1]; $name=$Matches[2]
        if ($known.ContainsKey($name)) { throw 'Entri checksum ganda.' }
        $known[$name]=$true; $path=Assert-Child $Root $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Item -LiteralPath $path).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "File paket hilang/tidak aman: $name" }
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw "Checksum berbeda: $name. Instalasi dibatalkan." }
    }
    foreach ($file in Get-ChildItem -LiteralPath $Root -File -Recurse -Force) {
        $relative=$file.FullName.Substring($Root.Length+1).Replace('\','/')
        if ($relative -ne 'SHA256SUMS' -and -not $known.ContainsKey($relative)) { throw "File di luar manifest: $relative" }
    }
    return $known.Keys.Count
}
function Private-File([string]$Path) {
    # A fresh discretionary descriptor avoids copying audit/SACL sections that
    # would require administrator-only SeSecurityPrivilege on a later upgrade.
    $acl=[Security.AccessControl.FileSecurity]::new()
    $acl.SetAccessRuleProtection($true,$false)
    $current=[Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl.SetOwner($current)
    $acl.SetAccessRule([Security.AccessControl.FileSystemAccessRule]::new($current,'FullControl','Allow'))
    $acl.SetAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-5-18'),'FullControl','Allow'))
    if ($PSVersionTable.PSEdition -eq 'Core') {
        [IO.FileSystemAclExtensions]::SetAccessControl([IO.FileInfo]::new($Path),$acl)
    } else {
        ([IO.FileInfo]::new($Path)).SetAccessControl($acl)
    }
}
function Pg-Tool([string]$Name) {
    if ($PostgresBin) { $candidate=Join-Path $PostgresBin ($Name+'.exe'); if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate } }
    $command=Get-Command ($Name+'.exe') -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    throw "Utilitas $Name tidak ditemukan. Isi -PostgresBin dengan folder bin PostgreSQL yang terpasang."
}
$savedEnvironment=@{}
foreach ($name in @('PGHOST','PGPORT','PGUSER','PGPASSWORD','PGDATABASE','PGCONNECT_TIMEOUT','DATABASE_URL')) {$savedEnvironment[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
try {
$verified=Verify-Package $package
$build=Get-Content -LiteralPath (Join-Path $package 'BUILD.json') -Raw | ConvertFrom-Json
if ($build.application -ne 'Haruflow Community' -or $build.platform -ne 'win32' -or $build.commit -notmatch '^[a-f0-9]{40}$') { throw 'Identitas paket Windows tidak valid.' }
$config=Join-Path $install '.env'; $currentFile=Join-Path $install 'current.json'
$previous=$null
if (Test-Path -LiteralPath $currentFile) { $previous=Get-Content -LiteralPath $currentFile -Raw | ConvertFrom-Json }
if ($previous -and -not $PSBoundParameters.ContainsKey('Port')) {$Port=[int]$previous.port}
if (-not $DatabaseUrl -and (Test-Path -LiteralPath $config)) {
    foreach ($line in [IO.File]::ReadAllLines($config)) { if ($line.StartsWith('DATABASE_URL=')) { $DatabaseUrl=$line.Substring(13).Trim('"') } }
}
if (-not $DatabaseUrl) {
    if ($NonInteractive) { throw 'DatabaseUrl wajib diisi pada instalasi noninteraktif.' }
    Write-Host 'Satu perusahaan, mata uang IDR, maksimal 10 akun aktif. PostgreSQL harus sudah tersedia.'
    Write-Host 'Masukkan koneksi PostgreSQL untuk database Haruflow yang telah disiapkan.'
    $secret=Read-Host 'Koneksi database (input disembunyikan)' -AsSecureString
    $pointer=[Runtime.InteropServices.Marshal]::SecureStringToBSTR($secret)
    try { $DatabaseUrl=[Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer) } finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
}
if ($DatabaseUrl -match '[\r\n\x00]') {throw 'Koneksi database mengandung karakter tidak valid.'}
$dbUri=[Uri]$DatabaseUrl
if ($dbUri.Scheme -notin @('postgres','postgresql') -or -not $dbUri.Host -or $dbUri.AbsolutePath -eq '/') { throw 'Koneksi harus postgres://pengguna:password@host:port/database.' }
$psql=Pg-Tool 'psql'; $dump=Pg-Tool 'pg_dump'; $restore=Pg-Tool 'pg_restore'
$env:PGHOST=$dbUri.Host; $env:PGPORT=if ($dbUri.Port -gt 0) { [string]$dbUri.Port } else { '5432' }
$credentials=$dbUri.UserInfo.Split(':',2); $env:PGUSER=[Uri]::UnescapeDataString($credentials[0]); $env:PGPASSWORD=if ($credentials.Length -gt 1) {[Uri]::UnescapeDataString($credentials[1])} else {''}; $env:PGDATABASE=[Uri]::UnescapeDataString($dbUri.AbsolutePath.TrimStart('/')); $env:PGCONNECT_TIMEOUT='5'
$present=& $psql -X -Atq -v ON_ERROR_STOP=1 -c "SELECT to_regclass('public._sqlx_migrations') IS NOT NULL;"
if ($LASTEXITCODE -ne 0) { throw 'Koneksi database gagal. Periksa PostgreSQL dan koneksinya.' }
if ($present -eq 'f') {
    $tables=& $psql -X -Atq -v ON_ERROR_STOP=1 -c "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE';"
    if ($LASTEXITCODE -ne 0 -or $tables -ne '0') { throw 'Database berisi tabel lain. Pilih database Haruflow atau database kosong khusus Haruflow.' }
}
New-Item -ItemType Directory -Path $install -Force | Out-Null
$versions=Join-Path $install 'versions'; New-Item -ItemType Directory -Path $versions -Force | Out-Null
$versionPath=Assert-Child $install ('versions/'+$build.commit)
if (Test-Path -LiteralPath $versionPath) { Verify-Package $versionPath | Out-Null } else {
    New-Item -ItemType Directory -Path $versionPath | Out-Null
    foreach ($item in Get-ChildItem -LiteralPath $package -Force) { Copy-Item -LiteralPath $item.FullName -Destination $versionPath -Recurse }
    Verify-Package $versionPath | Out-Null
}
$executable=Join-Path $versionPath 'haruflow.exe'; $identity=& $executable version | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or $identity.commit -ne $build.commit -or $identity.schema_version -ne $build.schema_version) { throw 'Identitas executable berbeda dari paket.' }
if ($previous) {
    $pidFile=Join-Path $install 'server.pid'
    if (Test-Path -LiteralPath $pidFile) {
        $owned=Get-Process -Id ([int](Get-Content -LiteralPath $pidFile)) -ErrorAction SilentlyContinue
        if ($owned) {
            $expected=Assert-Child $install ('versions/'+$previous.commit+'/haruflow.exe')
            if ($owned.Path -ne $expected) { throw 'PID milik proses lain. Hentikan server Haruflow yang benar sebelum upgrade.' }
            $owned.Kill(); if (-not $owned.WaitForExit(10000)) { throw 'Server belum berhenti.' }
        }
    }
}
$backup=$null
if ($present -eq 't') {
    $backupDir=Join-Path $install 'backups'; New-Item -ItemType Directory -Path $backupDir -Force | Out-Null
    $backup=Join-Path $backupDir ('before-upgrade-'+[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')+'-'+[Guid]::NewGuid().ToString('N')+'.backup')
    & $dump -Fc -f $backup
    if ($LASTEXITCODE -ne 0) { throw 'Backup gagal; database belum dimigrasikan.' }
    & $restore --list $backup | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Arsip backup tidak dapat dibaca; database belum dimigrasikan.' }
    Private-File $backup
    @{format='haruflow-windows-pre-upgrade-v1';sha256=(Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $backup).Length;created_at=[DateTime]::UtcNow.ToString('o');previous_commit=$previous.commit;candidate_commit=$build.commit} | ConvertTo-Json | Set-Content -LiteralPath ($backup+'.json') -Encoding UTF8
    Private-File ($backup+'.json')
}
$env:DATABASE_URL=$DatabaseUrl
Push-Location -LiteralPath $install
try {
    & $executable migrate
    if ($LASTEXITCODE -ne 0) { throw "Migrasi gagal. Server tetap berhenti. Pulihkan backup yang diverifikasi sebelum memakai versi sebelumnya: $backup" }
} finally { Pop-Location }
$tokenBytes=New-Object byte[] 32; $rng=[Security.Cryptography.RandomNumberGenerator]::Create(); try {$rng.GetBytes($tokenBytes)} finally {$rng.Dispose()}; $token=($tokenBytes | ForEach-Object {$_.ToString('x2')}) -join ''
if (Test-Path -LiteralPath $config) { foreach ($line in [IO.File]::ReadAllLines($config)) { if ($line.StartsWith('HARUFLOW_SETUP_TOKEN=')) { $token=$line.Substring(21).Trim('"') } } }
$safeUrl=$DatabaseUrl.Replace('\','\\').Replace('"','\"'); $static=(Join-Path $versionPath 'web').Replace('\','/')
if (-not (Test-Path -LiteralPath $config)) {[IO.File]::WriteAllText($config,'')}; Private-File $config
[IO.File]::WriteAllText($config,"DATABASE_URL=`"$safeUrl`"`nHARUFLOW_SETUP_TOKEN=$token`nHARUFLOW_BIND=127.0.0.1:$Port`nHARUFLOW_STATIC_DIR=`"$static`"`nHARUFLOW_SECURE_COOKIES=false`n",[Text.UTF8Encoding]::new($false))
$setupGuide=Join-Path $install 'Setup-pertama.txt'
if (-not (Test-Path -LiteralPath $setupGuide)) {[IO.File]::WriteAllText($setupGuide,'')}; Private-File $setupGuide
[IO.File]::WriteAllText($setupGuide,"Buka http://127.0.0.1:$Port lalu buat perusahaan dan administrator.`r`nKode setup: $token`r`nSimpan kode ini secara pribadi; jangan kirim file .env kepada orang lain.`r`n",[Text.UTF8Encoding]::new($false))
@{commit=$build.commit;schema_version=$build.schema_version;port=$Port;installed_at=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json | Set-Content -LiteralPath $currentFile -Encoding UTF8
Copy-Item -LiteralPath (Join-Path $versionPath 'source/scripts/windows/Start-Haruflow.ps1') -Destination (Join-Path $install 'Start-Haruflow.ps1') -Force
Copy-Item -LiteralPath (Join-Path $versionPath 'source/scripts/windows/Stop-Haruflow.ps1') -Destination (Join-Path $install 'Stop-Haruflow.ps1') -Force
if (-not $NoShortcuts) {
    $shell=New-Object -ComObject WScript.Shell; $shortcut=$shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Desktop')) 'Haruflow Community.lnk'))
    $shortcut.TargetPath=(Get-Command powershell.exe).Source; $shortcut.Arguments='-NoProfile -WindowStyle Hidden -File "'+(Join-Path $install 'Start-Haruflow.ps1')+'"'; $shortcut.WorkingDirectory=$install; $shortcut.Description='ERP usaha kecil Haruflow Community'; $shortcut.Save()
}
Write-Host "Haruflow Community berhasil dipasang. $verified file paket lolos checksum."
Write-Host "Folder: $install"
Write-Host "Buka Start-Haruflow.ps1 atau pintasan desktop. Akses lokal: http://127.0.0.1:$Port"
Write-Host 'Pada pemakaian pertama, baca Setup-pertama.txt pribadi untuk kode setup dan buat perusahaan serta administrator.'
if ($Start) { & (Join-Path $install 'Start-Haruflow.ps1') }
} finally {foreach ($name in $savedEnvironment.Keys) {if ([string]::IsNullOrEmpty($savedEnvironment[$name])) {Remove-Item -LiteralPath ('Env:\'+$name) -ErrorAction SilentlyContinue} else {[Environment]::SetEnvironmentVariable($name,$savedEnvironment[$name],'Process')}}}
