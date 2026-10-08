#!/bin/bash
export PATH=/home/Debian/.cargo/bin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
export HOME=/home/Debian
cd /home/Debian/dm3/src-tauri
cargo test --lib 2>&1 | tail -8
