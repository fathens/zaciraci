#!/bin/bash

# Error handling
set -euo pipefail

# Function to get available models from API
get_available_models_with_descriptions() {
    local api_url="${ZCRC_API_URL:-http://localhost:8000}/api/v1/models"
    
    # Try to fetch models from API
    local models_json
    if ! models_json=$(curl -s --connect-timeout 5 "$api_url" 2>/dev/null); then
        echo "Warning: Could not fetch models from API ($api_url)" >&2
        return 1
    fi
    
    # Parse JSON and extract model names with descriptions
    local models_info
    if command -v jq >/dev/null 2>&1; then
        # Use jq if available
        models_info=$(echo "$models_json" | jq -r '.[] | "  \(.name) - \(.description)"' 2>/dev/null)
    else
        # Fallback to grep/sed if jq is not available
        models_info=$(echo "$models_json" | grep -o '"name":"[^"]*","version":"[^"]*","description":"[^"]*"' | sed 's/"name":"\([^"]*\)","version":"[^"]*","description":"\([^"]*\)"/  \1 - \2/')
    fi
    
    if [ -n "$models_info" ]; then
        echo "$models_info"
        return 0
    else
        return 1
    fi
}

# Usage function
usage() {
    echo "Usage: $0 <MODEL_NAME>"
    echo "Example: $0 chronos_default"
    echo ""
    echo "Available models:"
    
    local models_info
    if models_info=$(get_available_models_with_descriptions); then
        echo "$models_info"
    else
        echo "Error: Cannot retrieve models from API server. Please ensure the server is running."
        exit 1
    fi
    
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

# 2. Run history and predict kick in parallel
echo "2. Fetching price history and starting predictions as they complete..."
pids=()
for token_file in "${TOKEN_FILES[@]}"; do
    echo "  Processing history for: $(basename "$token_file")"
    (
        $cmd history "$token_file"
        echo "  ✓ History completed for: $(basename "$token_file")"
        echo "  Starting prediction for: $(basename "$token_file")"
        $cmd predict kick "$token_file" --model "$MODEL_NAME" --end-pct 90
        echo "  ✓ Prediction task started for: $(basename "$token_file")"
    ) &
    pids+=($!)
done

# Wait for all history and predict kick processes to complete
echo "Waiting for all history fetches and prediction kicks to complete..."
for pid in "${pids[@]}"; do
    wait $pid
done
echo "✓ All price history fetch and prediction tasks completed"
echo ""

# 4. Run predict pull and chart generation in parallel
echo "4. Fetching prediction results and generating charts as they complete..."
pids=()
for token_file in "${TOKEN_FILES[@]}"; do
    echo "  Starting pull for: $(basename "$token_file")"
    (
        $cmd predict pull "$token_file" --poll-interval 60 --max-polls 180
        echo "  ✓ Pull completed for: $(basename "$token_file")"
        echo "  Generating chart for: $(basename "$token_file")"
        $cmd chart "$token_file" --chart-type combined --show-confidence
        echo "  ✓ Chart generated for: $(basename "$token_file")"
    ) &
    pids+=($!)
done

# Wait for all predict pull and chart generation processes to complete
echo "Waiting for all prediction pulls and chart generations to complete..."
for pid in "${pids[@]}"; do
    wait $pid
done
echo "✓ All prediction results fetched and charts generated"
echo ""

echo "=== Top Predict Script Completed ==="
echo "All token processing completed."
echo ""
echo "Results:"
echo "  Tokens: $CLI_TOKENS_BASE_DIR/tokens/"
echo "  History: $CLI_TOKENS_BASE_DIR/history/"
echo "  Predictions: $CLI_TOKENS_BASE_DIR/predictions/"
echo "  Charts: $CLI_TOKENS_BASE_DIR/charts/"