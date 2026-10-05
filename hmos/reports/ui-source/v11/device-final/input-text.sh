#!/bin/sh
value=$(printf '%s' 'IyBSZXZpc2VkIEIKCktlcHQgKipzb3VyY2UqKiBhbmQg5Lit5paH8J+YgC4='|base64 -d; printf '.')
exec uitest uiInput text "${value%.}"
