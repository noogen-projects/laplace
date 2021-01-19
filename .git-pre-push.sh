#!/bin/sh -e

remote="$1"

if [ "$remote" = "origin" ]; then
  cargo +nightly fmt -- --check
  cargo clippy --all-targets --all-features -- -D warnings
fi
