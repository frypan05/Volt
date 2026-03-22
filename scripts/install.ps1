$ErrorActionPreference = "Stop"

$Owner = "frypan05"
$Repo = "Volt"
$Binary = "volt.exe"
$InstallDir = if ($env:DIR) { $env:DIR } else { "$env:USERPROFILE\.local\bin" }

# Get latest release
$Release = Invoke-RestMethod "https://api.github.com/repos/$Owner/$Repo/releases/latest"
$Version = $Release.tag_name

$Filename = "volt-x86_64-pc-windows-msvc.zip"
$Url = "https://github.com/$Owner/$Repo/releases/download/$Version/$Filename"

Write-Host "Installing volt $Version..."
Write-Host "Downloading from $Url"

$Tmp = New-TemporaryFile | ForEach-Object { Remove-Item $_; New-Item -ItemType Directory -Path "$_.dir" }
Invoke-WebRequest -Uri $Url -OutFile "$Tmp\$Filename"
Expand-Archive -Path "$Tmp\$Filename" -DestinationPath $Tmp

# Install
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir | Out-Null
}
Move-Item -Force "$Tmp\volt.exe" "$InstallDir\$Binary"
Remove-Item -Recurse -Force $Tmp

# Add to PATH if not already there
$CurrentPath = [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::User)
if ($CurrentPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable(
        "Path",
        "$CurrentPath;$InstallDir",
        [EnvironmentVariableTarget]::User
    )
    Write-Host "Added $InstallDir to your user PATH."
}

Write-Host ""
Write-Host "Done. Open a new terminal and run: volt"
