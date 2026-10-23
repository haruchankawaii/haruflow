[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$PostgresBin,
    [string]$PostgresHost='127.0.0.1',
    [ValidateRange(1,65535)][int]$PostgresPort=55432,
    [string]$PostgresUser='postgres'
)
$ErrorActionPreference='Stop'
$workspace=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$tag=[Guid]::NewGuid().ToString('N').Substring(0,12)
$source='haruflow_backup_'+$tag; $target='haruflow_restore_'+$tag
foreach ($name in @($source,$target)) { if ($name -notmatch '^haruflow_(backup|restore)_[a-f0-9]{12}$') { throw 'Nama database uji tidak aman.' } }
$directory=Join-Path $workspace ('output/readiness/windows-backup-'+$tag)
if (-not $directory.StartsWith($workspace+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Output uji di luar workspace.' }
[IO.Directory]::CreateDirectory($directory) | Out-Null
$psql=Join-Path $PostgresBin 'psql.exe'; $restore=Join-Path $PostgresBin 'pg_restore.exe'
$environmentNames=@('PGHOST','PGHOSTADDR','PGPORT','PGUSER','PGPASSWORD','PGDATABASE','PGCONNECT_TIMEOUT','PGOPTIONS','PGSERVICE','PGSERVICEFILE','PGPASSFILE','PGSSLMODE','PGSSLROOTCERT','PGSSLCERT','PGSSLKEY','PGREQUIRESSL','PGCHANNELBINDING','PGAPPNAME')
$saved=@{}; foreach ($name in $environmentNames) { $saved[$name]=[Environment]::GetEnvironmentVariable($name,'Process') }
$created=@()
function Fixture-Environment {
    foreach ($name in $environmentNames) { Remove-Item -LiteralPath ('Env:\'+$name) -ErrorAction SilentlyContinue }
    $env:PGHOST=$PostgresHost; $env:PGPORT=[string]$PostgresPort; $env:PGUSER=$PostgresUser; $env:PGDATABASE='postgres'; $env:PGPASSWORD=$saved.PGPASSWORD; $env:PGCONNECT_TIMEOUT='10'
}
function Sql([string]$Query,[string]$Database='postgres') {
    $value=& $psql -X -w -Atq -v ON_ERROR_STOP=1 -d $Database -c $Query
    if ($LASTEXITCODE -ne 0) { throw 'SQL database disposable gagal.' }; return $value
}
function Check-Private([string]$Path) {
    $acl=Get-Acl -LiteralPath $Path
    if (-not $acl.AreAccessRulesProtected) { throw 'Berkas backup mewarisi izin.' }
    $owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    foreach ($rule in $acl.Access) { if ($rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -notin @($owner,'S-1-5-18')) { throw 'Backup dapat diakses principal lain.' } }
}
try {
    Fixture-Environment
    # Parser-only regression: no IPv6 server is required for address normalization.
    $ipv6=[Uri]'postgres://fixture@[::1]:5432/haruflow_fixture'
    $ipv6Host=$ipv6.DnsSafeHost.Trim('[',']')
    if ($ipv6Host.Contains('[') -or $ipv6Host.Contains(']') -or -not ([Net.IPAddress]::Parse($ipv6Host)).Equals([Net.IPAddress]::IPv6Loopback)) { throw 'Normalisasi alamat IPv6 untuk libpq gagal.' }
    Sql "CREATE DATABASE $source" | Out-Null; $created+=$source
    Sql "CREATE DATABASE $target" | Out-Null; $created+=$target
    Sql "CREATE TABLE _sqlx_migrations(version bigint,success boolean); INSERT INTO _sqlx_migrations VALUES(31,true); CREATE TABLE business_fixture(id integer PRIMARY KEY,value text); INSERT INTO business_fixture VALUES(1,'bukti asli & nilai: 12345');" $source | Out-Null
    $commit='a'*40; $install=Join-Path $directory 'installed'; $version=Join-Path $install ('versions/'+$commit)
    [IO.Directory]::CreateDirectory($version) | Out-Null
    @{commit=$commit;schema_version=31;port=65534} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $install 'current.json') -Encoding UTF8
    @{application='Haruflow Community';commit=$commit;schema_version=31} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $version 'BUILD.json') -Encoding UTF8
    # Fixture uses ambient test password only inside a private config, never a CLI argument.
    $userinfo=[Uri]::EscapeDataString($PostgresUser)
    if ($saved.PGPASSWORD) { $userinfo+=':'+ [Uri]::EscapeDataString($saved.PGPASSWORD) }
    $url='postgres://'+$userinfo+'@'+$PostgresHost+':'+$PostgresPort+'/'+$source
    $config=Join-Path $install '.env'; [IO.File]::WriteAllText($config,'')
    $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl=[Security.AccessControl.FileSecurity]::new(); $acl.SetOwner($sid); $acl.SetAccessRuleProtection($true,$false); $acl.SetAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','Allow')); Set-Acl -LiteralPath $config -AclObject $acl
    [IO.File]::WriteAllText($config,('DATABASE_URL="'+$url.Replace('\','\\').Replace('"','\"')+'"'),[Text.UTF8Encoding]::new($false))
    $destination=Join-Path $directory 'backups'; $script=Join-Path $PSScriptRoot 'Backup-Haruflow.ps1'
    $env:PGDATABASE='must_not_be_used'; $env:PGOPTIONS='-c application_name=parent_backup_test'; $env:PGHOSTADDR='192.0.2.1'; $env:PGSERVICE='haruflow_ambient_must_not_be_used'
    $messages=& $script -InstallRoot $install -BackupDirectory $destination -PostgresBin $PostgresBin
    if ($env:PGDATABASE -ne 'must_not_be_used' -or $env:PGOPTIONS -ne '-c application_name=parent_backup_test' -or $env:PGHOSTADDR -ne '192.0.2.1' -or $env:PGSERVICE -ne 'haruflow_ambient_must_not_be_used') { throw 'Environment pemanggil berubah.' }
    if ($saved.PGPASSWORD -and (($messages -join "`n").Contains($saved.PGPASSWORD))) { throw 'Password bocor pada output.' }
    Fixture-Environment
    $archive=Get-ChildItem -LiteralPath $destination -Filter '*.backup' -Recurse | Select-Object -First 1
    if (-not $archive) { throw 'Arsip backup tidak ditemukan.' }
    $metadata=Get-Content -LiteralPath ($archive.FullName+'.json') -Raw | ConvertFrom-Json
    if ($metadata.schema_version -ne 31 -or $metadata.application.commit -ne $commit) { throw 'Identitas backup berbeda.' }
    foreach ($file in @($archive.FullName,($archive.FullName+'.json'),($archive.FullName+'.sha256'),$archive.DirectoryName)) { Check-Private $file }
    $null=& $script -ArchivePath $archive.FullName -PostgresBin $PostgresBin
    $null=& $restore -w -d $target --no-owner --no-acl --single-transaction --exit-on-error $archive.FullName
    if ($LASTEXITCODE -ne 0) { throw 'Latihan restore database disposable gagal.' }
    if ((Sql 'SELECT id::text||value FROM business_fixture ORDER BY id' $source) -cne (Sql 'SELECT id::text||value FROM business_fixture ORDER BY id' $target)) { throw 'Data backup dan restore berbeda.' }
    # A later backup must preserve the first archive and create a distinct folder.
    $before=(Get-FileHash -LiteralPath $archive.FullName -Algorithm SHA256).Hash
    $null=& $script -InstallRoot $install -BackupDirectory $destination -PostgresBin $PostgresBin
    if (@(Get-ChildItem -LiteralPath $destination -Filter '*.backup' -Recurse).Count -ne 2 -or (Get-FileHash -LiteralPath $archive.FullName -Algorithm SHA256).Hash -cne $before) { throw 'Backup lama berubah atau terhapus.' }
    # Corruption must be rejected before a restore can be attempted.
    $stream=[IO.File]::Open($archive.FullName,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
    try { $stream.Position=$stream.Length-1; $last=$stream.ReadByte(); $stream.Position=$stream.Length-1; $stream.WriteByte([byte]($last -bxor 255)) } finally { $stream.Dispose() }
    $rejected=$false
    try { $null=& $script -ArchivePath $archive.FullName -PostgresBin $PostgresBin } catch { $rejected=$true }
    if (-not $rejected) { throw 'Arsip yang dimodifikasi diterima.' }
    $null=Sql 'UPDATE _sqlx_migrations SET version=30' $source
    $rejected=$false
    try { $null=& $script -InstallRoot $install -BackupDirectory $destination -PostgresBin $PostgresBin } catch { $rejected=$true }
    if (-not $rejected -or @(Get-ChildItem -LiteralPath $destination -Filter '*.backup' -Recurse).Count -ne 2) { throw 'Schema berbeda diterima atau arsip gagal dipublikasikan.' }
    $evidence=@{format='haruflow-windows-backup-test-v1';at=[DateTime]::UtcNow.ToString('o');scope='Disposable synthetic databases only; no business pilot acceptance';checks=@('IPv6 URI host normalized for libpq (parser only)','private backup folder and files','installed version and schema metadata','custom archive SHA256 and TOC','ambient PGHOSTADDR PGSERVICE and PGOPTIONS isolated and restored','restored fixture data equals source','second backup preserves previous archive','tampered archive rejected','schema mismatch rejected before publication');source_database=$source;target_database=$target;business_signoff='not claimed';retention='no deletion'}
    $evidence | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $directory 'verified.json') -Encoding UTF8
    Write-Output ('Windows backup tests passed: '+(Join-Path $directory 'verified.json'))
} finally {
    try {
        Fixture-Environment
        foreach ($database in $created) {
            if ($database -notmatch '^haruflow_(backup|restore)_[a-f0-9]{12}$') { throw 'Cleanup menolak nama database.' }
            $null=Sql "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='$database' AND pid<>pg_backend_pid();"
            $null=Sql "DROP DATABASE $database;"
        }
    } finally { foreach ($name in $saved.Keys) { if($null -eq $saved[$name]){Remove-Item -LiteralPath ('Env:\'+$name) -ErrorAction SilentlyContinue}else{[Environment]::SetEnvironmentVariable($name,$saved[$name],'Process')} } }
}
