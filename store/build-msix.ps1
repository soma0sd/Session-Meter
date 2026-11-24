[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9.-]{3,50}$')][string]$IdentityName,
    [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$Publisher,
    [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$PublisherDisplayName
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$config = Get-Content -Raw -LiteralPath (Join-Path $root 'src-tauri\tauri.conf.json') | ConvertFrom-Json
if ($config.version -notmatch '^\d+\.\d+\.\d+$') {
    throw 'The Store build requires a stable X.Y.Z version.'
}
$version = "$($config.version).0"
$kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
$sdk = Get-ChildItem -LiteralPath $kits -Directory |
    Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } |
    Sort-Object { [version]$_.Name } -Descending |
    ForEach-Object { Join-Path $_.FullName 'x64' } |
    Where-Object {
        (Test-Path -LiteralPath (Join-Path $_ 'makeappx.exe')) -and
        (Test-Path -LiteralPath (Join-Path $_ 'makepri.exe'))
    } | Select-Object -First 1
if (-not $sdk) {
    throw 'Windows SDK x64 MakeAppx and MakePri are required.'
}
$makeAppx = Join-Path $sdk 'makeappx.exe'
$makePri = Join-Path $sdk 'makepri.exe'
$out = Join-Path $root 'package\store'
$package = Join-Path $out "SessionMeter_${version}_x64.msix"
if (Test-Path -LiteralPath $package) {
    throw "Package already exists: $package"
}

$target = Join-Path $root 'src-tauri\target'
$previousTarget = $env:CARGO_TARGET_DIR
Push-Location -LiteralPath $root
try {
    $env:CARGO_TARGET_DIR = $target
    npm run tauri -- build --ci --no-bundle --features microsoft-store
    if ($LASTEXITCODE) { throw 'Microsoft Store application build failed.' }
} finally {
    $env:CARGO_TARGET_DIR = $previousTarget
    Pop-Location
}

$temp = Join-Path $out ('.build-' + [guid]::NewGuid().ToString('N'))
$stage = Join-Path $temp 'stage'
$assets = Join-Path $stage 'Assets'
New-Item -ItemType Directory -Path $assets -Force | Out-Null
try {
    Copy-Item -LiteralPath (Join-Path $target 'release\sessionmeter.exe') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $root 'LICENSE'), (Join-Path $root 'THIRD_PARTY_LICENSES.md') -Destination $stage
    $licenses = Join-Path $stage 'licenses'
    New-Item -ItemType Directory -Path $licenses | Out-Null
    Copy-Item -LiteralPath (Join-Path $root 'src\assets\fonts\Noto-Sans-OFL.txt'),
        (Join-Path $root 'src\assets\fonts\Noto-Sans-KR-OFL.txt') -Destination $licenses
    Copy-Item -LiteralPath (Join-Path $root 'src-tauri\assets\fonts\OFL.txt') -Destination (Join-Path $licenses 'tray-digits-OFL.txt')

    $manifest = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'AppxManifest.template.xml')
    $values = @{
        IdentityName = $IdentityName
        Publisher = $Publisher
        PublisherDisplayName = $PublisherDisplayName
        Version = $version
    }
    foreach ($key in $values.Keys) {
        $manifest = $manifest.Replace("{{$key}}", [System.Security.SecurityElement]::Escape($values[$key]))
    }
    $manifestPath = Join-Path $stage 'AppxManifest.xml'
    Set-Content -LiteralPath $manifestPath -Value $manifest -Encoding utf8

    Add-Type -AssemblyName System.Drawing
    $icon = [System.Drawing.Image]::FromFile((Join-Path $root 'src-tauri\icons\icon.png'))
    try {
        foreach ($logo in @{ StoreLogo = 50; Square150x150Logo = 150; Square44x44Logo = 44 }.GetEnumerator()) {
            $bitmap = [System.Drawing.Bitmap]::new($logo.Value, $logo.Value)
            $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
            try {
                $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $graphics.DrawImage($icon, 0, 0, $logo.Value, $logo.Value)
                $bitmap.Save((Join-Path $assets ($logo.Key + '.png')), [System.Drawing.Imaging.ImageFormat]::Png)
            } finally {
                $graphics.Dispose()
                $bitmap.Dispose()
            }
        }
    } finally {
        $icon.Dispose()
    }

    $strings = @{
        'ko-KR' = @('세션미터', 'Claude, Codex, Gemini 및 Antigravity IDE 구독 한도 모니터')
        'en-US' = @('SessionMeter', 'Subscription quota monitor for Claude, Codex, Gemini and Antigravity IDE')
    }
    foreach ($locale in $strings.Keys) {
        $dir = Join-Path $stage "Strings\$locale"
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
        $name, $description = $strings[$locale]
        $resw = @"
<?xml version="1.0" encoding="utf-8"?>
<root>
  <resheader name="resmimetype"><value>text/microsoft-resx</value></resheader>
  <resheader name="version"><value>2.0</value></resheader>
  <data name="AppDisplayName" xml:space="preserve"><value>$name</value></data>
  <data name="AppDescription" xml:space="preserve"><value>$description</value></data>
</root>
"@
        Set-Content -LiteralPath (Join-Path $dir 'Resources.resw') -Value $resw -Encoding utf8
    }

    $priConfig = Join-Path $temp 'priconfig.xml'
    $priFile = Join-Path $temp 'resources.pri'
    & $makePri createconfig /cf $priConfig /dq ko-KR_en-US
    if ($LASTEXITCODE) { throw 'MakePri config generation failed.' }
    & $makePri new /pr $stage /cf $priConfig /mn $manifestPath /of $priFile
    if ($LASTEXITCODE) { throw 'MakePri resource generation failed.' }
    Move-Item -LiteralPath $priFile -Destination $stage
    & $makeAppx pack /d $stage /p $package
    if ($LASTEXITCODE) { throw 'MakeAppx packaging failed.' }
    Get-Item -LiteralPath $package | Select-Object FullName, Length
} finally {
    if (Test-Path -LiteralPath $temp) {
        $resolved = (Resolve-Path -LiteralPath $temp).Path
        if ([IO.Path]::GetDirectoryName($resolved) -ne $out -or
            [IO.Path]::GetFileName($resolved) -notmatch '^\.build-[a-f0-9]{32}$') {
            throw "Unexpected package staging directory: $resolved"
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
