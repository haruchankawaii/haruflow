[CmdletBinding()]
param([string]$InstallRoot=(Join-Path $env:LOCALAPPDATA 'HaruflowCommunity'),[switch]$SkipIfFresh)
$ErrorActionPreference='Stop';. (Join-Path $PSScriptRoot 'Windows.Common.ps1')
$root=Assert-HaruflowPath $InstallRoot;$identity=Get-HaruflowTaskIdentity $root;$settings=Get-Content -LiteralPath (Join-Path $root 'backup-settings.json') -Raw | ConvertFrom-Json
if($settings.format -ne 'haruflow-backup-settings-v1' -or $settings.installation_id -ne $identity.prefix -or $settings.owner_sid -ne $identity.sid -or $settings.keep_latest -lt 2 -or $settings.keep_latest -gt 365 -or $settings.time -notmatch '^([01][0-9]|2[0-3]):[0-5][0-9]$'){throw 'Pengaturan backup tidak valid.'}
$destination=Assert-HaruflowPath $settings.directory;$stateFile=Join-Path $root 'backup-state.json'
# FileShare.None prevents manual invocations racing a scheduled backup/retention.
$lockPath=Join-Path $root 'backup.lock';$lock=$null
try{$lock=[IO.File]::Open($lockPath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)}catch{throw 'Backup lain sedang berjalan.'}
try{
 if($SkipIfFresh -and (Test-Path -LiteralPath $stateFile)){
  try{$state=Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json;$due=[DateTime]::Today.Add([TimeSpan]::ParseExact($settings.time,'hh\:mm',[Globalization.CultureInfo]::InvariantCulture));if([DateTime]::Now -lt $due){$due=$due.AddDays(-1)};if($state.status -eq 'success' -and ([DateTimeOffset]$state.completed_at).UtcDateTime -ge $due.ToUniversalTime() -and [IO.Path]::GetFullPath($state.archive).StartsWith($destination.TrimEnd('\','/')+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){$null=& (Join-Path $root 'Backup-Haruflow.ps1') -ArchivePath $state.archive -PostgresBin $settings.postgres_bin;Write-Output 'Backup terbaru sudah memenuhi jadwal; dilewati.';return}}catch{# Missing/corrupt previous archive requires a fresh backup instead of a skip.
  }
 }
 $managed=Get-HaruflowManagedPostgres $root;if($managed){& (Join-Path $root 'Manage-Postgres.ps1') -Action Start -InstallRoot $root}
 [IO.Directory]::CreateDirectory($destination) | Out-Null;$previous=@{};foreach($dir in Get-ChildItem -LiteralPath $destination -Directory){$previous[$dir.Name]=$true}
 $null=& (Join-Path $root 'Backup-Haruflow.ps1') -InstallRoot $root -BackupDirectory $destination -PostgresBin $settings.postgres_bin
 $created=@(Get-ChildItem -LiteralPath $destination -Directory | Where-Object {-not $previous.ContainsKey($_.Name) -and $_.Name -match '^haruflow-[0-9]{8}-[0-9]{6}-[a-f0-9]{32}$'})
 if($created.Count -ne 1){throw 'Tidak dapat mengenali hasil backup terkelola.'}
 $archive=Join-Path $created[0].FullName 'haruflow.backup';$null=& (Join-Path $root 'Backup-Haruflow.ps1') -ArchivePath $archive -PostgresBin $settings.postgres_bin
 Write-HaruflowPrivateJson (Join-Path $created[0].FullName '.owner.json') @{format='haruflow-managed-backup-v1';installation_id=$identity.prefix;owner_sid=$identity.sid;created_at=[DateTime]::UtcNow.ToString('o')}
 $eligible=@()
 foreach($dir in Get-ChildItem -LiteralPath $destination -Directory){
  if($dir.Name -notmatch '^haruflow-[0-9]{8}-[0-9]{6}-[a-f0-9]{32}$' -or ($dir.Attributes -band [IO.FileAttributes]::ReparsePoint)){continue}
  $marker=Join-Path $dir.FullName '.owner.json';if(-not(Test-Path -LiteralPath $marker -PathType Leaf)){continue}
  try{$owner=Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json;if($owner.format -ne 'haruflow-managed-backup-v1' -or $owner.installation_id -ne $identity.prefix -or $owner.owner_sid -ne $identity.sid){continue};$files=@(Get-ChildItem -LiteralPath $dir.FullName -Force);if($files.Count -ne 4 -or @($files | Where-Object {$_.PSIsContainer -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $_.Name -notin @('haruflow.backup','haruflow.backup.json','haruflow.backup.sha256','.owner.json')}).Count){continue};$eligible+=@{directory=$dir.FullName;created_at=[DateTimeOffset]$owner.created_at}}catch{continue}
 }
 $removed=0;foreach($old in @($eligible | Sort-Object created_at -Descending | Select-Object -Skip $settings.keep_latest)){
  $full=Assert-HaruflowPath $old.directory;if($full -eq $created[0].FullName){continue};if(-not $full.StartsWith($destination.TrimEnd('\','/')+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Retensi menolak path di luar folder backup.'}
  foreach($name in @('haruflow.backup','haruflow.backup.json','haruflow.backup.sha256','.owner.json')){Remove-Item -LiteralPath (Join-Path $full $name) -Force};[IO.Directory]::Delete($full,$false);$removed++
 }
 Write-HaruflowPrivateJson $stateFile @{status='success';completed_at=[DateTime]::UtcNow.ToString('o');archive=$archive;retention_removed=$removed}
 Write-Output ('Backup terkelola selesai: '+$archive)
}catch{Write-HaruflowPrivateJson $stateFile @{status='failed';completed_at=[DateTime]::UtcNow.ToString('o');message='Backup gagal; periksa lokasi, PostgreSQL dan ruang disk. Backup lama tidak dihapus sebelum hasil baru diverifikasi.'};throw}finally{if($lock){$lock.Dispose()}}
