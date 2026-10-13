# Nightly intake of Drive pictures: Incoming/ -> Processed/<uuid>.<ext>, subjects filed in Config/subjects.json.
# Code does the I/O; a model (Haiku) only looks at the small previews and writes labels.json.
#   intake.ps1 prepare   download + preview new pictures, print what to label (prints NONE when idle)
#   intake.ps1 apply     move labelled pictures to Processed/, unlabelled to Errored/, merge subjects.json
# Needs the write-scoped remote from `just drive-auth-writer` (service account orange-writer@static-pottery-465109-b6.iam.gserviceaccount.com)
# and the Drive folder shared with it as Editor.
param([Parameter(Mandatory)][ValidateSet('prepare', 'apply')][string]$Step)

$ErrorActionPreference = 'Stop'
$Folder = '1foxxmTR49YCuahiHT8zbkNTgLo2DIKOS' # production root, see justfile
$Remote = "gdrive-rw,root_folder_id=${Folder}:"
$Work = Join-Path $PSScriptRoot '..\scratch\intake'
$PreviewPx = 768
# Caps model spend per night (about 1k tokens per picture); the rest wait in Incoming/ for later runs.
$MaxPerRun = if ($env:OYG_INTAKE_MAX) { [int]$env:OYG_INTAKE_MAX } else { 60 }
$Exts = 'jpg', 'jpeg', 'png', 'webp'

function Rc { & rclone @args; if ($LASTEXITCODE -ne 0) { throw "rclone $($args -join ' ') failed" } }

function New-Preview($src, $dst) {
    Add-Type -AssemblyName System.Drawing
    $img = [System.Drawing.Image]::FromFile($src)
    try {
        $k = [Math]::Min(1.0, $PreviewPx / [Math]::Max($img.Width, $img.Height))
        $bmp = New-Object System.Drawing.Bitmap ([int]($img.Width * $k)), ([int]($img.Height * $k))
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.DrawImage($img, 0, 0, $bmp.Width, $bmp.Height)
        $bmp.Save($dst, [System.Drawing.Imaging.ImageFormat]::Jpeg)
        $g.Dispose(); $bmp.Dispose()
    } finally { $img.Dispose() }
}

if ($Step -eq 'prepare') {
    if (Test-Path $Work) { Remove-Item $Work -Recurse -Force }
    New-Item -ItemType Directory "$Work\orig", "$Work\preview" | Out-Null
    $names = @(Rc lsf "${Remote}Incoming" --files-only | Where-Object { $Exts -contains ($_.Split('.')[-1].ToLower()) })
    if ($names.Count -eq 0) { 'NONE'; exit 0 }
    $left = [Math]::Max(0, $names.Count - $MaxPerRun)
    $names = @($names | Select-Object -First $MaxPerRun)
    if ($left -gt 0) { "CAPPED: $left more stay in Incoming for the next run" }

    $manifest = @()
    foreach ($n in $names) {
        $id = [guid]::NewGuid().ToString()
        $ext = $n.Split('.')[-1].ToLower()
        Rc copyto "${Remote}Incoming/$n" "$Work\orig\$id.$ext" --max-size 25M
        try { New-Preview "$Work\orig\$id.$ext" "$Work\preview\$id.jpg" } catch { Write-Warning "unreadable $n"; continue }
        $manifest += [pscustomobject]@{ uuid = $id; name = $n; ext = $ext }
    }
    $manifest | ConvertTo-Json -AsArray | Set-Content "$Work\manifest.json"
    Rc copyto "${Remote}Config/subjects.json" "$Work\subjects.json"
    $known = (Get-Content "$Work\subjects.json" -Raw | ConvertFrom-Json).PSObject.Properties.Name -join ', '
    "KNOWN SUBJECTS: $known"
    "LABEL THESE (view each preview, write $Work\labels.json as {""<uuid>"": [""subject"", ...]}):"
    $manifest | ForEach-Object { "$Work\preview\$($_.uuid).jpg" }
    exit 0
}

# apply
$manifest = Get-Content "$Work\manifest.json" -Raw | ConvertFrom-Json
$labels = Get-Content "$Work\labels.json" -Raw | ConvertFrom-Json
$subjects = [ordered]@{}
(Get-Content "$Work\subjects.json" -Raw | ConvertFrom-Json).PSObject.Properties | ForEach-Object { $subjects[$_.Name] = @($_.Value) }

foreach ($m in $manifest) {
    $src = "${Remote}Incoming/$($m.name)"
    $tags = @($labels.($m.uuid) | Where-Object { $_ } | ForEach-Object { $_.ToString().Trim().ToLower() } | Select-Object -Unique)
    if ($tags.Count -eq 0) { Rc moveto $src "${Remote}Errored/$($m.name)"; continue }
    Rc moveto $src "${Remote}Processed/$($m.uuid).$($m.ext)"
    foreach ($t in $tags) {
        if (-not $subjects.Contains($t)) { $subjects[$t] = @() }
        $subjects[$t] += $m.uuid
    }
}
$subjects | ConvertTo-Json -Depth 4 | Set-Content "$Work\subjects.out.json"
Rc copyto "$Work\subjects.out.json" "${Remote}Config/subjects.json"
"DONE: $($manifest.Count) pictures"
