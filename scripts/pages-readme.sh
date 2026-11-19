#!/bin/sh
# Prints the README with each SVG diagram swapped for its mermaid-ascii
# rendering, for the pages site. Needs mermaid-ascii on the PATH.
set -eu
while IFS= read -r line; do
  case "$line" in
    '!['*'](docs/diagrams/'*'.svg)')
      name=${line##*docs/diagrams/}; name=${name%.svg)}
      echo '```'
      printf '%s\n' "$(mermaid-ascii -f "docs/diagrams/$name.mmd")"
      echo '```'
      ;;
    *) printf '%s\n' "$line" ;;
  esac
done < README.md
