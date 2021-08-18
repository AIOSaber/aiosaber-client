#!/bin/bash
cd ~/.aiosaber/client/ || exit
./serviceman-linux stop aiosaber-client
cd ~ || exit
pkill -f "serviceman.aiosaber-client"
pkill -f "aiosaber-client"
# Does it link its file somewhere? Gotta figure...
rm -f ~/.local/opt/serviceman/bin/serviceman.aiosaber-client
rm -f ~/.local/opt/serviceman/etc/aiosaber-client.json
rm -rf ~/.aiosaber