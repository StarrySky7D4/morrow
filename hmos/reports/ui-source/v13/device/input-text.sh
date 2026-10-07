#!/bin/sh
value=$(printf '%s' 'SE1PUy1pbmxpbmUtMjAyNjEwMDctQQ=='|base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
