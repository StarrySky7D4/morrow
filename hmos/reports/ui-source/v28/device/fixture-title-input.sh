#!/bin/sh
value=$(printf '%s' 'SE1PUy12MjgtMjAyNjEwMDktQQ==' | base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
