#!/bin/bash
set -e

FEATURE_NAME="$1"

if [ -z "$FEATURE_NAME" ]; then
    echo "Usage: $0 <feature-name>"
    echo "Example: $0 user-authentication"
    exit 1
fi

echo "🎯 Creating PRD for: $FEATURE_NAME"

# Create PRD using Kiro CLI
kiro-cli chat --no-interactive --trust-all-tools "$(cat .kiro/artifacts/prds/create-prd.md)" "$FEATURE_NAME"

echo ""
echo "✅ PRD created! Next steps:"
echo ""
echo "1. Review the PRD:"
echo "   cat .kiro/artifacts/prds/$FEATURE_NAME/prd.json"
echo ""
echo "2. Start Ralph loop:"
echo "   cd .kiro/artifacts/prds/$FEATURE_NAME"
echo "   ./ralph.sh 25"
echo ""
echo "3. Monitor with Shards:"
echo "   shards start ralph-$FEATURE_NAME \"cd .kiro/artifacts/prds/$FEATURE_NAME && ./ralph.sh 25\""
