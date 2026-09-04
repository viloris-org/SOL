#!/bin/bash
# Quick demo of new Lyra commands

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  Lyra Shell - Phase 3: Core Commands Demonstration        ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo

# Prepare test environment
TEST_DIR="/tmp/lyra_demo_$$"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR" || exit 1

echo "📁 Test directory: $TEST_DIR"
echo

# Demo 1: File Operations
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "1️⃣  FILE OPERATIONS (cat, touch, mkdir, cp, mv, rm)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo

echo "Creating test files..."
echo "Hello from Lyra!" > greeting.txt
echo "This is line 1" > file1.txt
echo "This is line 2" >> file1.txt
echo "This is line 3" >> file1.txt
seq 1 100 > numbers.txt
mkdir -p subdir/nested

echo "✓ Files created"
echo
echo "Available commands:"
echo "  λ cat greeting.txt"
echo "  λ cp greeting.txt backup.txt"
echo "  λ mv backup.txt subdir/"
echo "  λ mkdir -p dir1/dir2/dir3"
echo "  λ touch newfile.txt"
echo

# Demo 2: Text Processing
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "2️⃣  TEXT PROCESSING (grep, head, tail, wc, sort, uniq)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo

cat > sample.txt << EOF
apple
banana
cherry
apple
date
banana
elderberry
fig
grape
apple
EOF

echo "Sample file created with fruits (some duplicates)"
echo
echo "Available commands:"
echo "  λ grep apple sample.txt          # Search for 'apple'"
echo "  λ grep -c apple sample.txt       # Count matches"
echo "  λ head -n 5 numbers.txt          # First 5 lines"
echo "  λ tail -n 5 numbers.txt          # Last 5 lines"
echo "  λ wc sample.txt                  # Word count"
echo "  λ sort sample.txt                # Sort alphabetically"
echo "  λ sort sample.txt | uniq         # Remove duplicates"
echo "  λ sort sample.txt | uniq -c      # Count occurrences"
echo

# Demo 3: System Utilities
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "3️⃣  SYSTEM UTILITIES (env, date, whoami, uname, basename, dirname)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo "Available commands:"
echo "  λ whoami                         # Current user"
echo "  λ date                           # Current date/time"
echo "  λ uname -a                       # System info"
echo "  λ env                            # Environment variables"
echo "  λ basename /path/to/file.txt     # → file.txt"
echo "  λ dirname /path/to/file.txt      # → /path/to"
echo "  λ sleep 2                        # Wait 2 seconds"
echo

# Demo 4: Pipelines
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "4️⃣  PIPELINE EXAMPLES"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo "  λ cat sample.txt | grep apple | wc -l"
echo "    → Count how many lines contain 'apple'"
echo
echo "  λ cat sample.txt | sort | uniq -c | sort -rn"
echo "    → Sort by frequency (most common first)"
echo
echo "  λ cat numbers.txt | head -n 20 | tail -n 5"
echo "    → Show lines 16-20"
echo
echo "  λ ls | grep txt | wc -l"
echo "    → Count .txt files"
echo

# Summary
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "📊 SUMMARY"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo "Total builtin commands: 31"
echo
echo "  ✓ Basic (8):     echo, ls, cd, pwd, exit, which, clear, reset"
echo "  ✓ File Ops (6):  cat, cp, mv, rm, mkdir, touch"
echo "  ✓ Text (6):      grep, head, tail, wc, sort, uniq"
echo "  ✓ System (11):   env, basename, dirname, sleep, date, true, false,"
echo "                   whoami, uname"
echo
echo "Start Lyra shell to try these commands:"
echo "  $ cargo run -p lyra"
echo "  λ cd $TEST_DIR"
echo "  λ cat greeting.txt"
echo "  λ grep apple sample.txt"
echo
echo "See COMMANDS.md for full reference"
echo

# Cleanup
read -p "Press Enter to cleanup test directory..."
cd /
rm -rf "$TEST_DIR"
echo "✓ Cleanup complete"
