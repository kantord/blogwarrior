#!/bin/sh
# QUERY_STRING is set by spudkit, e.g. id=abc123
ID=$(echo "$QUERY_STRING" | sed 's/.*id=\([^&]*\).*/\1/')
blog "id:$ID" read 2>/dev/null
