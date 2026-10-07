#!/bin/sh
value=$(printf '%s' 'Zmlyc3Qg5rGJ5a2XIPCfp6ogZcyBLg==' | base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
