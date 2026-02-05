## Polyglot SDK Example

This example provides skeleton components for JavaScript, TypeScript, Go, and Zig.
It is designed to exercise the multi-language build pipeline.

### Structure

```
polyglot-sdk/
├── run.toml
├── wit/
│   └── greeter.wit
├── js/
│   ├── package.json
│   └── index.js
├── ts/
│   ├── package.json
│   ├── tsconfig.json
│   └── src/index.ts
├── go/
│   ├── go.mod
│   └── main.go
└── zig/
    ├── build.zig
    └── src/main.zig
```

### Build

```bash
run build
```

### Notes

- JS/TS use `jco componentize` under the hood.
- Go uses `tinygo` + `wasm-tools component new`.
- Zig uses `zig build` and copies the first `.wasm` output from `zig-out/`.
- The WIT file is included for interface design; for production, ensure your
  toolchain is configured to bind the WIT world to your language runtime.
