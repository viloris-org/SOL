#!/bin/bash
# Test script for new Lyra builtin commands

echo "Testing Lyra builtin commands..."
echo

# Create test directory
TEST_DIR="/tmp/lyra_test_$$"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR" || exit 1

echo "=== Test Directory: $TEST_DIR ==="
echo

# File operations
echo "1. Testing cat, touch, mkdir..."
echo "Hello, SOL!" > test.txt
echo "Line 2" >> test.txt
echo "✓ Created test.txt"

mkdir testdir
echo "✓ Created testdir"

touch newfile.txt
echo "✓ Created newfile.txt"

# Text utilities
echo
echo "2. Testing head, tail, wc..."
seq 1 20 > numbers.txt
echo "✓ Created numbers.txt with 20 lines"

# System utilities
echo
echo "3. Testing basename, dirname, whoami..."
echo "Test files created:"
ls -la

# Cleanup function
cleanup() {
    echo
    echo "Cleaning up test directory..."
    cd /
    rm -rf "$TEST_DIR"
    echo "✓ Cleanup complete"
}

trap cleanup EXIT

echo
echo "=== All test files created successfully ==="
echo
echo "You can now test these commands in Lyra:"
echo "  cd $TEST_DIR"
echo "  cat test.txt"
echo "  head -n 5 numbers.txt"
echo "  tail -n 5 numbers.txt"
echo "  wc test.txt"
echo "  grep SOL test.txt"
echo "  sort numbers.txt"
echo "  basename /path/to/file.txt"
echo "  dirname /path/to/file.txt"
echo "  whoami"
echo "  date"
echo "  sleep 1"
echo
