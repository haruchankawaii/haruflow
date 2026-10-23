[CmdletBinding()]param(
 [Parameter(Mandatory=$true)][string]$PackageRoot,
 [Parameter(Mandatory=$true)][string]$PostgresBin,
 [string]$PostgresHost='127.0.0.1', [ValidateRange(1,65535)][int]$PostgresPort=55432, [string]$PostgresUser='postgres'
)
$ErrorActionPreference='Stop'
$workspace=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$tag=[Guid]::NewGuid().ToString('N').Substring(0,8);$database='haruflow_install_'+$tag
if($database -notmatch '^haruflow_install_[a-f0-9]{8}$'){throw 'Unsafe disposable name'}
$directory=Join-Path $workspace ('.local/installer-'+$tag);$install=Join-Path $directory 'installed'
if(-not [IO.Path]::GetFullPath($directory).StartsWith($workspace+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Test path outside workspace'}
New-Item -ItemType Directory -Path $directory | Out-Null
$psql=Join-Path $PostgresBin 'psql.exe'
# A caller's PGHOSTADDR/PGSERVICE must never redirect fixture creation or cleanup.
# Save every current PG* variable, including versions libpq may add in future.
$savedEnvironment=@{DATABASE_URL=[Environment]::GetEnvironmentVariable('DATABASE_URL','Process')}
foreach($entry in @(Get-ChildItem Env: | Where-Object {$_.Name -like 'PG*'})) {$savedEnvironment[$entry.Name]=$entry.Value}
function Set-TestPgEnvironment {
 foreach($entry in @(Get-ChildItem Env: | Where-Object {$_.Name -like 'PG*'})) {Remove-Item -LiteralPath ('Env:\'+$entry.Name) -ErrorAction SilentlyContinue}
 $env:PGHOST=$PostgresHost.Trim('[',']');$env:PGPORT=[string]$PostgresPort;$env:PGUSER=$PostgresUser;$env:PGDATABASE='postgres';$env:PGCONNECT_TIMEOUT='5'
 if($savedEnvironment.ContainsKey('PGPASSWORD')) {$env:PGPASSWORD=$savedEnvironment['PGPASSWORD']}
}
function Sql([string]$Query,[string]$Db='postgres') { $value=& $psql -X -w -Atq -v ON_ERROR_STOP=1 -d $Db -c $Query; if($LASTEXITCODE -ne 0){throw 'Test SQL failed'};return $value }
function Check-InstalledHelpers {
 foreach($name in @('Start-Haruflow.ps1','Stop-Haruflow.ps1','Connect-Codex.ps1','Backup-Haruflow.ps1','Windows.Common.ps1','Manage-Postgres.ps1','Schedule-Haruflow.ps1','Run-Backup.ps1','Status-Haruflow.ps1')) {
  $installedHelper=Join-Path $install $name;$packagedHelper=Join-Path $PackageRoot ('source/scripts/windows/'+$name)
  if(-not (Test-Path -LiteralPath $installedHelper -PathType Leaf) -or -not (Test-Path -LiteralPath $packagedHelper -PathType Leaf) -or (Get-FileHash -LiteralPath $installedHelper -Algorithm SHA256).Hash -cne (Get-FileHash -LiteralPath $packagedHelper -Algorithm SHA256).Hash) {throw 'Installed integration/backup helper differs from packaged source'}
 }
}
$listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0);$listener.Start();$port=$listener.LocalEndpoint.Port;$listener.Stop()
$created=$false
try {
 Set-TestPgEnvironment
 Remove-Item Env:DATABASE_URL -ErrorAction SilentlyContinue
 Sql "CREATE DATABASE $database" | Out-Null;$created=$true
 $userinfo=[Uri]::EscapeDataString($PostgresUser)
 if($savedEnvironment['PGPASSWORD']) {$userinfo+=':'+ [Uri]::EscapeDataString($savedEnvironment['PGPASSWORD'])}
 $uriHost=if($PostgresHost.Contains(':') -and -not $PostgresHost.StartsWith('[')) {'['+$PostgresHost+']'} else {$PostgresHost}
 $url='postgres://'+$userinfo+'@'+$uriHost+':'+$PostgresPort+'/'+$database
 $env:PGHOSTADDR='192.0.2.1';$env:PGSERVICE='haruflow_nonexistent_install_test';$env:PGOPTIONS='invalid_installer_test_options'
 [Environment]::SetEnvironmentVariable('PGSERVICEFILE','','Process');$emptyServiceFilePresent=Test-Path Env:PGSERVICEFILE
 & (Join-Path $PackageRoot 'Install-Haruflow.ps1') -PackageRoot $PackageRoot -InstallRoot $install -DatabaseUrl $url -PostgresBin $PostgresBin -Port $port -NoShortcuts -NonInteractive
 if($env:PGHOSTADDR -ne '192.0.2.1' -or $env:PGSERVICE -ne 'haruflow_nonexistent_install_test' -or $env:PGOPTIONS -ne 'invalid_installer_test_options'){throw 'Installer changed caller PostgreSQL override environment'}
 if((Test-Path Env:PGSERVICEFILE) -ne $emptyServiceFilePresent -or ($emptyServiceFilePresent -and $env:PGSERVICEFILE -ne '') -or (Test-Path Env:PGAPPNAME)){throw 'Installer confused absent and empty libpq settings.'}
 Set-TestPgEnvironment
 $build=Get-Content (Join-Path $PackageRoot 'BUILD.json') -Raw | ConvertFrom-Json
 Check-InstalledHelpers
 if((Sql 'SELECT max(version) FROM _sqlx_migrations' $database) -ne [string]$build.schema_version){throw 'Installed schema mismatch'}
 $config=Join-Path $install '.env';$acl=Get-Acl -LiteralPath $config
 if(-not $acl.AreAccessRulesProtected){throw 'Credentials inherit access'}
 $owner=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
 foreach($rule in $acl.Access){$sid=$rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value;if($sid -notin @($owner,'S-1-5-18')){throw 'Credentials accessible to another principal'}}
 $env:DATABASE_URL='postgres://postgres@127.0.0.1:1/invalid_parent_environment'
 & (Join-Path $install 'Start-Haruflow.ps1') -NoBrowser
 if($env:DATABASE_URL -ne 'postgres://postgres@127.0.0.1:1/invalid_parent_environment'){throw 'Start mutated parent environment'}
 $public=Invoke-RestMethod ('http://127.0.0.1:'+$port+'/version');if($public.commit -ne $build.commit){throw 'Running version mismatch'}
 $token=([IO.File]::ReadAllLines((Join-Path $install 'Setup-pertama.txt')) | Where-Object {$_.StartsWith('Kode setup: ')}).Substring(12)
 $body=@{token=$token;company_name='INSTALL synthetic small business';name='Install tester';email='install@example.test';password='local-installer-test-password-2026'} | ConvertTo-Json
 Invoke-RestMethod ('http://127.0.0.1:'+$port+'/api/setup') -Method Post -ContentType 'application/json' -Headers @{'X-Haruflow-Request'='1'} -Body $body | Out-Null
 $company=Sql "SELECT id::text||':'||name FROM company" $database
 & (Join-Path $install 'Stop-Haruflow.ps1')
 # Exercise the copied helper, including verification through that exact file.
 $backupHelper=Join-Path $install 'Backup-Haruflow.ps1';$manualBackups=Join-Path $directory 'manual-backups'
 $null=& $backupHelper -InstallRoot $install -BackupDirectory $manualBackups -PostgresBin $PostgresBin
 $manualArchives=@(Get-ChildItem -LiteralPath $manualBackups -Filter '*.backup' -Recurse)
 if($manualArchives.Count -ne 1){throw 'Installed backup helper did not publish exactly one archive'}
 $manualArchive=$manualArchives[0].FullName
 $null=& $backupHelper -ArchivePath $manualArchive -PostgresBin $PostgresBin
 $manualMetadata=Get-Content -LiteralPath ($manualArchive+'.json') -Raw | ConvertFrom-Json
 if($manualMetadata.database -ne $database -or $manualMetadata.application.commit -ne $build.commit -or $manualMetadata.schema_version -ne $build.schema_version){throw 'Installed backup helper targeted a different database/build/schema'}
 if($env:DATABASE_URL -ne 'postgres://postgres@127.0.0.1:1/invalid_parent_environment'){throw 'Backup helper mutated parent database environment'}
 # Reinstall/upgrade must preserve the configured port and company and back up first.
 & (Join-Path $PackageRoot 'Install-Haruflow.ps1') -PackageRoot $PackageRoot -InstallRoot $install -PostgresBin $PostgresBin -NoShortcuts -NonInteractive
 if((Get-Content (Join-Path $install 'current.json') -Raw | ConvertFrom-Json).port -ne $port){throw 'Configured port lost on upgrade'}
 if((Sql "SELECT id::text||':'||name FROM company" $database) -ne $company){throw 'Company changed during reinstall'}
 Check-InstalledHelpers
 $backup=Get-ChildItem -LiteralPath (Join-Path $install 'backups') -Filter '*.backup' | Select-Object -First 1
 if(-not $backup){throw 'Pre-upgrade backup missing'}
 $metadata=Get-Content ($backup.FullName+'.json') -Raw | ConvertFrom-Json
 if($metadata.format -ne 'haruflow-windows-pre-upgrade-v1' -or $metadata.sha256 -ne (Get-FileHash $backup.FullName -Algorithm SHA256).Hash.ToLowerInvariant()){throw 'Backup checksum mismatch'}
 # Recovery works offline and must not mint or enable AI credentials.
 if((Sql 'SELECT count(*) FROM ai_tokens' $database) -ne '0' -or (Sql 'SELECT count(*) FROM ai_access_grants' $database) -ne '0'){throw 'Installer unexpectedly created AI credentials'}
 $current=Get-Content -LiteralPath (Join-Path $install 'current.json') -Raw | ConvertFrom-Json
 $installedExe=Join-Path $install ('versions/'+$current.commit+'/haruflow.exe')
 $pidFile=Join-Path $install 'server.pid'
 if((Test-Path -LiteralPath $pidFile) -and (Get-Process -Id ([int](Get-Content -LiteralPath $pidFile)) -ErrorAction SilentlyContinue)){throw 'Server must be stopped for offline recovery test'}
 $previousDatabaseUrl=[Environment]::GetEnvironmentVariable('DATABASE_URL','Process')
 try {
  $env:DATABASE_URL=$url
  Push-Location -LiteralPath $install
  try {$revocationOutput=& $installedExe admin revoke-ai-tokens 'install@example.test';if($LASTEXITCODE -ne 0){throw 'Installed offline AI token recovery failed'}} finally {Pop-Location}
 } finally {if($null -eq $previousDatabaseUrl){Remove-Item Env:DATABASE_URL -ErrorAction SilentlyContinue}else{[Environment]::SetEnvironmentVariable('DATABASE_URL',$previousDatabaseUrl,'Process')}}
 if(($revocationOutput -join "`n") -notmatch '^0 kunci AI dicabut\.') {throw 'Offline recovery returned an unexpected token count'}
 if((Sql 'SELECT count(*) FROM ai_tokens' $database) -ne '0' -or (Sql 'SELECT count(*) FROM ai_access_grants' $database) -ne '0'){throw 'Offline recovery created AI credentials or grants'}
 if((Sql "SELECT count(*) FROM audit_events WHERE action='ai.tokens.revoked_local'" $database) -ne '1' -or (Sql "SELECT count(*) FROM audit_events e JOIN users u ON u.id=e.actor_id WHERE e.action='ai.tokens.revoked_local' AND u.email='install@example.test' AND u.active AND u.role='admin' AND e.resource_type='user' AND e.resource_id=u.id AND e.details->>'revoked_count'='0' AND e.details->>'source'='local_operator_recovery'" $database) -ne '1'){throw 'Offline recovery audit missing or attributed incorrectly'}
 & (Join-Path $install 'Start-Haruflow.ps1') -NoBrowser
 # A corrupted package must be rejected before creating its installation folder.
 $corrupt=Join-Path $directory 'corrupt-package';Copy-Item -LiteralPath $PackageRoot -Destination $corrupt -Recurse
 Add-Content -LiteralPath (Join-Path $corrupt 'START-HERE.md') -Value 'corruption fixture'
 $rejected=$false;$badRoot=Join-Path $directory 'must-not-install'
 try { & (Join-Path $PackageRoot 'Install-Haruflow.ps1') -PackageRoot $corrupt -InstallRoot $badRoot -DatabaseUrl $url -PostgresBin $PostgresBin -NoShortcuts -NonInteractive } catch {$rejected=$true}
 if(-not $rejected -or (Test-Path -LiteralPath $badRoot)){throw 'Corrupted package accepted or partially installed'}
 $evidence=@{at=[DateTime]::UtcNow.ToString('o');build=$build;database=$database;scope='Disposable Windows installation, not business pilot acceptance';checks=@('manifest verification','fresh install and schema','private credential ACL','installer ignores poisoned PGHOSTADDR PGSERVICE PGOPTIONS and restores caller values','start ignoring inherited database environment','setup company','installed MCP/backup helpers equal packaged source','installed backup helper creates and verifies owned database archive','owned stop and restart','reinstall preserves company and port','pre-upgrade backup checksum','offline AI token recovery leaves credentials disabled and writes one attributed audit','caller PG and DATABASE_URL environment isolated','corrupt package rejected before install')}
 $output=Join-Path $workspace ('output/readiness/windows-install-'+$tag+'.json');New-Item -ItemType Directory -Path (Split-Path $output) -Force | Out-Null;$evidence | ConvertTo-Json -Depth 10 | Set-Content $output -Encoding UTF8
 Write-Output "Windows installer checks passed: $output"
} finally {
 try {
  try {if(Test-Path -LiteralPath (Join-Path $install 'current.json')) {& (Join-Path $install 'Stop-Haruflow.ps1')}} finally {
   if($created){Set-TestPgEnvironment;if($database -notmatch '^haruflow_install_[a-f0-9]{8}$'){throw 'Unsafe database cleanup target'};Sql "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='$database' AND pid<>pg_backend_pid()" | Out-Null;Sql "DROP DATABASE $database" | Out-Null}
  }
 } finally {
  foreach($entry in @(Get-ChildItem Env: | Where-Object {$_.Name -like 'PG*'})) {Remove-Item -LiteralPath ('Env:\'+$entry.Name) -ErrorAction SilentlyContinue}
  foreach($name in $savedEnvironment.Keys) {if($null -eq $savedEnvironment[$name]){Remove-Item -LiteralPath ('Env:\'+$name) -ErrorAction SilentlyContinue}else{[Environment]::SetEnvironmentVariable($name,$savedEnvironment[$name],'Process')}}
 }
}
