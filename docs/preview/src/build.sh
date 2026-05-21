#!/usr/bin/env bash
# Regenerate all five ATD 技术预览 decks from source.
# Prerequisite (once):  npm install
set -euo pipefail
cd "$(dirname "$0")"

[ -d node_modules ] || { echo "run 'npm install' first"; exit 1; }

node deck1.js ../atd-preview-1-background.pptx
node deck2.js ../atd-preview-2-design-principles.pptx
node deck3.js ../atd-preview-3-architecture.pptx
node deck4.js ../atd-preview-4-adoption.pptx
node deck5.js ../atd-preview-5-roadmap.pptx

echo "5 decks regenerated into docs/preview/."
