<#
Read-only probe. Finds which shader cache files Overwatch writes.
It never deletes or changes anything except its own output files.

Steps (run once per renderer, with a different -Label):
  1. Close Overwatch and Battle.net games.
  2. Optional but recommended: clear shader caches first (Disk Cleanup > DirectX Shader Cache,
     and the GPU vendor "reset shader cache" option). This gives a clear signal.
  3. .\probe.ps1 -Mode Before -Label dx11
  4. Start Overwatch with that renderer. Play one match or a Practice Range session. Exit the game.
  5. .\probe.ps1 -Mode After -Label dx11
  6. Send probe-out\report-<Label>.txt.
#>
param(
    [Parameter(Mandatory)][ValidateSet('Before', 'After')][string]$Mode,
    [Parameter(Mandatory)][string]$Label,
    [string]$OutDir = (Join-Path $PSScriptRoot 'probe-out')
)

$ErrorActionPreference = 'Stop'
$local = $env:LOCALAPPDATA
$data = $env:ProgramData
$docs = [Environment]::GetFolderPath('MyDocuments')

$candidates = @(
    "$local\D3DSCache",
    "$local\NVIDIA\DXCache",
    "$local\NVIDIA\GLCache",
    "$local\NVIDIA\PerDriverVersion",
    "$local\NVIDIA Corporation\NV_Cache",
    "$data\NVIDIA Corporation\NV_Cache",
    "$local\AMD\DxCache",
    "$local\AMD\DxcCache",
    "$local\AMD\VkCache",
    "$local\AMD\GLCache",
    "$local\Intel\ShaderCache",
    "$local\Blizzard Entertainment",
    "$data\Blizzard Entertainment",
    "$docs\Overwatch"
)

function Get-Snapshot {
    $items = foreach ($dir in $candidates) {
        if (-not (Test-Path -LiteralPath $dir)) { continue }
        Get-ChildItem -LiteralPath $dir -Recurse -Force -File -ErrorAction SilentlyContinue |
            ForEach-Object {
                [pscustomobject]@{
                    Root  = $dir
                    Path  = $_.FullName
                    Size  = $_.Length
                    Ticks = $_.LastWriteTimeUtc.Ticks
                }
            }
    }
    @($items)
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$snapFile = Join-Path $OutDir "snapshot-$Label.json"

if ($Mode -eq 'Before') {
    $snap = Get-Snapshot
    ConvertTo-Json -InputObject $snap -Depth 3 -Compress | Set-Content -LiteralPath $snapFile -Encoding UTF8
    Write-Host "Before snapshot saved: $($snap.Count) files -> $snapFile"
    return
}

if (-not (Test-Path -LiteralPath $snapFile)) { throw "No Before snapshot for label '$Label'." }

$before = @{}
# Windows PowerShell 5.1 returns the JSON array as one object. Parentheses make the pipeline unroll it.
$beforeItems = Get-Content -LiteralPath $snapFile -Raw -Encoding UTF8 | ConvertFrom-Json
($beforeItems) | ForEach-Object { $before[$_.Path] = $_ }
$after = Get-Snapshot
ConvertTo-Json -InputObject $after -Depth 3 -Compress | Set-Content -LiteralPath (Join-Path $OutDir "snapshot-$Label-after.json") -Encoding UTF8
$afterPaths = @{}
$changes = foreach ($f in $after) {
    $afterPaths[$f.Path] = $true
    $old = $before[$f.Path]
    if ($null -eq $old) { $f | Add-Member -NotePropertyName Kind -NotePropertyValue 'new' -PassThru }
    elseif ($old.Size -ne $f.Size -or $old.Ticks -ne $f.Ticks) { $f | Add-Member -NotePropertyName Kind -NotePropertyValue 'modified' -PassThru }
}
$changes = @($changes)
$deleted = @($before.Keys | Where-Object { -not $afterPaths.ContainsKey($_) })

$gpu = Get-CimInstance Win32_VideoController | ForEach-Object { "$($_.Name) | driver $($_.DriverVersion)" }
$os = (Get-CimInstance Win32_OperatingSystem).Caption + ' ' + (Get-CimInstance Win32_OperatingSystem).BuildNumber

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("Label: $Label")
$lines.Add("OS: $os")
foreach ($g in $gpu) { $lines.Add("GPU: $g") }
$lines.Add("Files before: $($before.Count)  after: $($after.Count)  new/modified: $($changes.Count)  deleted: $($deleted.Count)")
$lines.Add('')
$lines.Add('== Changes by root ==')
foreach ($grp in ($changes | Group-Object Root)) {
    $sum = ($grp.Group | Measure-Object Size -Sum).Sum
    $new = @($grp.Group | Where-Object Kind -eq 'new').Count
    $lines.Add(("{0}`n    files: {1} (new {2}, modified {3}), bytes: {4}" -f $grp.Name, $grp.Count, $new, ($grp.Count - $new), $sum))
}
$lines.Add('')
$lines.Add('== Changed files (up to 60 per root, largest first) ==')
foreach ($grp in ($changes | Group-Object Root)) {
    $lines.Add($grp.Name)
    foreach ($f in ($grp.Group | Sort-Object Size -Descending | Select-Object -First 60)) {
        $lines.Add(("    [{0}] {1}  {2} bytes" -f $f.Kind, $f.Path.Substring($grp.Name.Length).TrimStart('\'), $f.Size))
    }
}
$lines.Add('')
$lines.Add('== Deleted files by root ==')
foreach ($grp in ($deleted | Group-Object { $before[$_].Root })) { $lines.Add("$($grp.Name): $($grp.Count)") }

$report = Join-Path $OutDir "report-$Label.txt"
$lines | Set-Content -LiteralPath $report -Encoding UTF8
Write-Host "Report saved: $report"
