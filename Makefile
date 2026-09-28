out := rust/target/wasm32-unknown-unknown/release
crates := $(patsubst rust/%/Cargo.toml,%,$(wildcard rust/*/Cargo.toml))
# struct is a Rust keyword, so its library builds under another name.
artifact = $(if $(filter struct,$(1)),edge_struct,$(1))

.PHONY: wasm
wasm:
	cargo build --release --target wasm32-unknown-unknown --manifest-path rust/Cargo.toml
	$(foreach crate,$(crates),cp $(out)/$(call artifact,$(crate)).wasm edge/$(crate)/$(crate).wasm;)
