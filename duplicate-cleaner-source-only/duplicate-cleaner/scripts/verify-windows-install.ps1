# Smoke-test both Windows installer formats on a Windows runner.
# Run after `npm run tauri:build`; the hosted windows-latest runner is elevated.
$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$bundleRoot = Join-Path $root "src-tauri\target\release\bundle"
$msi = Get-ChildItem (Join-Path $bundleRoot "msi") -Filter "*.msi" |
  Select-Object -First 1
$nsis = Get-ChildItem (Join-Path $bundleRoot "nsis") -Filter "*.exe" |
  Where-Object { $_.Name -notmatch "uninstall" } |
  Select-Object -First 1

if (-not $msi) { throw "MSI installer not found under $bundleRoot\msi" }
if (-not $nsis) { throw "NSIS setup executable not found under $bundleRoot\nsis" }

$smokeRoot = Join-Path $env:TEMP "duplicate-cleaner-installer-smoke-$PID"
$msiDir = Join-Path $smokeRoot "msi"
$nsisDir = Join-Path $smokeRoot "nsis"
New-Item -ItemType Directory -Path $msiDir, $nsisDir -Force | Out-Null

function Invoke-CheckedProcess([string]$FilePath, [string[]]$Arguments, [int[]]$AllowedExitCodes = @(0)) {
  Write-Host "Running: $FilePath $($Arguments -join ' ')"
  $process = Start-Process -FilePath $FilePath -ArgumentList $Arguments -Wait -PassThru
  if ($AllowedExitCodes -notcontains $process.ExitCode) {
    throw "Process failed with exit code $($process.ExitCode): $FilePath"
  }
}

function Assert-InstalledApp([string]$InstallDir, [string]$Format) {
  $exe = Join-Path $InstallDir "duplicate-cleaner.exe"
  if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
    throw "$Format installer returned success but the application executable is missing: $exe"
  }
  if ((Get-Item -LiteralPath $exe).Length -lt 1MB) {
    throw "$Format installed executable is unexpectedly small: $exe"
  }
  Write-Host "$Format installation verified: $exe"
  return $exe
}

function Assert-Shortcuts([string]$ExePath, [string]$Format) {
  $roots = @(
    (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"),
    (Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs"),
    (Join-Path $env:USERPROFILE "Desktop"),
    (Join-Path $env:PUBLIC "Desktop")
  ) | Where-Object { Test-Path -LiteralPath $_ }

  $shell = New-Object -ComObject WScript.Shell
  $targets = @(
    foreach ($rootDir in $roots) {
      Get-ChildItem -LiteralPath $rootDir -Filter "*.lnk" -File -Recurse -ErrorAction SilentlyContinue |
        ForEach-Object {
          $link = $shell.CreateShortcut($_.FullName)
          if ($link.TargetPath -and
              [IO.Path]::GetFullPath($link.TargetPath) -ieq [IO.Path]::GetFullPath($ExePath)) {
            [PSCustomObject]@{ Path = $_.FullName; Target = $link.TargetPath }
          }
        }
    }
  )

  $menuLink = $targets | Where-Object {
    $_.Path -like "*\Start Menu\Programs\*"
  } | Select-Object -First 1
  $desktopLink = $targets | Where-Object {
    $_.Path -like "*\Desktop\*"
  } | Select-Object -First 1
  if (-not $menuLink) { throw "$Format install did not create a Start Menu shortcut for $ExePath" }
  if (-not $desktopLink) { throw "$Format install did not create a Desktop shortcut for $ExePath" }
  Write-Host "$Format shortcuts verified: $($menuLink.Path); $($desktopLink.Path)"
}

try {
  # Tauri's WiX bundle uses INSTALLDIR for its application directory.
  Invoke-CheckedProcess "msiexec.exe" @(
    "/i", "`"$($msi.FullName)`"", "/qn", "/norestart",
    "INSTALLDIR=`"$msiDir`""
  ) @(0, 3010)
  $msiExe = Assert-InstalledApp $msiDir "MSI"
  Assert-Shortcuts $msiExe "MSI"
  Invoke-CheckedProcess "msiexec.exe" @(
    "/x", "`"$($msi.FullName)`"", "/qn", "/norestart"
  ) @(0, 3010)
  if (Test-Path -LiteralPath (Join-Path $msiDir "duplicate-cleaner.exe")) {
    throw "MSI uninstall left the application executable behind: $msiDir"
  }
  Write-Host "MSI silent uninstall verified"

  # NSIS requires /D=... to be the final command-line argument.
  Invoke-CheckedProcess $nsis.FullName @("/S", "/D=$nsisDir")
  $nsisExe = Assert-InstalledApp $nsisDir "NSIS"
  Assert-Shortcuts $nsisExe "NSIS"
  $uninstaller = Join-Path $nsisDir "uninstall.exe"
  if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
    throw "NSIS uninstaller was not installed: $uninstaller"
  }
  Invoke-CheckedProcess $uninstaller @("/S")
  if (Test-Path -LiteralPath (Join-Path $nsisDir "duplicate-cleaner.exe")) {
    throw "NSIS uninstall left the application executable behind: $nsisDir"
  }
  Write-Host "NSIS silent uninstall verified"
}
finally {
  Remove-Item -LiteralPath $smokeRoot -Recurse -Force -ErrorAction SilentlyContinue
}
