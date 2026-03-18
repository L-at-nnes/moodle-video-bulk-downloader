@echo off
setlocal

REM Build standalone Windows executable with icon
python -m pip install -r requirements.txt
python -m playwright install chromium

if exist build rmdir /s /q build
if exist dist rmdir /s /q dist
if exist moodle-video-bulk-downloader.spec del /q moodle-video-bulk-downloader.spec

pyinstaller ^
  --noconfirm ^
  --clean ^
  --onefile ^
  --name moodle-video-bulk-downloader ^
  --icon icon.ico ^
  main.py

echo.
echo Build complete.
echo EXE: dist\moodle-video-bulk-downloader.exe
echo Keep tools\ffmpeg.exe and tools\mkvmerge.exe next to the EXE folder/project.
