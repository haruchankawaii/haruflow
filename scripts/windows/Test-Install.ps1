[CmdletBinding()]param(
 [Parameter(Mandatory=$true)][string]$PackageRoot,
 [Parameter(Mandatory=$true)][string]$PostgresBin,
 [string]$PostgresHost='127.0.0.1', [int]$PostgresPort=55432, [string]$PostgresUser='postgres'
)
$ErrorActionPreference='Stop'
$workspace=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$tag=[Guid]::NewGuid().ToString('N').Substring(0,8);$database='haruflow_install_'+$tag
if($database -notmatch '^haruflow_install_[a-f0-9]{8}$'){throw 'Unsafe disposable name'}
$directory=Join-Path $workspace ('.local/installer-'+$tag);$install=Join-Path $directory 'installed'
if(-not [IO.Path]::GetFullPath($directory).StartsWith($workspace+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Test path outside workspace'}
New-Item -ItemType Directory -Path $directory | Out-Null
$psql=Join-Path $PostgresBin 'psql.exe';$env:PGHOST=$PostgresHost;$env:PGPORT=[string]$PostgresPort;$env:PGUSER=$PostgresUser;$env:PGDATABASE='postgres'
function Sql([string]$Query,[string]$Db='postgres') { $value=& $psql -X -Atq -v ON_ERROR_STOP=1 -d $Db -c $Query; if($LASTEXITCODE -ne 0){throw 'Test SQL failed'};return $value }
$listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0);$listener.Start();$port=$listener.LocalEndpoint.Port;$listener.Stop()
$created=$false
try {
 Sql "CREATE DATABASE $database" | Out-Null;$created=$true
 $url='postgres://'+$PostgresUser+'@'+$PostgresHost+':'+$PostgresPort+'/'+$database
 & (Join-Path $PackageRoot 'Install-Haruflow.ps1') -PackageRoot $PackageRoot -InstallRoot $install -DatabaseUrl $url -PostgresBin $PostgresBin -Port $port -NoShortcuts -NonInteractive
 $build=Get-Content (Join-Path $PackageRoot 'BUILD.json') -Raw | ConvertFrom-Json
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
 # Reinstall/upgrade must preserve the configured port and company and back up first.
 & (Join-Path $PackageRoot 'Install-Haruflow.ps1') -PackageRoot $PackageRoot -InstallRoot $install -PostgresBin $PostgresBin -NoShortcuts -NonInteractive
 if((Get-Content (Join-Path $install 'current.json') -Raw | ConvertFrom-Json).port -ne $port){throw 'Configured port lost on upgrade'}
 if((Sql "SELECT id::text||':'||name FROM company" $database) -ne $company){throw 'Company changed during reinstall'}
 $backup=Get-ChildItem -LiteralPath (Join-Path $install 'backups') -Filter '*.backup' | Select-Object -First 1
 if(-not $backup){throw 'Pre-upgrade backup missing'}
 $metadata=Get-Content ($backup.FullName+'.json') -Raw | ConvertFrom-Json
 if($metadata.format -ne 'haruflow-windows-pre-upgrade-v1' -or $metadata.sha256 -ne (Get-FileHash $backup.FullName -Algorithm SHA256).Hash.ToLowerInvariant()){throw 'Backup checksum mismatch'}
 & (Join-Path $install 'Start-Haruflow.ps1') -NoBrowser
 # A corrupted package must be rejected before creating its installation folder.
 $corrupt=Join-Path $directory 'corrupt-package';Copy-Item -LiteralPath $PackageRoot -Destination $corrupt -Recurse
 Add-Content -LiteralPath (Join-Path $corrupt 'START-HERE.md') -Value 'corruption fixture'
 $rejected=$false;$badRoot=Join-Path $directory 'must-not-install'
 try { & (Join-Path $PackageRoot 'Install-Haruflow.ps1') -PackageRoot $corrupt -InstallRoot $badRoot -DatabaseUrl $url -PostgresBin $PostgresBin -NoShortcuts -NonInteractive } catch {$rejected=$true}
 if(-not $rejected -or (Test-Path -LiteralPath $badRoot)){throw 'Corrupted package accepted or partially installed'}
 $evidence=@{at=[DateTime]::UtcNow.ToString('o');build=$build;database=$database;scope='Disposable Windows installation, not business pilot acceptance';checks=@('manifest verification','fresh install and schema','private credential ACL','start ignoring inherited database environment','setup company','owned stop and restart','reinstall preserves company and port','pre-upgrade backup checksum','corrupt package rejected before install')}
 $output=Join-Path $workspace ('output/readiness/windows-install-'+$tag+'.json');New-Item -ItemType Directory -Path (Split-Path $output) -Force | Out-Null;$evidence | ConvertTo-Json -Depth 10 | Set-Content $output -Encoding UTF8
 Write-Output "Windows installer checks passed: $output"
} finally {
 if(Test-Path -LiteralPath (Join-Path $install 'current.json')) {& (Join-Path $install 'Stop-Haruflow.ps1')}
 if($created){Sql "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='$database' AND pid<>pg_backend_pid()" | Out-Null;Sql "DROP DATABASE $database" | Out-Null}
}
