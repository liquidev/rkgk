#!/usr/bin/env fish

set filename $argv[1]
set icon_name (basename $filename .svg)
set icon_base64 (svgcleaner --stdout $filename 2>/dev/null | base64 -w0)

printf "--icon-%s: url('data:image/svg+xml;base64,%s');" "$icon_name" "$icon_base64"
