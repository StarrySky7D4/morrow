#!/bin/sh
value=$(printf '%s' 'SE1PUy1hdHRhY2htZW50LTIwMjYxMDA1LUE='|base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
