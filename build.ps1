# Add MSVC tools and libraries to PATH and LIB
$msvcBin = "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\MSVC\14.32.31326\bin\HostX64\x64"
$msvcLib = "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\MSVC\14.32.31326\lib\x64"
$sdkLib = "C:\Program Files (x86)\Windows Kits\10\lib\10.0.22621.0\ucrt\x64"
$sdkLib2 = "C:\Program Files (x86)\Windows Kits\10\lib\10.0.22621.0\um\x64"

$env:PATH = "$msvcBin;$env:PATH"
$env:PATH = $env:PATH -replace [regex]::Escape('C:\Program Files\Git\usr\bin;'), ''
$env:LIB = "$msvcLib;$sdkLib;$sdkLib2"

# Build
cargo build 2>&1