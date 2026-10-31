# JSON WASM size probe (#142)

`json_wasm_size` links haecceity and its dependencies without quiddity or Specify.
Its exported `export_json` reads variable STEP input and serializes semantic and
As Drawn companions, keeping the export path reachable. `json-import` additionally
exports `import_json`, retaining parsing, validation and STEP writing. The functions
return output lengths; this is a size probe, not a supported client FFI.

Reproduce (default presentation feature, standard release profile, no wasm-opt):

```sh
cargo build -p haecceity --target wasm32-unknown-unknown --release --features json --example json_wasm_size
# Record target/wasm32-unknown-unknown/release/examples/json_wasm_size.wasm before rebuilding:
cargo build -p haecceity --target wasm32-unknown-unknown --release --features json-import --example json_wasm_size
```

Measured with rustc 1.99.0 (b940084d7, 2026-09-28), 2026-10-31:

| Features | Raw WASM bytes |
| --- | ---: |
| `json` | 5,884,717 |
| `json-import` | 8,350,505 |

The serializer-only artifact is **2,465,788 bytes (29.5%) smaller** in this probe.
Neither schema URN occurs in that artifact; both occur in the importer artifact.
These are not Specify application sizes and do not establish its final ceiling.

The export feature excludes the embedded schemas, runtime schema parsing and
validation, model deserialization, JSON parser/importer and ordering restoration.
A generated typed traversal replaces schema-tree walks and per-item schema clones.
Canonical identity JSON appends to one buffer instead of formatting/joining a
new allocation at each node; digest hex encoding also avoids general formatting.
The pre-split spindle fixture pins identical exported values and content ids.
