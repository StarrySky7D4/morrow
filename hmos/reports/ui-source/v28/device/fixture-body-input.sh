#!/bin/sh
value=$(printf '%s' 'Qm9keSDmsYnlrZcg8J+nqiBlzIEuClNlY29uZCBsaW5l' | base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
