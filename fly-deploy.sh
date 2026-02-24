#!/bin/bash
set -e

echo "=== Zaciraci Fly.io Deploy ==="

# 設定の検証
echo "Validating fly.toml..."
fly config validate

# 必須シークレットの確認
REQUIRED_SECRETS=("ROOT_ACCOUNT_ID" "ROOT_MNEMONIC" "PG_DSN")
MISSING=()

EXISTING_SECRETS=$(fly secrets list --json 2>/dev/null | grep -o '"Name":"[^"]*"' | cut -d'"' -f4)

for SECRET in "${REQUIRED_SECRETS[@]}"; do
    if ! echo "$EXISTING_SECRETS" | grep -q "^${SECRET}$"; then
        MISSING+=("$SECRET")
    fi
done

if [ ${#MISSING[@]} -gt 0 ]; then
    echo ""
    echo "WARNING: 以下の必須シークレットが未設定です:"
    for S in "${MISSING[@]}"; do
        echo "  - $S"
    done
    echo ""
    echo "設定方法:"
    echo "  fly secrets set ROOT_ACCOUNT_ID=\"your-account.near\""
    echo "  fly secrets set ROOT_MNEMONIC=\"your twelve word mnemonic\""
    echo "  fly secrets set PG_DSN=\"postgres://...\""
    echo ""
    read -p "続行しますか? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Git commit hash をビルド引数に渡してデプロイ
GIT_HASH=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
echo "Deploying commit: $GIT_HASH"

fly deploy --build-arg "GIT_COMMIT_HASH=$GIT_HASH"

echo ""
echo "=== Deploy complete ==="
fly status
