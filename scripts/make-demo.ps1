# Builds a demo folder of sparse files: large sizes, almost no disk use. For README screenshots.
param([string]$Root)
Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue
$list = @"
Videos\Trip-Ladakh-2025.mp4|18636
Videos\OBS Recordings\2026-09-12 21-04-11.mkv|10035
Videos\OBS Recordings\2026-08-30 19-22-50.mkv|8294
Videos\OBS Recordings\2026-07-02 22-10-05.mkv|7577
Videos\Family\Wedding-cut-4K.mov|8090
Videos\Family\Diwali-2024.mp4|3174
Downloads\ubuntu-24.04.3-desktop-amd64.iso|6246
Downloads\Win11_24H2_English_x64.iso|5939
Downloads\DaVinci_Resolve_19.1_Windows.zip|3482
Downloads\Installers\blender-4.2-windows-x64.msi|380
Downloads\Installers\node-v22.11.0-x64.msi|31
Games\Halo Infinite\Content\campaign_deploy_01.pak|9114
Games\Halo Infinite\Content\multiplayer_deploy_02.pak|7373
Games\Halo Infinite\Content\shared_textures.pak|6144
Games\Ori\ori.pak|4403
Music\Lossless\Album 1.flac|620
Music\Lossless\Album 2.flac|540
Documents\Taxes\2025-return.pdf|3
Documents\Thesis\thesis-final.docx|48
Documents\Archive\college-project-final (2).zip|1331
Projects\web-app\node_modules\.pnpm\esbuild.exe|10
Projects\web-app\node_modules\.pnpm\next-swc.node|147
Projects\web-app\package.json|1
Projects\rust-tool\target\debug\deps\librust_tool.rlib|2458
Projects\rust-tool\Cargo.toml|1
Projects\vms\dev-box.vhdx|15053
"@ -split "`r?`n" | Where-Object { $_ }
for ($i = 1; $i -le 60; $i++) { $list += "Pictures\Camera Roll\IMG_$((4000 + $i)).HEIC|$((3 + ($i % 5)))" }
for ($i = 1; $i -le 20; $i++) { $list += "Pictures\Lightroom\Export\DSC_$((1200 + $i)).jpg|$((12 + $i))" }
foreach ($line in $list) {
  $rel, $mb = $line -split '\|'
  $path = Join-Path $Root $rel
  New-Item -ItemType Directory -Force (Split-Path $path) | Out-Null
  New-Item -ItemType File -Force $path | Out-Null
  fsutil sparse setflag "$path" | Out-Null
  $fs = [IO.File]::Open($path, 'Open', 'Write'); $fs.SetLength([long]$mb * 1048576); $fs.Close()
}
"made $($list.Count) files"
