#!/bin/bash

# Error handling
set -euo pipefail

# Usage function
usage() {
    echo "Usage: $0 <MODEL_NAME>"
    echo "Example: $0 chronos_default"
    echo ""
    echo "Available models:"
    echo "  chronos_default"
    echo "  fast_statistical"
    echo "  balanced_ml"
    echo "  deep_learning"
    echo "  autoets_only"
    echo "  npts_only"
    echo "  seasonal_naive_only"
    echo "  recursive_tabular_only"
    echo "  ets_only"
    echo "  chronos_zero_shot"
    exit 1
}

# Check arguments
if [ $# -ne 1 ]; then
    usage
fi

MODEL_NAME="$1"

# Set CLI command path (release build)
cmd="$(dirname $0)/../target/release/cli_tokens"

# Set base directory
export CLI_TOKENS_BASE_DIR="${CLI_TOKENS_BASE_DIR:-$(pwd)}"

echo "=== Top Predict Script Start ==="
echo "Model: $MODEL_NAME"
echo "Base directory: $CLI_TOKENS_BASE_DIR"
echo ""

# Clean up existing directories
echo "Cleaning up existing directories..."
rm -rf "$CLI_TOKENS_BASE_DIR/tokens"
rm -rf "$CLI_TOKENS_BASE_DIR/history"
rm -rf "$CLI_TOKENS_BASE_DIR/predictions"
rm -rf "$CLI_TOKENS_BASE_DIR/charts"
echo "✓ Cleanup completed"
echo ""

# 1. Get high volatility tokens
echo "1. Fetching high volatility tokens..."
$cmd top
echo "✓ Token fetch completed"
echo ""

# Get token file list
TOKEN_FILES=($(find "$CLI_TOKENS_BASE_DIR/tokens" -name "*.json" -type f))

if [ ${#TOKEN_FILES[@]} -eq 0 ]; then
    echo "Error: No token files found"
    exit 1
fi

echo "Number of tokens fetched: ${#TOKEN_FILES[@]}"
echo ""

# 2. Run history command for all tokens
echo "2. Fetching price history for all tokens..."
for token_file in "${TOKEN_FILES[@]}"; do
    echo "  Processing history for: $(basename "$token_file")"
    $cmd history "$token_file"
done
echo "✓ All price history fetch completed"
echo ""

# 3. Run predict kick for all tokens
echo "3. Starting prediction tasks for all tokens..."
for token_file in "${TOKEN_FILES[@]}"; do
    echo "  Starting prediction for: $(basename "$token_file")"
    $cmd predict kick "$token_file" --model "$MODEL_NAME" --end-pct 90
done
echo "✓ All prediction tasks started"
echo ""

# 4. Run predict pull for all tokens in parallel
echo "4. Fetching prediction results in parallel..."
pids=()
for token_file in "${TOKEN_FILES[@]}"; do
    echo "  Starting pull for: $(basename "$token_file")"
    (
        $cmd predict pull "$token_file" --poll-interval 60 --max-polls 30
        echo "  ✓ Pull completed for: $(basename "$token_file")"
    ) &
    pids+=($!)
done

# Wait for all predict pull processes to complete
echo "Waiting for all prediction pulls to complete..."
for pid in "${pids[@]}"; do
    wait $pid
done
echo "✓ All prediction results fetched"
echo ""

# 5. Run chart command for all tokens
echo "5. Generating charts for all tokens..."
for token_file in "${TOKEN_FILES[@]}"; do
    echo "  Generating chart for: $(basename "$token_file")"
    $cmd chart "$token_file" --chart-type combined --show-confidence
done
echo "✓ All charts generated"
echo ""

echo "=== Top Predict Script Completed ==="
echo "All token processing completed."
echo ""
echo "Results:"
echo "  Tokens: $CLI_TOKENS_BASE_DIR/tokens/"
echo "  History: $CLI_TOKENS_BASE_DIR/history/"
echo "  Predictions: $CLI_TOKENS_BASE_DIR/predictions/"
echo "  Charts: $CLI_TOKENS_BASE_DIR/charts/"