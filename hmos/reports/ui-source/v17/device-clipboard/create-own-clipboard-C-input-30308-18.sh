#!/bin/sh
value=$(printf '%s' 'SE1PUy1jbGlwYm9hcmQtMjAyNjEwMDctQw==' | base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
