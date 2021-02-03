#!/bin/sh

app=${1}
mode=${2:+"--release"}
flags=${2:+"-Copt-level=s"}
out_dir=${2:-debug}

RUSTFLAGS="$flags -C lto=no -Z wasi-exec-model=reactor" cargo +nightly build -p ${app}_server --target wasm32-wasi $mode
mkdir -p daps/$app
cp target/wasm32-wasi/${out_dir}/${app}_server.wasm daps/$app
cp examples/$app/settings.toml daps/$app
