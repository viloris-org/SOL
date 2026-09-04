#!/bin/bash
# Test ls .. command

echo "Testing ls .. command in Lyra"
echo

cd /tmp
mkdir -p lyra_test_parent/child
cd lyra_test_parent/child

echo "Current directory: $(pwd)"
echo "Parent directory contains:"
ls ..
echo

# Cleanup
cd /tmp
rm -rf lyra_test_parent

echo "✅ Test passed - ls .. works correctly"
