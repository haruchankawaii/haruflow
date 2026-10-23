[CmdletBinding()]
param([ValidateSet('Configure','Startup','Status','Disable','Enable','Remove','RunNow')][string]$Action='Status',[string]$InstallRoot=(Join-Path $env:LOCALAPPDATA 'HaruflowCommunity'),[ValidatePattern('^([01][0-9]|2[0-3]):[0-5][0-9]$')][string]$BackupTime,[string]$BackupDirectory,[string]$PostgresBin,[ValidateRange(2,365)][int]$KeepLatest=14,[switch]$StartAtLogin,[switch]$NoStartAtLogin)
$ErrorActionPreference='Stop';. (Join-Path $PSScriptRoot 'Windows.Common.ps1')
$root=Assert-HaruflowPath $InstallRoot;$identity=Get-HaruflowTaskIdentity $root;$backupName=$identity.prefix+'-Backup';$loginName=$identity.prefix+'-Login';$settingsFile=Join-Path $root 'backup-settings.json'
function Owned-Task([string]$Name){
 $task=Get-ScheduledTask -TaskName $Name -TaskPath '\' -ErrorAction SilentlyContinue
 if($task){try{$principalSid=if($task.Principal.UserId -match '^S-1-'){$task.Principal.UserId}else{([Security.Principal.NTAccount]::new($task.Principal.UserId)).Translate([Security.Principal.SecurityIdentifier]).Value}}catch{throw 'Identitas akun task tidak dapat diverifikasi.'};if($task.Description -ne $identity.marker -or $principalSid -ne $identity.sid){throw 'Task dengan nama sama bukan milik instalasi/akun ini.'}}
 return $task
}
function Register-OwnedTask([string]$Name,[string]$Script,[object[]]$Triggers){
 $null=Owned-Task $Name
 foreach($path in @($root,$Script)){if($path -match '["\r\n]'){throw 'Path task mengandung karakter tidak didukung.'}}
 $argument='-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File "'+$Script+'"';if($Name -eq $backupName){$argument+=' -InstallRoot "'+$root+'" -SkipIfFresh'}else{$argument+=' -NoBrowser'}
 $actionObject=New-ScheduledTaskAction -Execute (Join-Path $env:WINDIR 'System32/WindowsPowerShell/v1.0/powershell.exe') -Argument $argument -WorkingDirectory $root
 $principal=New-ScheduledTaskPrincipal -UserId $identity.sid -LogonType Interactive -RunLevel Limited
 $settings=New-ScheduledTaskSettingsSet -StartWhenAvailable -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 2) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
 Register-ScheduledTask -TaskName $Name -TaskPath '\' -Action $actionObject -Trigger $Triggers -Principal $principal -Settings $settings -Description $identity.marker -Force | Out-Null
}
if($Action -eq 'Startup'){
 if(-not(Test-Path -LiteralPath (Join-Path $root 'current.json'))){throw 'Pasang Haruflow dahulu.'}
 if($NoStartAtLogin){$task=Owned-Task $loginName;if($task){Disable-ScheduledTask -InputObject $task | Out-Null}}else{Register-OwnedTask $loginName (Join-Path $root 'Start-Haruflow.ps1') @((New-ScheduledTaskTrigger -AtLogOn -User $identity.sid))};return
}
if($Action -eq 'Configure'){
 if(-not(Test-Path -LiteralPath (Join-Path $root 'current.json'))){throw 'Pasang Haruflow sebelum mengatur jadwal.'}
 # A time-only change must preserve the user's chosen off-device location,
 # retention and PostgreSQL tools. Parameter defaults apply to first setup only.
 if(Test-Path -LiteralPath $settingsFile){
  try{$previousSettings=Get-Content -LiteralPath $settingsFile -Raw | ConvertFrom-Json}catch{throw 'Pengaturan backup lama tidak dapat dibaca; tidak ada jadwal diubah.'}
  if($previousSettings.format -ne 'haruflow-backup-settings-v1' -or $previousSettings.installation_id -ne $identity.prefix -or $previousSettings.owner_sid -ne $identity.sid -or [string]::IsNullOrWhiteSpace($previousSettings.directory) -or [string]::IsNullOrWhiteSpace($previousSettings.postgres_bin) -or $previousSettings.keep_latest -lt 2 -or $previousSettings.keep_latest -gt 365){throw 'Pengaturan backup lama bukan milik instalasi ini atau tidak valid.'}
  if(-not $PSBoundParameters.ContainsKey('BackupDirectory')){$BackupDirectory=$previousSettings.directory}
  if(-not $PSBoundParameters.ContainsKey('KeepLatest')){$KeepLatest=[int]$previousSettings.keep_latest}
  if(-not $PSBoundParameters.ContainsKey('PostgresBin')){$PostgresBin=$previousSettings.postgres_bin}
 }
 if(-not $BackupTime){
  Write-Host 'Pilih jam backup harian: 1) 12:00  2) 17:00  3) 21:00  4) isi HH:mm  5) tanpa jadwal'
  $choice=Read-Host 'Pilihan';switch($choice){'1'{$BackupTime='12:00'}'2'{$BackupTime='17:00'}'3'{$BackupTime='21:00'}'4'{$BackupTime=Read-Host 'Jam lokal HH:mm'}'5'{Write-Output 'Jadwal tidak diubah.';return}default{throw 'Pilihan jam tidak valid; tidak ada task dibuat.'}}
 }
 if($BackupTime -notmatch '^([01][0-9]|2[0-3]):[0-5][0-9]$'){throw 'Jam harus HH:mm (00:00 sampai 23:59).'}
 if(-not $BackupDirectory){$BackupDirectory=Join-Path $root 'backups/scheduled'};$destination=Assert-HaruflowPath $BackupDirectory
 $managed=Get-HaruflowManagedPostgres $root;if(-not $PostgresBin -and $managed){$PostgresBin=$managed.bin};$bin=Find-HaruflowPostgres $PostgresBin
 [IO.Directory]::CreateDirectory($destination) | Out-Null
 # The selected folder remains untouched; only newly created run folders receive private ACLs.
 $settings=@{format='haruflow-backup-settings-v1';installation_id=$identity.prefix;owner_sid=$identity.sid;time=$BackupTime;directory=$destination;postgres_bin=$bin;keep_latest=$KeepLatest;updated_at=[DateTime]::UtcNow.ToString('o')}
 Write-HaruflowPrivateJson $settingsFile $settings
 $daily=New-ScheduledTaskTrigger -Daily -At ([DateTime]::Today.Add([TimeSpan]::ParseExact($BackupTime,'hh\:mm',[Globalization.CultureInfo]::InvariantCulture)))
 $login=New-ScheduledTaskTrigger -AtLogOn -User $identity.sid;$login.Delay='PT2M'
 Register-OwnedTask $backupName (Join-Path $root 'Run-Backup.ps1') @($daily,$login)
 if($StartAtLogin){Register-OwnedTask $loginName (Join-Path $root 'Start-Haruflow.ps1') @((New-ScheduledTaskTrigger -AtLogOn -User $identity.sid))}
 if($NoStartAtLogin){$task=Owned-Task $loginName;if($task){Disable-ScheduledTask -InputObject $task | Out-Null}}
 Write-Output ('Backup harian '+$BackupTime+' waktu Windows, hanya saat akun ini masuk. '+$KeepLatest+' backup terkelola terbaru dipertahankan.');return
}
foreach($name in @($backupName,$loginName)){
 $task=Owned-Task $name;if(-not $task){if($Action -eq 'Status'){Write-Output ($name+': belum diatur')};continue}
 switch($Action){
  'Status'{$info=Get-ScheduledTaskInfo -InputObject $task;Write-Output ($name+': '+$task.State+'; terakhir '+$info.LastRunTime+'; hasil '+$info.LastTaskResult+'; berikutnya '+$info.NextRunTime)}
  'Disable'{Disable-ScheduledTask -InputObject $task | Out-Null}
  'Enable'{Enable-ScheduledTask -InputObject $task | Out-Null}
  'Remove'{Unregister-ScheduledTask -TaskName $name -TaskPath '\' -Confirm:$false}
  'RunNow'{if($name -eq $backupName){Start-ScheduledTask -InputObject $task}}
 }
}
