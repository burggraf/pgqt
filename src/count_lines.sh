#!/bin/bash
find ~/dev/pgqt/src -name "*.rs" -exec wc -l {} \; | sort -nr

