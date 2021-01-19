#!/bin/sh

app=${1}
mode=${2:+"--release"}
flags=${2:+"-Copt-level=s"}
out_dir=${2:-debug}

RUSTFLAGS=$flags cargo build -p ${app}_client --target wasm32-unknown-unknown $mode
wasm-bindgen --target web --no-typescript --out-dir daps/$app/static --out-name ${app}_client target/wasm32-unknown-unknown/${out_dir}/${app}_client.wasm