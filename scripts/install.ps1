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

$Tmp = [System.IO.Path]::GetTempPath() + [System.Guid]::NewGuid().ToString()
New-Item -ItemType Directory -Path $Tmp | Out-Null
Invoke-WebRequest -Uri $Url -OutFile "$Tmp\$Filename"
Expand-Archive -Path "$Tmp\$Filename" -DestinationPath $Tmp -Force

if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}
Copy-Item -Force "$Tmp\volt.exe" "$InstallDir\volt.exe"
Remove-Item -Recurse -Force $Tmp

# Write PATH directly to registry — works in all environments
$registryPath = "HKCU:\Environment"
$currentPath = (Get-ItemProperty -Path $registryPath -Name Path -ErrorAction SilentlyContinue).Path

if ($null -eq $currentPath) {
    Set-ItemProperty -Path $registryPath -Name Path -Value $InstallDir
    Write-Host "Created PATH entry with $InstallDir"
} elseif ($currentPath -notlike "*$InstallDir*") {
    Set-ItemProperty -Path $registryPath -Name Path -Value "$currentPath;$InstallDir"
    Write-Host "Added $InstallDir to PATH"
} else {
    Write-Host "$InstallDir already in PATH"
}

# Broadcast WM_SETTINGCHANGE so Explorer and new terminals pick up the change
# without needing a full reboot
$signature = @'
[DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Auto)]
public static extern IntPtr SendMessageTimeout(
    IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam,
    uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
$type = Add-Type -MemberDefinition $signature -Name WinAPI -Namespace Win32 -PassThru
$result = [UIntPtr]::Zero
$type::SendMessageTimeout(
    [IntPtr]0xffff, 0x001A, [UIntPtr]::Zero, "Environment",
    0x0002, 5000, [ref]$result) | Out-Null

Write-Host ""
Write-Host "Done. Open a NEW terminal window and run: volt"
