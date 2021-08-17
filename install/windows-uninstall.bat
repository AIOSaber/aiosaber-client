cd "%USERPROFILE%\.aiosaber\client\"
serviceman-win.exe stop aiosaber-client
cd "%USERPROFILE%"
taskkill /f /im serviceman.aiosaber-client.exe
taskkill /f /im aiosaber-client.exe
reg delete HKEY_CURRENT_USER\SOFTWARE\Microsoft\Windows\CurrentVersion\Run /v aiosaber-client /f
del /q /f "%USERPROFILE%\.local\opt\serviceman\bin\serviceman.aiosaber-client.exe"
del /q /f "%USERPROFILE%\.local\opt\serviceman\etc\aiosaber-client.json"
rmdir /s /q "%USERPROFILE%\.aiosaber"