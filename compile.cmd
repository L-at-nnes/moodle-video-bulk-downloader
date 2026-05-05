@echo off
setlocal

REM Build standalone Windows executable with icon and bundled tools
python -m pip install -r requirements.txt
python -m playwright install chromium

if exist build rmdir /s /q build
if exist dist rmdir /s /q dist
if exist moodle-video-bulk-downloader.spec del /q moodle-video-bulk-downloader.spec

pyinstaller ^
  --noconfirm ^
  --clean ^
  --onedir ^
  --name moodle-video-bulk-downloader ^
  --icon icon.ico ^
  --add-data "tools;tools" ^
  main.py

REM Copy additional tools to the dist folder
if not exist dist\moodle-video-bulk-downloader\tools mkdir dist\moodle-video-bulk-downloader\tools
copy /Y tools\ffmpeg.exe dist\moodle-video-bulk-downloader\tools\ 2>nul
copy /Y tools\mkvmerge.exe dist\moodle-video-bulk-downloader\tools\ 2>nul

echo.
echo Build complete.
echo EXE folder: dist\moodle-video-bulk-downloader\
echo Bundled with: ffmpeg.exe and mkvmerge.exe in tools\ subfolder

