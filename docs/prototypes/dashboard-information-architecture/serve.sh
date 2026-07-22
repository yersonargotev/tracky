#!/bin/sh
set -eu

cd "$(dirname "$0")"
printf '%s\n' 'PROTOTYPE: http://127.0.0.1:4173/?variant=A'
exec python3 -m http.server 4173 --bind 127.0.0.1
