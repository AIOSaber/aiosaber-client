#!/bin/bash
cd utility || exit
./serviceman-mac stop aiosaber-client
pkill -f "serviceman.aiosaber-client"
pkill -f "aiosaber-client"
cd .. || exit
cp aiosaber-client ~/.aiosaber/client/
cd ~/.aiosaber/client/ || exit
chmod +x aiosaber-client
./serviceman-mac start aiosaber-client