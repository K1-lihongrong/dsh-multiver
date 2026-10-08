#!/bin/bash
export PATH=/usr/local/bin:/usr/local/sbin:/usr/sbin:/usr/bin:/sbin:/bin
export HOME=/home/Debian
cd /home/Debian/dm3
./node_modules/.bin/vite --host 127.0.0.1 --port 1420 > /tmp/v.log 2>&1
