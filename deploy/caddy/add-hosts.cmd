@echo off
rem Adds *.localhost entries to Windows hosts. Run elevated.
rem
rem Usage (from elevated CMD or PowerShell):
rem   deploy\caddy\add-hosts.cmd

set HOSTS=%SystemRoot%\System32\drivers\etc\hosts

findstr /c:"neurogrim-local.localhost" "%HOSTS%" >nul
if %errorlevel%==0 (
    echo hosts entries already present
    goto :eof
)

echo.>> "%HOSTS%"
echo # Added by NeuroGrim deploy/caddy/add-hosts.cmd>> "%HOSTS%"
echo 127.0.0.1 neurogrim-local.localhost>> "%HOSTS%"
echo 127.0.0.1 neurogrim-external.localhost>> "%HOSTS%"
echo 127.0.0.1 webhooks.localhost>> "%HOSTS%"

echo Added 3 entries. Flushing DNS cache...
ipconfig /flushdns >nul
echo Done.
