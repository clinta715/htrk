# publish-release.ps1 — Tag, build, zip, and create a GitHub release
# Usage: .\publish-release.ps1 [-Tag v0.27.0] [-Draft]

param(
    [string]$Tag = "v0.27.0",
    [switch]$Draft
)

$ErrorActionPreference = "Stop"
$zipName = "htrk-$Tag-win64.zip"
$zipPath = Join-Path $PSScriptRoot $zipName

Write-Host "=== HTRK Release: $Tag ===" -ForegroundColor Cyan

# 1. Verify clean working tree
$dirty = git status --porcelain
if ($dirty) {
    Write-Host "ERROR: Working tree is dirty. Commit or stash first." -ForegroundColor Red
    exit 1
}

# 2. Tag the commit
$existingTag = git tag -l $Tag
if ($existingTag) {
    Write-Host "Tag $Tag already exists, skipping tag creation." -ForegroundColor Yellow
} else {
    Write-Host "Creating tag $Tag ..." -ForegroundColor Green
    git tag -a $Tag -m "Release $Tag"
    git push origin $Tag
    Write-Host "Tag pushed to origin." -ForegroundColor Green
}

# 3. Build release binary
Write-Host "Building release binary ..." -ForegroundColor Green
cargo build --release
if ($LASTEXITCODE -ne 0) { Write-Host "Build failed!" -ForegroundColor Red; exit 1 }

# 4. Stage release files
$staging = Join-Path $PSScriptRoot "_release_staging"
if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }
New-Item -ItemType Directory -Path $staging | Out-Null
New-Item -ItemType Directory -Path "$staging\docs" | Out-Null

$exe = Join-Path $PSScriptRoot "target\release\htrk.exe"
if (-not (Test-Path $exe)) { Write-Host "ERROR: htrk.exe not found at $exe" -ForegroundColor Red; exit 1 }

Copy-Item $exe "$staging\"
@("README.md","CHANGELOG.md","KEYBOARD.md","FORMATS.md","STYLE.md") | ForEach-Object {
    $src = Join-Path $PSScriptRoot $_
    if (Test-Path $src) { Copy-Item $src "$staging\" }
}

# Docs directory
$docsSrc = Join-Path $PSScriptRoot "docs"
$docsDst = "$staging\docs"
Get-ChildItem "$docsSrc\*.md" | ForEach-Object { Copy-Item $_.FullName $docsDst }

# 5. Create zip
if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
Compress-Archive -Path "$staging\*" -DestinationPath $zipPath -Force
$zipSize = [math]::Round((Get-Item $zipPath).Length / 1MB, 2)
Write-Host "Created $zipName ($zipSize MB)" -ForegroundColor Green

# 6. Clean up staging
Remove-Item $staging -Recurse -Force

# 7. Read changelog for release notes
$changelog = Get-Content (Join-Path $PSScriptRoot "CHANGELOG.md") -Raw
$version = $Tag.TrimStart("v")
$pattern = "(?s)## \[$([regex]::Escape($version))\].*?(?=## \[|\z)"
$match = [regex]::Match($changelog, $pattern)
if ($match.Success) {
    $notes = $match.Value.Trim()
} else {
    $notes = "Release $Tag"
}

# 8. Create GitHub release
Write-Host "Creating GitHub release ..." -ForegroundColor Green
$ghArgs = @("release","create",$Tag,$zipPath,"--title","HTRK $Tag","--notes",$notes)
if ($Draft) { $ghArgs += "--draft" }
& gh @ghArgs

if ($LASTEXITCODE -eq 0) {
    Write-Host "Done! Release published at:" -ForegroundColor Green
    & gh release view $Tag --web
} else {
    Write-Host "gh release create failed. You can create it manually at:" -ForegroundColor Yellow
    Write-Host "  https://github.com/clinta715/htrk/releases/new?tag=$Tag"
    Write-Host "Upload $zipName as the release asset."
}

Write-Host "`n=== Release complete ===" -ForegroundColor Cyan
