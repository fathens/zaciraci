#!/bin/bash

# 環境変数を設定
export CLI_TOKENS_BASE_DIR="./cli_tokens/.work"

echo "=== cli_tokens predict kick/pull サブコマンドのテスト ==="
echo ""

# ヘルプを表示
echo "1. predictコマンドのヘルプ:"
cargo run -- predict --help
echo ""

echo "2. kickサブコマンドのヘルプ:"
cargo run -- predict kick --help
echo ""

echo "3. pullサブコマンドのヘルプ:"
cargo run -- predict pull --help
echo ""

# サンプルトークンファイルの確認
SAMPLE_TOKEN_FILE="$CLI_TOKENS_BASE_DIR/tokens/wrap.near/linear-protocol.near.json"

if [ -f "$SAMPLE_TOKEN_FILE" ]; then
    echo "4. サンプルでkickコマンドを実行:"
    echo "   cargo run -- predict kick $SAMPLE_TOKEN_FILE"
    
    # 実際に実行する場合は以下のコメントを外す
    # cargo run -- predict kick "$SAMPLE_TOKEN_FILE"
    
    echo ""
    echo "5. pullコマンドの例:"
    echo "   cargo run -- predict pull $SAMPLE_TOKEN_FILE"
else
    echo "サンプルトークンファイルが見つかりません: $SAMPLE_TOKEN_FILE"
    echo "以下のコマンドを先に実行してください:"
    echo "  cargo run -- top -l 5"
    echo "  cargo run -- history tokens/wrap.near/[token-name].json"
fi

echo ""
echo "=== 並列実験の例 ==="
echo ""
echo "# 異なるモデルで並列実行"
echo "cargo run -- predict kick tokens/wrap.near/sample.token.near.json --model chronos-small --output predictions/small"
echo "cargo run -- predict kick tokens/wrap.near/sample.token.near.json --model chronos-large --output predictions/large"
echo ""
echo "# 結果を取得"
echo "cargo run -- predict pull tokens/wrap.near/sample.token.near.json --output predictions/small"
echo "cargo run -- predict pull tokens/wrap.near/sample.token.near.json --output predictions/large"