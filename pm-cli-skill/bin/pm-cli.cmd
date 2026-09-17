@echo off
rem pm-cli launcher for Windows: forwards to pm-cli.mjs in the same directory.
rem Keep this file ASCII-only and CRLF-terminated; see AGENTS.md.
setlocal
node "%~dp0pm-cli.mjs" %*
set "pm_cli_code=%errorlevel%"
if not "%pm_cli_code%"=="9009" exit /b %pm_cli_code%
echo pm-cli requires Node.js 18 or higher. Install Node.js and retry. 1>&2
exit /b 3
