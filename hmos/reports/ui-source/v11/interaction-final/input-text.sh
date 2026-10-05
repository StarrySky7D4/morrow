#!/bin/sh
value=$(printf '%s' 'ZGV2MTEgc3RhYmxlIFRhc2tJZA=='|base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
