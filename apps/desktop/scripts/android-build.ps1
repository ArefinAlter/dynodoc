# Builds the debug APK on Windows, with or without the right to create symbolic links.
#
#   powershell -ExecutionPolicy Bypass -File scripts\android-build.ps1 [-Targets aarch64,x86_64] [-SplitPerAbi]
#
# Needs JAVA_HOME (JDK 17), ANDROID_HOME and NDK_HOME in the environment (docs/DESKTOP-AND-MOBILE-APPS.md
# lists the versions this was run with). Run it in apps/desktop after `npm ci --ignore-scripts`.
#
# WHY THIS EXISTS. `tauri android build` links the compiled Rust library into the Gradle project
# with a symbolic link, and Windows lets only an administrator, or any user once Developer Mode is
# on (Settings > System > For developers), create one. Without that right the Tauri command line
# compiles the library and then stops ("Creation symbolic link is not allowed for this system").
# With the right, this script just runs the normal command. Without it, the script does what the
# command line would have done: it lets the command line compile each target (and ignores that one
# failure), COPIES the library into app/src/main/jniLibs/<abi>/ and runs Gradle, skipping the
# rustBuild* tasks (they would run the same command line again and fail the same way). The APK is
# the same either way. Only the debug APK is handled; a release build and signing are the
# owner's step (docs/DESKTOP-AND-MOBILE-APPS.md).
param(
  [string[]]$Targets = @('aarch64', 'x86_64'),
  [switch]$SplitPerAbi,
  # A release APK (optimised, not debuggable). Signed only if gen/android/keystore.properties exists.
  [switch]$Release
)

$ErrorActionPreference = 'Stop'
$desktop = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$android = Join-Path $desktop 'src-tauri\gen\android'
$known = @{
  aarch64 = @{ triple = 'aarch64-linux-android'; abi = 'arm64-v8a'; arch = 'arm64'; task = 'Arm64' }
  armv7   = @{ triple = 'armv7-linux-androideabi'; abi = 'armeabi-v7a'; arch = 'arm'; task = 'Arm' }
  i686    = @{ triple = 'i686-linux-android'; abi = 'x86'; arch = 'x86'; task = 'X86' }
  x86_64  = @{ triple = 'x86_64-linux-android'; abi = 'x86_64'; arch = 'x86_64'; task = 'X86_64' }
}
foreach ($t in $Targets) { if (-not $known.ContainsKey($t)) { throw "unknown target '$t' (aarch64, armv7, i686, x86_64)" } }
foreach ($name in 'JAVA_HOME', 'ANDROID_HOME', 'NDK_HOME') {
  if (-not (Get-Item "env:$name" -ErrorAction SilentlyContinue)) { throw "$name is not set" }
}
if (-not (Test-Path (Join-Path $android 'gradlew.bat'))) {
  throw "no Android project at $android (run: npm run tauri -- android init)"
}

function Test-Symlink {
  $dir = Join-Path ([IO.Path]::GetTempPath()) ("dynodoc-symlink-" + [guid]::NewGuid())
  New-Item -ItemType Directory -Path $dir | Out-Null
  try {
    $file = Join-Path $dir 'a.txt'
    Set-Content -Path $file -Value 'x'
    New-Item -ItemType SymbolicLink -Path (Join-Path $dir 'b.txt') -Target $file -ErrorAction Stop | Out-Null
    return $true
  } catch {
    return $false
  } finally {
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
  }
}

Set-Location $desktop
$targetFlags = @('--target') + $Targets
$split = @(); if ($SplitPerAbi) { $split = @('--split-per-abi') }
$profileFlag = @('--debug'); $buildDir = 'debug'; $buildName = 'Debug'
if ($Release) { $profileFlag = @(); $buildDir = 'release'; $buildName = 'Release' }

if (Test-Symlink) {
  Write-Host 'android-build: symbolic links are allowed; running the normal command'
  & npm run tauri -- android build --ci @profileFlag --apk @targetFlags @split -- --locked
  if ($LASTEXITCODE -ne 0) { throw "tauri android build failed ($LASTEXITCODE)" }
  return
}

Write-Host 'android-build: symbolic links are NOT allowed here (enable Developer Mode to use the plain command); copying instead'
$cargoTarget = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $desktop 'src-tauri\target' }
foreach ($t in $Targets) {
  $info = $known[$t]
  $lib = Join-Path $cargoTarget "$($info.triple)\$buildDir\libdynodoc_desktop_lib.so"
  # Compile through the Tauri command line (it sets the NDK linker, the manifest and the Kotlin
  # sources up). Its last step, the link, is the part that cannot work here: that one failure is
  # expected; any other failure, or no "Finished" line from cargo, is not.
  $logFile = [IO.Path]::GetTempFileName()
  try {
    $flags = $profileFlag -join ' '
    cmd /c "npm run tauri -- android build --ci $flags --apk --target $t -- --locked > `"$logFile`" 2>&1"
    $exit = $LASTEXITCODE
    $log = Get-Content -Raw $logFile
  } finally {
    Remove-Item -Force $logFile -ErrorAction SilentlyContinue
  }
  if ($exit -ne 0 -and $log -notmatch 'symbolic link is not allowed') {
    Write-Host $log
    throw "compiling $t failed ($exit)"
  }
  if ($log -notmatch 'Finished' -or -not (Test-Path $lib)) {
    Write-Host $log
    throw "the $t library was not built: $lib"
  }
  $jni = Join-Path $android "app\src\main\jniLibs\$($info.abi)"
  New-Item -ItemType Directory -Force $jni | Out-Null
  Copy-Item -Force $lib $jni
  Write-Host "android-build: copied $($info.triple) library into jniLibs/$($info.abi)"
}

$tasks = @()
$skip = @()
foreach ($t in $Targets) { $skip += @('-x', "rustBuild$($known[$t].task)$buildName") }
$skip += @('-x', "rustBuildUniversal$buildName")
$propertyFlags = @()
if ($SplitPerAbi) {
  foreach ($t in $Targets) { $tasks += "assemble$($known[$t].task)$buildName" }
} else {
  $tasks += "assembleUniversal$buildName"
  $propertyFlags = @(
    "-PabiList=$(($Targets | ForEach-Object { $known[$_].abi }) -join ',')",
    "-ParchList=$(($Targets | ForEach-Object { $known[$_].arch }) -join ',')",
    "-PtargetList=$($Targets -join ',')"
  )
}
Set-Location $android
& (Join-Path $android 'gradlew.bat') @tasks @propertyFlags @skip --warn --no-daemon
if ($LASTEXITCODE -ne 0) { throw "gradle failed ($LASTEXITCODE)" }
Get-ChildItem (Join-Path $android 'app\build\outputs\apk') -Recurse -Filter '*.apk' | ForEach-Object { Write-Host "android-build: $($_.FullName) ($([math]::Round($_.Length / 1MB, 1)) MiB)" }
