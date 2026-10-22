# Disposable local fixture: no real ERP credential or Codex setting is used.
[CmdletBinding()]param()
$ErrorActionPreference='Stop'
$workspace=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$directory=Join-Path $workspace ('.local/mcp-connection-'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($directory) | Out-Null
$tokenFile=Join-Path $directory 'dummy-token.txt'
$dummy='hf_mcp_'+('a'*64)
$fixturePromptCounter=@{Count=0}
function Read-Host {
    param([string]$Prompt,[switch]$AsSecureString)
    if (-not $AsSecureString) { throw 'Credential prompt must be hidden' }
    $fixturePromptCounter.Count++
    return ConvertTo-SecureString $dummy -AsPlainText -Force
}
# The helper only records this existing fixture path; it does not execute it.
$fixtureProgram=Join-Path $directory 'haruflow.exe'
[IO.File]::WriteAllText($fixtureProgram,'non-executable fixture')
$output=(& (Join-Path $PSScriptRoot 'Connect-Codex.ps1') -HaruflowExe $fixtureProgram -TokenFile $tokenFile) -join "`n"
if ($output.Contains($dummy)) { throw 'Output leaked credential' }
if ([IO.File]::ReadAllText($tokenFile) -cne $dummy) { throw 'Credential bytes differ' }
$bytes=[IO.File]::ReadAllBytes($tokenFile)
if ($bytes.Length -ne 71 -or $bytes[0] -ne 104) { throw 'Credential must be UTF-8 without BOM/newline' }
$acl=Get-Acl -LiteralPath $tokenFile
if (-not $acl.AreAccessRulesProtected) { throw 'Credential inherits other access' }
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
foreach($rule in $acl.Access){if($rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -ne $sid){throw 'Another principal can read credential'}}
if (-not $output.Contains('[mcp_servers.haruflow]') -or -not $output.Contains('HARUFLOW_MCP_TOKEN_FILE')) { throw 'Missing configuration' }
$denied=$false
try { & (Join-Path $PSScriptRoot 'Connect-Codex.ps1') -HaruflowExe $fixtureProgram -TokenFile $tokenFile | Out-Null } catch { $denied=$true }
if (-not $denied -or $fixturePromptCounter.Count -ne 1) { throw 'Must reject overwrite before requesting credential' }
$denied=$false
try { & (Join-Path $PSScriptRoot 'Connect-Codex.ps1') -HaruflowExe $fixtureProgram -TokenFile (Join-Path $directory 'forbidden.txt') -ServerUrl 'http://remote.example.test' | Out-Null } catch { $denied=$true }
if (-not $denied -or $fixturePromptCounter.Count -ne 1) { throw 'Must reject remote URL before requesting credential' }
Write-Output 'Windows MCP helper passed: hidden input, private ACL, exact no-BOM bytes, no secret in config/output, overwrite and remote URL refused.'
