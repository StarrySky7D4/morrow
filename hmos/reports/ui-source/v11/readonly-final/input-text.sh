#!/bin/sh
value=$(printf '%s' 'SE1PUy1tYXJrZG93bi0yMDI2MTAwNS1B'|base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
