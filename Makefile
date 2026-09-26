out := rust/target/wasm32-unknown-unknown/release
crates := $(patsubst rust/%/Cargo.toml,%,$(wildcard rust/*/Cargo.toml))

.PHONY: wasm
wasm:
	cargo build --release --target wasm32-unknown-unknown --manifest-path rust/Cargo.toml
	$(foreach crate,$(crates),cp $(out)/$(crate).wasm edge/$(crate)/;)
