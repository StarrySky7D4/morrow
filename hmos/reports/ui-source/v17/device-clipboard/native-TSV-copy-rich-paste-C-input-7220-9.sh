#!/bin/sh
value=$(printf '%s' 'TmFtZQlWYWx1ZQpBbHBoYQkxMjM=' | base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
