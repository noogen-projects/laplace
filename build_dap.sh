#!/bin/sh

app=${1}
mode=${2:+"--release"}
flags=${2:+"-Copt-level=s"}
out_dir=${2:-debug}

RUSTFLAGS=$flags cargo build -p ${app}_server --target wasm32-unknown-unknown $mode
cp target/wasm32-unknown-unknown/${out_dir}/${app}_server.wasm daps/$app
