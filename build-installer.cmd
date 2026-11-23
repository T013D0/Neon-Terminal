@echo off
rem Compila Neon y su instalador. Resultado: dist\NeonSetup.exe
cd /d "%~dp0"
echo Compilando Neon...
cargo build --release -p neon || goto :error
echo Compilando el instalador...
cargo build --release -p neon-setup || goto :error
if not exist dist mkdir dist
copy /Y target\release\NeonSetup.exe dist\NeonSetup.exe >nul
echo.
echo Listo: dist\NeonSetup.exe
goto :eof
:error
echo.
echo Fallo la compilacion. Revisa el error de arriba.
exit /b 1
