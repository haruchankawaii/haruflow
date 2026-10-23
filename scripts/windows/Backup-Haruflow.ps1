[CmdletBinding(DefaultParameterSetName='Backup')]
param(
    [Parameter(ParameterSetName='Backup')][string]$InstallRoot = (Join-Path $env:LOCALAPPDATA 'HaruflowCommunity'),
    [Parameter(Mandatory=$true,ParameterSetName='Backup')][string]$BackupDirectory,
    [Parameter(Mandatory=$true,ParameterSetName='Verify')][string]$ArchivePath,
    [string]$PostgresBin
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Skrip ini untuk instalasi Haruflow di Windows.' }

function Pg-Tool([string]$Name) {
    if ($PostgresBin) {
        $candidate=Join-Path $PostgresBin ($Name+'.exe')
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
        throw "Utilitas $Name tidak ditemukan di -PostgresBin."
    }
    $command=Get-Command ($Name+'.exe') -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    throw "Utilitas $Name tidak ditemukan. Isi -PostgresBin dengan folder bin PostgreSQL."
}
function Assert-NoLinks([string]$Path) {
    $ancestor=[IO.Path]::GetFullPath($Path)
    while ($ancestor) {
        if ((Test-Path -LiteralPath $ancestor) -and ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Path backup/instalasi tidak boleh melewati symlink atau junction.' }
        $parent=[IO.Path]::GetDirectoryName($ancestor)
        if ($parent -eq $ancestor) { break }; $ancestor=$parent
    }
}
function Private-Path([string]$Path,[bool]$Directory=$false) {
    $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User
    if ($Directory) {
        $acl=[Security.AccessControl.DirectorySecurity]::new()
        $inheritance=[Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'
        $propagation=[Security.AccessControl.PropagationFlags]::None
        $acl.SetAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl',$inheritance,$propagation,'Allow'))
        $acl.SetAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-5-18'),'FullControl',$inheritance,$propagation,'Allow'))
    } else {
        $acl=[Security.AccessControl.FileSecurity]::new()
        $acl.SetAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','Allow'))
        $acl.SetAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-5-18'),'FullControl','Allow'))
    }
    $acl.SetAccessRuleProtection($true,$false); $acl.SetOwner($sid)
    Set-Acl -LiteralPath $Path -AclObject $acl
}
function Private-EmptyFile([string]$Path) {
    $stream=[IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    $stream.Dispose(); Private-Path $Path
}
function Verify-Archive([string]$Path,[string]$RestoreTool) {
    $full=[IO.Path]::GetFullPath($Path); Assert-NoLinks $full
    foreach ($file in @($full,($full+'.json'),($full+'.sha256'))) {
        Assert-NoLinks $file
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw 'Arsip, metadata, atau checksum backup tidak lengkap.' }
    }
    try { $metadata=Get-Content -LiteralPath ($full+'.json') -Raw | ConvertFrom-Json } catch { throw 'Metadata backup tidak dapat dibaca.' }
    if ($metadata.format -ne 'haruflow-windows-backup-v1' -or $metadata.sha256 -cnotmatch '^[a-f0-9]{64}$' -or [long]$metadata.bytes -le 0 -or $metadata.application.commit -cnotmatch '^[a-f0-9]{40}$' -or [int]$metadata.schema_version -lt 1) { throw 'Identitas metadata backup tidak valid.' }
    $checksum=[IO.File]::ReadAllText($full+'.sha256').Trim()
    $expected=$metadata.sha256+'  '+[IO.Path]::GetFileName($full)
    if ($checksum -cne $expected -or (Get-Item -LiteralPath $full).Length -ne [long]$metadata.bytes -or (Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash.ToLowerInvariant() -cne $metadata.sha256) { throw 'Ukuran atau SHA256 backup berbeda. Jangan pulihkan arsip ini.' }
    # TOC validation proves archive readability, not successful data restoration.
    $null=& $RestoreTool --list $full 2>&1
    if ($LASTEXITCODE -ne 0) { throw 'Struktur arsip PostgreSQL tidak dapat dibaca.' }
    return $metadata
}

$restore=Pg-Tool 'pg_restore'
if ($PSCmdlet.ParameterSetName -eq 'Verify') {
    $null=Verify-Archive $ArchivePath $restore
    Write-Output ('Backup lolos SHA256, ukuran, identitas dan pg_restore --list: '+[IO.Path]::GetFullPath($ArchivePath))
    return
}
$install=[IO.Path]::GetFullPath($InstallRoot); Assert-NoLinks $install
try { $current=Get-Content -LiteralPath (Join-Path $install 'current.json') -Raw | ConvertFrom-Json } catch { throw 'current.json instalasi tidak dapat dibaca.' }
if ($current.commit -cnotmatch '^[a-f0-9]{40}$' -or [int]$current.schema_version -lt 1) { throw 'Identitas instalasi tidak valid.' }
$buildFile=Join-Path $install ('versions/'+$current.commit+'/BUILD.json'); Assert-NoLinks $buildFile
try { $build=Get-Content -LiteralPath $buildFile -Raw | ConvertFrom-Json } catch { throw 'BUILD.json versi terpasang tidak dapat dibaca.' }
if ($build.application -ne 'Haruflow Community' -or $build.commit -ne $current.commit -or $build.schema_version -ne $current.schema_version) { throw 'Identitas versi terpasang berbeda dari current.json.' }
$config=Join-Path $install '.env'; Assert-NoLinks $config
$databaseUrl=$null
foreach ($line in [IO.File]::ReadAllLines($config)) {
    if ($line -match '^DATABASE_URL=(.*)$') {
        if ($null -ne $databaseUrl) { throw 'DATABASE_URL ganda dalam konfigurasi instalasi.' }
        $databaseUrl=$Matches[1]
        if ($databaseUrl.StartsWith('"') -and $databaseUrl.EndsWith('"')) { $databaseUrl=$databaseUrl.Substring(1,$databaseUrl.Length-2).Replace('\"','"').Replace('\\','\') }
    }
}
# Never print the connection string or forward it as an executable argument.
try { $dbUri=[Uri]$databaseUrl } catch { throw 'Konfigurasi koneksi database tidak valid.' }
if (-not $dbUri -or $dbUri.Scheme -notin @('postgres','postgresql') -or -not $dbUri.Host -or $dbUri.AbsolutePath -eq '/' -or $dbUri.Fragment -or $databaseUrl -match '[\r\n\x00]') { throw 'Konfigurasi koneksi database tidak valid.' }
$dump=Pg-Tool 'pg_dump'; $psql=Pg-Tool 'psql'
$environmentNames=@('PGHOST','PGHOSTADDR','PGPORT','PGUSER','PGPASSWORD','PGDATABASE','PGCONNECT_TIMEOUT','PGOPTIONS','PGSERVICE','PGSERVICEFILE','PGPASSFILE','PGSSLMODE','PGSSLROOTCERT','PGSSLCERT','PGSSLKEY','PGREQUIRESSL','PGCHANNELBINDING','PGAPPNAME')
$saved=@{}; foreach ($name in $environmentNames) { $saved[$name]=[Environment]::GetEnvironmentVariable($name,'Process') }
$owned=@(); $completed=$false
try {
    foreach ($name in $environmentNames) { Remove-Item -LiteralPath ('Env:\'+$name) -ErrorAction SilentlyContinue }
    # Uri.Host retains IPv6 brackets on .NET; libpq expects the bare address.
    $env:PGHOST=$dbUri.DnsSafeHost.Trim('[',']'); $env:PGPORT=if ($dbUri.Port -gt 0) { [string]$dbUri.Port } else { '5432' }
    $credentials=$dbUri.UserInfo.Split(':',2)
    $env:PGUSER=[Uri]::UnescapeDataString($credentials[0]); $env:PGPASSWORD=if ($credentials.Length -gt 1) { [Uri]::UnescapeDataString($credentials[1]) } else { '' }
    $env:PGDATABASE=[Uri]::UnescapeDataString($dbUri.AbsolutePath.TrimStart('/')); $env:PGCONNECT_TIMEOUT='10'; $env:PGAPPNAME='haruflow-windows-backup'
    $queryOptions=@{sslmode='PGSSLMODE';sslrootcert='PGSSLROOTCERT';sslcert='PGSSLCERT';sslkey='PGSSLKEY';channel_binding='PGCHANNELBINDING';connect_timeout='PGCONNECT_TIMEOUT'}
    $seen=@{}
    if ($dbUri.Query) {
        foreach ($part in $dbUri.Query.TrimStart('?').Split('&')) {
            $pair=$part.Split('=',2); $key=[Uri]::UnescapeDataString($pair[0]); $value=if ($pair.Length -eq 2) { [Uri]::UnescapeDataString($pair[1]) } else { '' }
            if (-not $queryOptions.ContainsKey($key) -or $seen.ContainsKey($key) -or $value -match '[\r\n\x00]') { throw 'Opsi koneksi database tidak didukung atau ganda; periksa konfigurasi secara pribadi.' }
            $seen[$key]=$true; [Environment]::SetEnvironmentVariable($queryOptions[$key],$value,'Process')
        }
    }
    $schema=& $psql -X -w -Atq -v ON_ERROR_STOP=1 -c "SELECT max(version) FROM public._sqlx_migrations WHERE success;" 2>$null
    if ($LASTEXITCODE -ne 0 -or [string]$schema -ne [string]$current.schema_version) { throw 'Database tidak terhubung atau schema berbeda dari versi terpasang. Tidak ada backup yang dipublikasikan.' }
    $postgresVersion=& $psql -X -w -Atq -v ON_ERROR_STOP=1 -c 'SHOW server_version;' 2>$null
    if ($LASTEXITCODE -ne 0) { throw 'Identitas PostgreSQL tidak dapat dibaca.' }
    $destination=[IO.Path]::GetFullPath($BackupDirectory); Assert-NoLinks $destination
    [IO.Directory]::CreateDirectory($destination) | Out-Null
    $run=Join-Path $destination ('haruflow-'+[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')+'-'+[Guid]::NewGuid().ToString('N'))
    if (Test-Path -LiteralPath $run) { throw 'Folder backup sudah ada; jalankan kembali.' }
    [IO.Directory]::CreateDirectory($run) | Out-Null; Private-Path $run $true
    $archive=Join-Path $run 'haruflow.backup'; $partial=$archive+'.partial'
    Private-EmptyFile $partial; $owned+=$partial
    $null=& $dump -w -Fc --file=$partial 2>&1
    if ($LASTEXITCODE -ne 0 -or (Get-Item -LiteralPath $partial).Length -le 0) { throw 'pg_dump gagal. Periksa akses database, ruang disk, dan kecocokan versi PostgreSQL.' }
    $null=& $restore --list $partial 2>&1
    if ($LASTEXITCODE -ne 0) { throw 'Arsip hasil pg_dump tidak dapat dibaca.' }
    $metadata=@{format='haruflow-windows-backup-v1';created_at=[DateTime]::UtcNow.ToString('o');application=@{name='Haruflow Community';commit=$current.commit;schema_version=[int]$current.schema_version};schema_version=[int]$schema;postgres_version=[string]$postgresVersion;postgres_major=[int](([string]$postgresVersion).Split('.')[0]);database=$env:PGDATABASE;sha256=(Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $partial).Length;verification='sha256-and-pg_restore-list';contains_connection_credentials=$false;contains_business_data=$true;contains_account_password_hashes=$true;contains_mcp_token_hashes=([int]$schema -ge 31)}
    foreach ($suffix in @('.json','.sha256')) { $path=$archive+$suffix+'.partial'; Private-EmptyFile $path; $owned+=$path }
    [IO.File]::WriteAllText($archive+'.json.partial',($metadata | ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($archive+'.sha256.partial',($metadata.sha256+'  '+[IO.Path]::GetFileName($archive)+"`r`n"),[Text.UTF8Encoding]::new($false))
    foreach ($suffix in @('.json','.sha256','')) {
        $source=$archive+$suffix+'.partial'; $published=$archive+$suffix
        [IO.File]::Move($source,$published); $owned+=$published
    }
    $null=Verify-Archive $archive $restore; $completed=$true
    Write-Output ('Backup selesai dan terverifikasi: '+$archive)
    Write-Output 'Tidak ada backup lama yang dihapus. Salin arsip, .json dan .sha256 bersama; lakukan latihan restore terpisah.'
} finally {
    foreach ($name in $saved.Keys) { if($null -eq $saved[$name]){Remove-Item -LiteralPath ('Env:\'+$name) -ErrorAction SilentlyContinue}else{[Environment]::SetEnvironmentVariable($name,$saved[$name],'Process')} }
    $databaseUrl=$null; $credentials=$null
    if (-not $completed) {
        # Only exact files created by this invocation are removed, never directories.
        foreach ($file in $owned) { if (Test-Path -LiteralPath $file -PathType Leaf) { Remove-Item -LiteralPath $file -Force } }
    }
}
