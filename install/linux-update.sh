#!/bin/bash
cd utility || exit
./serviceman-linux stop aiosaber-client
pkill -f "serviceman.aiosaber-client"
pkill -f "aiosaber-client"
cd .. || exit
cp aiosaber-client ~/.aiosaber/client/
cd ~/.aiosaber/client/ || exit
chmod +x aiosaber-client
./serviceman-linux start aiosaber-client