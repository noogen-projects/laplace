# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - YYYY-MM-DD

### Added

- Response routing to wasm from services, split wasm messages into In/Out
- Lapp setting `application.autoload` to configure the lapp to load at Laplace startup or in lazy mode on request from lapp client part
- Lapp setting `application.data_dir` to configure data dir of lapp, "data" by default (the relative path will be inside the lapp directory)
- Display of errors in the client UI
- Make commands for checking and testing
- This changelog file

### Fixed

- It became possible to restart gossipsub service for lapp
- Fix WS close send error
- Improve services communication

### Changed

- Replace wasmer to wasmtime
- Use separated threads for server side wasm
- Lapp loading is now lazy by default (use `application.autoload` setting for change this)
- Update dependencies: borsh 1.1.0, yew 0.21.0, libp2p 0.52.4, wasmtime, etc.

### Removed

- Unneсessary locks of the lapp manager when handling lapp requests
