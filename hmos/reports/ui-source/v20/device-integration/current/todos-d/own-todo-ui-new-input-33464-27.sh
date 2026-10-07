#!/bin/sh
value=$(printf '%s' 'UHVibGljIEZsdXR0ZXIgdG9kbyBVSSBmaXh0dXJlOiDmsYnlrZcg8J+nqiBlzIEu' | base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
