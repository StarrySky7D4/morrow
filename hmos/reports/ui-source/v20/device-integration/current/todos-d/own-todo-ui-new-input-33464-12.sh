#!/bin/sh
value=$(printf '%s' 'SE1PUy10b2Rvcy0yMDI2MTAwNy1E' | base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
