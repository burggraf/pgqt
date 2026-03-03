#!/bin/bash
find . -name "*.rs" -exec wc -l {} \; | sort -nr

