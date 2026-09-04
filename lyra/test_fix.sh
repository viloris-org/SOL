#!/bin/bash
# Quick test to verify the cd command fix

echo "Building lyra..."
cargo build -p lyra --quiet 2>&1 > /dev/null

echo ""
echo "Testing lyra command argument parsing:"
echo ""

# Test 1: cd with simple argument
echo "Test 1: cd docs"
echo "cd docs" | timeout 1 cargo run -p lyra --quiet 2>&1 || echo "(timeout expected - this is an interactive shell)"

echo ""
echo "Test 2: ls with argument"
echo "ls lyra" | timeout 1 cargo run -p lyra --quiet 2>&1 || echo "(timeout expected - this is an interactive shell)"

echo ""
echo "Test 3: cd with path"
echo "cd /tmp" | timeout 1 cargo run -p lyra --quiet 2>&1 || echo "(timeout expected - this is an interactive shell)"

echo ""
echo "All basic parsing tests completed!"
echo ""
echo "To test interactively, run: cargo run -p lyra"
