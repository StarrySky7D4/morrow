#!/bin/sh
value=$(printf '%s' 'QmVmb3JlIHwgQWZ0ZXI=' | base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
