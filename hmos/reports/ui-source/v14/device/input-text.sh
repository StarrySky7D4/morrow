#!/bin/sh
value=$(printf '%s' 'IyBQdWJsaWMgbWVkaWEgZml4dHVyZQoKMTItc2Vjb25kIGdlbmVyYXRlZCBhdWRpby92aWRlbyBhbmQgYSBnZW5lcmF0ZWQgUERGLg=='|base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
