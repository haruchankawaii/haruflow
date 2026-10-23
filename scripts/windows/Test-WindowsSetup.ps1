[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$PackageRoot,[Parameter(Mandatory=$true)][string]$PostgresBin,[switch]$SkipDatabaseCheck)
$ErrorActionPreference='Stop';. (Join-Path $PSScriptRoot 'Windows.Common.ps1')
$workspace=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'));$tag=[Guid]::NewGuid().ToString('N').Substring(0,12)
$directory=Join-Path $workspace ('.local/windows-setup-'+$tag);$install=Join-Path $directory 'installed'
if(-not $directory.StartsWith($workspace+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Fixture outside workspace.'}
function Free-Port{$listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0);$listener.Start();$number=$listener.LocalEndpoint.Port;$listener.Stop();return $number}
$port=Free-Port;$pgPort=Free-Port;while($pgPort -eq $port){$pgPort=Free-Port};$identity=Get-HaruflowTaskIdentity $install;$registered=$false
$parentPg=Save-HaruflowPgEnvironment
$env:PGHOSTADDR='192.0.2.1';$env:PGSERVICE='haruflow_nonexistent_setup_fixture';$env:PGOPTIONS='invalid_setup_start_and_status_options';[Environment]::SetEnvironmentVariable('PGSERVICEFILE','','Process')
try{
 # Fail only pg_ctl's detached launch after initdb. The interrupted cluster
 # must keep usable protected credentials and resume without reinitialising.
 [IO.Directory]::CreateDirectory($install) | Out-Null;Protect-HaruflowPath $install -Directory
 $interrupted=$false
 function Start-Process {throw 'Fixture deliberate pg_ctl interruption'}
 try{$null=& (Join-Path $PackageRoot 'Manage-Postgres.ps1') -Action Initialize -InstallRoot $install -PostgresBin $PostgresBin -Port $pgPort}catch{if($_.Exception.Message -notmatch 'Fixture deliberate pg_ctl interruption'){throw};$interrupted=$true}finally{Remove-Item Function:Start-Process}
 $pending=Get-HaruflowManagedPostgres $install
 if(-not $interrupted -or -not $pending.bootstrap_pending -or (Test-Path -LiteralPath (Join-Path $install 'postgres/init-password.tmp'))){throw 'Interrupted cluster bootstrap did not preserve a safe retry state.'}
 $savedCredentials=@{};foreach($name in @('admin-connection.txt','connection.txt')){$file=Join-Path $install ('postgres/'+$name);if(-not(Test-Path -LiteralPath $file -PathType Leaf)){throw 'Interrupted initialization lost a generated credential.'};$savedCredentials[$name]=(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash}
 $adminFile=Join-Path $install 'postgres/admin-connection.txt';$originalAdmin=[IO.File]::ReadAllText($adminFile);$badAdmin=$originalAdmin -replace ':[0-9]+/postgres$',':bad/postgres';$sanitized=$false
 try{[IO.File]::WriteAllText($adminFile,$badAdmin,[Text.UTF8Encoding]::new($false));try{$null=& (Join-Path $PackageRoot 'Manage-Postgres.ps1') -Action Initialize -InstallRoot $install}catch{if($_.ToString().Contains($badAdmin) -or $_.ToString().Contains(($originalAdmin -split ':')[2].Split('@')[0])){throw 'Corrupted bootstrap URL leaked a private credential.'};$sanitized=$_.Exception.Message -eq 'Koneksi bootstrap privat rusak; periksa konfigurasi secara pribadi. Data tidak dihapus.'}}finally{[IO.File]::WriteAllText($adminFile,$originalAdmin,[Text.UTF8Encoding]::new($false));$originalAdmin=$null;$badAdmin=$null}
 if(-not $sanitized){throw 'Corrupted bootstrap URL did not fail safely.'}
 $null=& (Join-Path $PackageRoot 'Setup-Haruflow.ps1') -PackageRoot $PackageRoot -InstallRoot $install -PostgresBin $PostgresBin -DatabaseMode Private -Port $port -PostgresPort $pgPort -SkipSchedule -NoShortcuts -NonInteractive -NoStart
 if($env:PGHOSTADDR -ne '192.0.2.1' -or $env:PGSERVICE -ne 'haruflow_nonexistent_setup_fixture'){throw 'Setup changed the caller libpq environment.'}
 $config=Get-HaruflowManagedPostgres $install;if($config.port -ne $pgPort -or $config.bootstrap_pending){throw 'Private cluster bootstrap did not finish on the original port.'}
 foreach($name in $savedCredentials.Keys){if((Get-FileHash -LiteralPath (Join-Path $install ('postgres/'+$name)) -Algorithm SHA256).Hash -ne $savedCredentials[$name]){throw 'Retry replaced the original cluster credentials.'}}
 # Installer failure may leave a complete cluster but no .env. An explicit
 # connection must win; a later retry without one uses the protected URL.
 $explicitConnection=[IO.File]::ReadAllText((Join-Path $install 'postgres/connection.txt'))+'?connect_timeout=7'
 Remove-Item -LiteralPath (Join-Path $install '.env') -Force
 $null=& (Join-Path $PackageRoot 'Setup-Haruflow.ps1') -PackageRoot $PackageRoot -InstallRoot $install -PostgresBin $PostgresBin -DatabaseUrl $explicitConnection -SkipSchedule -NoShortcuts -NonInteractive -NoStart
 if(-not([IO.File]::ReadAllText((Join-Path $install '.env')).Contains('?connect_timeout=7'))){throw 'Managed cluster retry overwrote the explicit connection.'}
 $explicitConnection=$null;Remove-Item -LiteralPath (Join-Path $install '.env') -Force
 $null=& (Join-Path $PackageRoot 'Setup-Haruflow.ps1') -PackageRoot $PackageRoot -InstallRoot $install -PostgresBin $PostgresBin -SkipSchedule -NoShortcuts -NonInteractive -NoStart
 if([IO.File]::ReadAllText((Join-Path $install '.env')).Contains('?connect_timeout=7')){throw 'Retry did not use the saved managed connection.'}
 $savedPg=Save-HaruflowPgEnvironment
 try{
  $appUri=[Uri][IO.File]::ReadAllText((Join-Path $install 'postgres/connection.txt'));$pair=$appUri.UserInfo.Split(':',2);$env:PGHOST='127.0.0.1';$env:PGPORT=[string]$pgPort;$env:PGUSER=$pair[0];$env:PGPASSWORD=$pair[1];$env:PGDATABASE='haruflow'
  $role=& (Join-Path $PostgresBin 'psql.exe') -X -w -Atq -c "SELECT rolname||':'||rolsuper::text||':'||rolcreatedb::text||':'||rolcreaterole::text||':'||rolreplication::text||':'||rolbypassrls::text FROM pg_roles WHERE rolname=current_user;"
  if($LASTEXITCODE -ne 0 -or $role -ne 'haruflow:false:false:false:false:false'){throw 'Application role has excess cluster privileges.'}
  $storage=& (Join-Path $PostgresBin 'psql.exe') -X -w -Atq -c "SELECT current_setting('fsync')||':'||current_setting('full_page_writes')||':'||current_setting('synchronous_commit');"
  if($LASTEXITCODE -ne 0 -or $storage -ne 'on:on:on'){throw 'Durability settings are unsafe.'}
 }finally{Restore-HaruflowPgEnvironment $savedPg;$appUri=$null;$pair=$null}
 $null=& (Join-Path $install 'Start-Haruflow.ps1') -NoBrowser
 $base='http://127.0.0.1:'+$port;$current=Get-Content -LiteralPath (Join-Path $install 'current.json') -Raw | ConvertFrom-Json
 if((Invoke-RestMethod ($base+'/version')).commit -ne $current.commit){throw 'Owned service version differs.'}
 $setupToken=([IO.File]::ReadAllLines((Join-Path $install 'Setup-pertama.txt')) | Where-Object {$_.StartsWith('Kode setup: ')}).Substring(12)
 $null=Invoke-RestMethod ($base+'/api/setup') -Method Post -Headers @{'X-Haruflow-Request'='1'} -ContentType 'application/json' -Body (@{token=$setupToken;company_name='WINDOWS SIMULATION '+$tag;name='Simulation owner';email='simulation@example.test';password=[Guid]::NewGuid().ToString('N')+'Aa9!'} | ConvertTo-Json)
 $hour=[DateTime]::Now.AddHours(1).ToString('HH:mm');$schedule=Join-Path $install 'Schedule-Haruflow.ps1'
 $customBackups=Join-Path $directory 'chosen-backup-location'
 $null=& $schedule -Action Configure -InstallRoot $install -BackupTime $hour -BackupDirectory $customBackups -PostgresBin $PostgresBin -KeepLatest 2 -StartAtLogin;$registered=$true
 $oldSettings=Get-Content -LiteralPath (Join-Path $install 'backup-settings.json') -Raw | ConvertFrom-Json
 $changedHour=[DateTime]::Now.AddHours(2).ToString('HH:mm')
 $null=& $schedule -Action Configure -InstallRoot $install -BackupTime $changedHour
 $changedSettings=Get-Content -LiteralPath (Join-Path $install 'backup-settings.json') -Raw | ConvertFrom-Json
 if($changedSettings.time -ne $changedHour -or $changedSettings.directory -ne $oldSettings.directory -or $changedSettings.directory -ne [IO.Path]::GetFullPath($customBackups) -or $changedSettings.keep_latest -ne 2 -or $changedSettings.postgres_bin -ne $oldSettings.postgres_bin){throw 'Time-only schedule change reset chosen location, retention or PostgreSQL tools.'}
 foreach($suffix in @('-Backup','-Login')){$task=Get-ScheduledTask -TaskName ($identity.prefix+$suffix);if([int]$task.Principal.LogonType -ne 3 -or [int]$task.Principal.RunLevel -ne 0 -or $task.Description -ne $identity.marker -or $task.Actions.Arguments -match 'postgres://|PGPASSWORD'){throw 'Scheduled task credentials/principal differs.'}}
 $null=& $schedule -Action Disable -InstallRoot $install
 if((Get-ScheduledTask -TaskName ($identity.prefix+'-Backup')).State -ne 'Disabled'){throw 'Disable schedule failed.'}
 $null=& $schedule -Action Enable -InstallRoot $install
 $settings=Get-Content -LiteralPath (Join-Path $install 'backup-settings.json') -Raw | ConvertFrom-Json;$backupDirectory=$settings.directory
 $untouched=Join-Path $backupDirectory 'keep-my-files';[IO.Directory]::CreateDirectory($untouched) | Out-Null;[IO.File]::WriteAllText((Join-Path $untouched 'notes.txt'),'manual user file')
 $runner=Join-Path $install 'Run-Backup.ps1';for($i=0;$i -lt 3;$i++){$null=& $runner -InstallRoot $install}
 if(@(Get-ChildItem -LiteralPath $backupDirectory -Directory -Filter 'haruflow-*').Count -ne 2 -or [IO.File]::ReadAllText((Join-Path $untouched 'notes.txt')) -ne 'manual user file'){throw 'Managed retention changed unrelated files or wrong count.'}
 $state=Get-Content -LiteralPath (Join-Path $install 'backup-state.json') -Raw | ConvertFrom-Json;if($state.status -ne 'success' -or $state.retention_removed -ne 1){throw 'Backup/retention status incorrect.'}
 # Exercise a just-passed local due time. Core PowerShell parses ISO JSON
 # dates as UTC DateTime; converting it via a locale string loses that zone.
 $scheduledTime=$settings.time;$settings.time=[DateTime]::Now.AddMinutes(-1).ToString('HH:mm');Write-HaruflowPrivateJson (Join-Path $install 'backup-settings.json') $settings
 $before=[IO.File]::ReadAllText((Join-Path $install 'backup-state.json'));$null=& $runner -InstallRoot $install -SkipIfFresh;if([IO.File]::ReadAllText((Join-Path $install 'backup-state.json')) -cne $before){throw 'Login catchup repeated an already current backup.'}
 $settings.time=$scheduledTime;Write-HaruflowPrivateJson (Join-Path $install 'backup-settings.json') $settings
 $state.completed_at=[DateTime]::UtcNow.AddDays(-2).ToString('o');Write-HaruflowPrivateJson (Join-Path $install 'backup-state.json') $state
 $started=[DateTime]::UtcNow;Start-ScheduledTask -TaskName ($identity.prefix+'-Backup');$finished=$false
 for($attempt=0;$attempt -lt 80;$attempt++){Start-Sleep -Milliseconds 500;$task=Get-ScheduledTask -TaskName ($identity.prefix+'-Backup');$info=Get-ScheduledTaskInfo -InputObject $task;if($info.LastRunTime.ToUniversalTime() -ge $started.AddSeconds(-1) -and $task.State -ne 'Running'){$finished=$true;break}}
 if(-not $finished -or $info.LastTaskResult -ne 0){throw 'Actual limited-user scheduled backup did not succeed.'}
 $state=Get-Content -LiteralPath (Join-Path $install 'backup-state.json') -Raw | ConvertFrom-Json;if($state.status -ne 'success' -or ([DateTimeOffset]$state.completed_at).UtcDateTime -lt $started){throw 'Scheduled task did not publish a fresh successful backup.'}
 $null=& $schedule -Action Configure -InstallRoot $install -BackupTime ([DateTime]::Now.AddHours(2).ToString('HH:mm')) -NoStartAtLogin
 if((Get-ScheduledTask -TaskName ($identity.prefix+'-Login')).State -ne 'Disabled'){throw 'Disable login startup failed.'}
 $null=& (Join-Path $install 'Stop-Haruflow.ps1') -StopDatabase
 $null=& (Join-Path $install 'Start-Haruflow.ps1') -NoBrowser
 $null=& (Join-Path $install 'Status-Haruflow.ps1') -InstallRoot $install -CheckDatabase:(-not $SkipDatabaseCheck)
 $null=& $schedule -Action Remove -InstallRoot $install;$registered=$false
 if(Get-ScheduledTask -TaskName ($identity.prefix+'*') -ErrorAction SilentlyContinue){throw 'Fixture task remains.'}
 $output=Join-Path $workspace ('output/readiness/windows-setup-'+$tag+'.json');$evidence=@{at=[DateTime]::UtcNow.ToString('o');format='haruflow-windows-setup-test-v1';scope='Owned private PostgreSQL cluster, installation and scheduled tasks; synthetic simulation only';build=$current;database_port=$pgPort;task_prefix=$identity.prefix;checks=@('interrupted pg_ctl launch preserves private credentials and resumes owned bootstrap','managed cluster retry without .env preserves an explicit connection and otherwise uses the saved connection','private SCRAM cluster from installed binaries','restricted application role and fsync full_page_writes synchronous_commit on','owned initial database and full package installation','loopback readiness and synthetic company setup','selected backup time with current-user Interactive Limited tasks and no stored password','time-only schedule update preserves custom backup location retention and PostgreSQL tools','schedule disable/enable/change/remove','three successful managed backups keep latest two and preserve manual files','fresh login catchup skips duplicate backup','actual limited-user scheduled task publishes fresh backup with result zero','owned database stop and restart with app','operator status');database_diagnostic_skipped=[bool]$SkipDatabaseCheck;real_business_signoff='not claimed';tasks_removed=$true}
 $evidence | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $output -Encoding UTF8;Write-Output ('Windows setup simulation passed: '+$output)
}finally{
 try{try{if(Test-Path -LiteralPath (Join-Path $install 'Schedule-Haruflow.ps1')){$null=& (Join-Path $install 'Schedule-Haruflow.ps1') -Action Remove -InstallRoot $install}}finally{if(Test-Path -LiteralPath (Join-Path $install 'current.json')){$null=& (Join-Path $install 'Stop-Haruflow.ps1') -StopDatabase}elseif(Test-Path -LiteralPath (Join-Path $install 'postgres.json')){$null=& (Join-Path $PackageRoot 'Manage-Postgres.ps1') -Action Stop -InstallRoot $install}}}finally{Restore-HaruflowPgEnvironment $parentPg}
}
