@echo off
rem Tiny test job for Crontab — no dependencies (cmd built-ins only).
rem Appends a timestamped line to heartbeat.log next to this .bat
rem and echoes the same line to stdout (caught by Crontab's per-job log).
rem
rem Use in a job's command:
rem   "C:\Users\hp\Documents\claude\cron\examples\heartbeat.bat"
rem
rem Try schedule:  * * * * *   (every minute)

set "LINE=[%date% %time%] heartbeat ok pid=%RANDOM%"
echo %LINE% >> "%~dp0heartbeat.log"
echo %LINE%
