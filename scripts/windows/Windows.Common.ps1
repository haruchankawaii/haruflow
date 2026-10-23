# Shared helpers for the Windows operator scripts; compatible with PowerShell 5.1.
function Assert-HaruflowPath([string]$Path) {
    $full=[IO.Path]::GetFullPath($Path);$ancestor=$full
    while($ancestor){if((Test-Path -LiteralPath $ancestor) -and ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Folder Haruflow tidak boleh melewati symlink/junction.'};$next=[IO.Path]::GetDirectoryName($ancestor);if($next -eq $ancestor){break};$ancestor=$next}
    return $full
}
function Protect-HaruflowPath([string]$Path,[switch]$Directory) {
    $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User
    if($Directory){$acl=[Security.AccessControl.DirectorySecurity]::new();foreach($principal in @($sid,[Security.Principal.SecurityIdentifier]::new('S-1-5-18'))){$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($principal,'FullControl','ContainerInherit,ObjectInherit','None','Allow'))}}
    else{$acl=[Security.AccessControl.FileSecurity]::new();foreach($principal in @($sid,[Security.Principal.SecurityIdentifier]::new('S-1-5-18'))){$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($principal,'FullControl','Allow'))}}
    $acl.SetOwner($sid);$acl.SetAccessRuleProtection($true,$false)
    $item=if($Directory){[IO.DirectoryInfo]::new($Path)}else{[IO.FileInfo]::new($Path)}
    if($PSVersionTable.PSEdition -eq 'Core'){[IO.FileSystemAclExtensions]::SetAccessControl($item,$acl)}else{$item.SetAccessControl($acl)}
}
function Write-HaruflowPrivateJson([string]$Path,[object]$Value) {
    if(-not(Test-Path -LiteralPath $Path)){[IO.File]::WriteAllText($Path,'')};Protect-HaruflowPath $Path
    [IO.File]::WriteAllText($Path,($Value | ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
}
function Find-HaruflowPostgres([string]$Requested,[switch]$Server) {
    $candidates=@();if($Requested){$candidates+= $Requested}
    else{
        $command=Get-Command psql.exe -ErrorAction SilentlyContinue;if($command){$candidates+=Split-Path $command.Source}
        foreach($parent in @((Join-Path $env:ProgramFiles 'PostgreSQL'),(Join-Path $env:LOCALAPPDATA 'HaruflowCommunity/tools/pgsql'))){if(Test-Path -LiteralPath $parent){$candidates+=Join-Path $parent 'bin';$candidates+=@(Get-ChildItem -LiteralPath $parent -Directory | Sort-Object Name -Descending | ForEach-Object {Join-Path $_.FullName 'bin'})}}
    }
    $tools=@('psql','pg_dump','pg_restore');if($Server){$tools+=@('initdb','pg_ctl','postgres')}
    foreach($candidate in $candidates){$valid=$true;foreach($tool in $tools){if(-not(Test-Path -LiteralPath (Join-Path $candidate ($tool+'.exe')) -PathType Leaf)){$valid=$false}};if($valid){$version=& (Join-Path $candidate 'pg_dump.exe') --version 2>$null;if($LASTEXITCODE -eq 0 -and $version -match '\b([0-9]+)(?:\.[0-9]+)?' -and [int]$Matches[1] -ge 14){return (Resolve-Path -LiteralPath $candidate).Path}}}
    throw 'PostgreSQL belum ditemukan. Pasang/extract binaries x64 resmi dari https://www.postgresql.org/download/windows/ lalu pilih folder bin. Haruflow tidak mengunduh atau mengubah instalasi PostgreSQL lain.'
}
function Get-HaruflowTaskIdentity([string]$InstallRoot) {
    $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $hash=[Security.Cryptography.SHA256]::Create();try{$bytes=$hash.ComputeHash([Text.Encoding]::UTF8.GetBytes($sid+'|'+[IO.Path]::GetFullPath($InstallRoot).TrimEnd('\','/').ToLowerInvariant()))}finally{$hash.Dispose()}
    $suffix=([BitConverter]::ToString($bytes).Replace('-','').ToLowerInvariant()).Substring(0,16)
    return @{sid=$sid;prefix=('HaruflowCommunity-'+$suffix);marker=('Haruflow Community current-user '+$sid+' '+$suffix)}
}
function Get-HaruflowManagedPostgres([string]$InstallRoot) {
    $root=Assert-HaruflowPath $InstallRoot;$file=Join-Path $root 'postgres.json'
    if(-not(Test-Path -LiteralPath $file)){return $null}
    $config=Get-Content -LiteralPath $file -Raw | ConvertFrom-Json
    if($config.format -ne 'haruflow-private-postgres-v1' -or $config.owner_sid -ne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value -or [IO.Path]::GetFullPath($config.data_directory) -ne [IO.Path]::GetFullPath((Join-Path $root 'postgres/data')) -or $config.port -lt 1024 -or $config.port -gt 65535){throw 'Identitas cluster PostgreSQL privat tidak valid.'}
    $null=Assert-HaruflowPath $config.data_directory;$null=Find-HaruflowPostgres $config.bin -Server
    return $config
}
function Save-HaruflowPgEnvironment {
    $saved=@{};foreach($entry in @(Get-ChildItem Env: | Where-Object {$_.Name -like 'PG*'})){$saved[$entry.Name]=$entry.Value;Remove-Item -LiteralPath ('Env:\'+$entry.Name) -ErrorAction SilentlyContinue};return $saved
}
function Restore-HaruflowPgEnvironment([hashtable]$Saved) {
    foreach($entry in @(Get-ChildItem Env: | Where-Object {$_.Name -like 'PG*'})){Remove-Item -LiteralPath ('Env:\'+$entry.Name) -ErrorAction SilentlyContinue};foreach($name in $Saved.Keys){[Environment]::SetEnvironmentVariable($name,$Saved[$name],'Process')}
}
