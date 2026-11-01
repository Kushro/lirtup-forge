#!/usr/bin/env fish

# ===================================================================
# Compile the GUI translations (.ts -> .qm)
# ===================================================================
# The .qm files are committed so that pip/pipx installs ship them
# without needing Qt's tools at build time. Run this after editing a .ts.

set root_dir (dirname (dirname (status --current-filename)))
set locale_dir "$root_dir/src/upscaler/gui/locale"

set lrelease (command -s pyside6-lrelease lrelease-qt6 lrelease)[1]
if test -z "$lrelease"
    echo "'lrelease' (or 'pyside6-lrelease') must be installed or in PATH."
    return 1
end

for ts in $locale_dir/*.ts
    echo "Compiling '$ts'..."
    $lrelease -silent "$ts" -qm (string replace -r '\.ts$' '.qm' "$ts")
end
