# Lyra Builtin Commands Reference

## Overview

Lyra now includes 31 builtin commands, replicating most common busybox utilities.

## Command Categories

### Basic Shell Commands (8)
- `echo` - print text
- `ls` - list files (supports `-a`, `-l` flags)
- `cd` - change directory
- `pwd` - print working directory
- `exit` - exit the shell
- `which` - find command location
- `clear` - clear the screen
- `reset` - reset the terminal

### File Operations (6)
- `cat` - concatenate and display files
  - Flags: `-n` (line numbers), `-E` (show line ends)
- `cp` - copy files and directories
  - Flags: `-r` (recursive), `-f` (force), `-v` (verbose)
- `mv` - move/rename files
  - Flags: `-f` (force), `-v` (verbose)
- `rm` - remove files or directories
  - Flags: `-r` (recursive), `-f` (force), `-v` (verbose)
- `mkdir` - make directories
  - Flags: `-p` (parents), `-v` (verbose)
- `touch` - create empty files or update timestamps

### Text Utilities (6)
- `grep` - search text using patterns
  - Flags: `-i` (ignore case), `-v` (invert), `-n` (line numbers), `-c` (count)
- `head` - output first part of files
  - Flags: `-n <N>` (number of lines, default 10)
- `tail` - output last part of files
  - Flags: `-n <N>` (number of lines, default 10)
- `wc` - word, line, character, and byte count
  - Flags: `-l` (lines), `-w` (words), `-c` (chars), `-m` (bytes)
- `sort` - sort lines of text
  - Flags: `-r` (reverse), `-u` (unique), `-n` (numeric)
- `uniq` - report or omit repeated lines
  - Flags: `-c` (count), `-d` (duplicates only), `-u` (unique only)

### System Utilities (11)
- `env` - display environment variables
- `basename` - strip directory from filenames
- `dirname` - strip last component from file name
- `sleep` - delay for a specified amount of time
- `date` - display system date and time
  - Flags: `-f <format>` (custom format string)
- `true` - return success (exit code 0)
- `false` - return failure (exit code 1)
- `whoami` - print effective user name
- `uname` - print system information
  - Flags: `-a` (all), `-s` (kernel name), `-n` (nodename), `-r` (release), `-v` (version), `-m` (machine)

## Examples

### File Operations
```bash
# Create and view files
λ touch newfile.txt
λ echo "Hello, SOL!" > test.txt
λ cat test.txt
Hello, SOL!

# Copy and move
λ cp test.txt backup.txt
λ mv backup.txt archive.txt

# Create directories
λ mkdir -p dir1/dir2/dir3
λ ls dir1
```

### Text Processing
```bash
# Search in files
λ grep "error" logfile.txt
λ grep -i "ERROR" logfile.txt  # Case insensitive

# View parts of files
λ head -n 5 file.txt
λ tail -n 10 file.txt

# Count words/lines
λ wc file.txt

# Sort and deduplicate
λ sort file.txt
λ sort file.txt | uniq
```

### System Info
```bash
# User info
λ whoami
rownix

# System info
λ uname -a
linux hostname 6.0.0 #1 SMP x86_64

# Current date
λ date
Fri Aug 29 12:34:56 PDT 2026

# Environment
λ env
PATH=/usr/bin:/bin
HOME=/home/rownix
...

# Path utilities
λ basename /path/to/file.txt
file.txt

λ dirname /path/to/file.txt
/path/to
```

### Scripting
```bash
# Conditional execution
λ if true { echo "Success!" }
Success!

# Timing
λ sleep 2  # Wait 2 seconds
```

## Implementation Details

All commands follow the busybox reference implementation at `/home/rownix/Downloads/busybox-1.38.0/`:

- **coreutils**: cat, cp, mv, rm, mkdir, touch, wc, sort, uniq, basename, dirname, env, date, true, false, uname
- **findutils**: grep (basic implementation)
- **System utilities**: whoami, sleep

Commands support standard flags and behave similarly to their GNU/busybox counterparts.

## Still To Implement (Phase 3+)

### High Priority
- `find` - find files by pattern
- `xargs` - build command lines from input
- `cut` - extract columns from text
- `tr` - translate characters
- `tee` - split output to file and stdout
- `ln` - create symbolic/hard links
- `chmod` / `chown` - change permissions/ownership

### Medium Priority  
- `df` / `du` - disk usage
- `stat` - detailed file information
- `seq` - generate sequences
- `yes` - output string repeatedly
- `printenv` - print environment variable
- `mktemp` - create temporary file/directory

### Lower Priority
- `od` - octal dump
- `nl` - number lines
- `paste` - merge lines
- `expand` / `fold` - text formatting
- `comm` - compare sorted files

## Testing

Run the test script to verify all commands:
```bash
./test_commands.sh
```

Or test manually:
```bash
cargo run -p lyra
λ which cat
cat: shell built-in command
λ echo "test" | cat
test
```
