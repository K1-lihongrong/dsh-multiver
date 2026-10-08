#!/bin/bash
export DISPLAY=:99
EDGE="/mnt/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe"
mkdir -p /mnt/e/.LLM-AGENTS/cuckoo-code-/deepseek-Desktop/dsh-multiver/docs/screenshots/theme
OUT=/mnt/e/.LLM-AGENTS/cuckoo-code-/deepseek-Desktop/dsh-multiver/docs/screenshots/theme
"$EDGE" --headless=new --disable-gpu --hide-scrollbars --window-size=1120,900 --virtual-time-budget=6000 --screenshot="$OUT/light.png" "http://127.0.0.1:1420/" 2>/dev/null
echo DONE
ls -la $OUT
