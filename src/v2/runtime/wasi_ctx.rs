use super::capabilities::{Capability, CapabilitySet};
use cap_std::time::Duration as CapDuration;
use std::path::PathBuf;
use wasmtime_wasi::{
    DirPerms, FilePerms, HostMonotonicClock, HostWallClock, ResourceTable, WasiCtx,
    WasiCtxBuilder as WasmtimeWasiCtxBuilder,
};

pub struct WasiCtxBuilder {
    args: Vec<String>,
    env: Vec<(String, String)>,
    stdin: Option<Vec<u8>>,
    preopens: Vec<(PathBuf, DirPerms, FilePerms)>,
    inherit_stdout: bool,
    inherit_stderr: bool,
    inherit_stdin: bool,
    allow_clock: bool,
    allow_random: bool,
}

impl Default for WasiCtxBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WasiCtxBuilder {
    pub fn new() -> Self {
        Self {
            args: vec![],
            env: vec![],
            stdin: None,
            preopens: vec![],
            inherit_stdout: false,
            inherit_stderr: false,
            inherit_stdin: false,
            allow_clock: false,
            allow_random: false,
        }
    }

    pub fn from_capabilities(caps: &CapabilitySet) -> Self {
        let mut builder = Self::new();

        for cap in caps.iter() {
            match cap {
                Capability::Stdout => builder.inherit_stdout = true,
                Capability::Stderr => builder.inherit_stderr = true,
                Capability::Stdin => builder.inherit_stdin = true,
                Capability::Clock => builder.allow_clock = true,
                Capability::Random => builder.allow_random = true,
                Capability::FileRead(path) => {
                    builder
                        .preopens
                        .push((path.clone(), DirPerms::READ, FilePerms::READ));
                }
                Capability::FileWrite(path) => {
                    builder.preopens.push((
                        path.clone(),
                        DirPerms::READ | DirPerms::MUTATE,
                        FilePerms::READ | FilePerms::WRITE,
                    ));
                }
                Capability::DirRead(path) => {
                    builder
                        .preopens
                        .push((path.clone(), DirPerms::READ, FilePerms::READ));
                }
                Capability::DirCreate(path) => {
                    builder.preopens.push((
                        path.clone(),
                        DirPerms::READ | DirPerms::MUTATE,
                        FilePerms::READ | FilePerms::WRITE,
                    ));
                }
                Capability::Args => {}
                Capability::Cwd => {}
                Capability::EnvRead(var) => {
                    if let Ok(val) = std::env::var(var) {
                        builder.env.push((var.clone(), val));
                    }
                }
                Capability::EnvReadAll => {
                    for (k, v) in std::env::vars() {
                        builder.env.push((k, v));
                    }
                }
                Capability::Unrestricted => {
                    builder.inherit_stdout = true;
                    builder.inherit_stderr = true;
                    builder.inherit_stdin = true;
                    builder.allow_clock = true;
                    builder.allow_random = true;
                    if let Ok(cwd) = std::env::current_dir() {
                        builder.preopens.push((
                            cwd,
                            DirPerms::READ | DirPerms::MUTATE,
                            FilePerms::READ | FilePerms::WRITE,
                        ));
                    }
                }
                _ => {}
            }
        }

        builder
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = env;
        self
    }

    pub fn with_stdin(mut self, stdin: Vec<u8>) -> Self {
        self.stdin = Some(stdin);
        self
    }

    pub fn with_preopens(mut self, preopens: Vec<(PathBuf, DirPerms, FilePerms)>) -> Self {
        self.preopens = preopens;
        self
    }

    pub fn build(self) -> wasmtime::Result<WasiCtx> {
        let mut builder = WasmtimeWasiCtxBuilder::new();

        builder.args(&self.args);

        for (k, v) in &self.env {
            builder.env(k, v);
        }

        if self.inherit_stdout {
            builder.inherit_stdout();
        }
        if self.inherit_stderr {
            builder.inherit_stderr();
        }
        if self.inherit_stdin {
            builder.inherit_stdin();
        }

        for (path, dir_perms, file_perms) in &self.preopens {
            if path.exists() {
                builder.preopened_dir(
                    path,
                    path.to_string_lossy().as_ref(),
                    *dir_perms,
                    *file_perms,
                )?;
            }
        }

        if !self.allow_random {
            builder.secure_random(ZeroRng);
            builder.insecure_random(ZeroRng);
            builder.insecure_random_seed(0);
        }

        if !self.allow_clock {
            builder.wall_clock(ZeroWallClock);
            builder.monotonic_clock(ZeroMonotonicClock);
        }

        Ok(builder.build())
    }
}

pub struct WasiHostState {
    pub wasi: WasiCtx,
    pub table: ResourceTable,
    pub stdout_buffer: Vec<u8>,
    pub stderr_buffer: Vec<u8>,
    pub fuel_remaining: Option<u64>,
}

impl WasiHostState {
    pub fn new(wasi: WasiCtx, fuel: Option<u64>) -> Self {
        Self {
            wasi,
            table: ResourceTable::new(),
            stdout_buffer: Vec::new(),
            stderr_buffer: Vec::new(),
            fuel_remaining: fuel,
        }
    }
}

impl wasmtime_wasi::WasiView for WasiHostState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
}

#[derive(Clone, Copy)]
struct ZeroWallClock;

impl HostWallClock for ZeroWallClock {
    fn resolution(&self) -> CapDuration {
        CapDuration::from_secs(0)
    }

    fn now(&self) -> CapDuration {
        CapDuration::from_secs(0)
    }
}

#[derive(Clone, Copy)]
struct ZeroMonotonicClock;

impl HostMonotonicClock for ZeroMonotonicClock {
    fn resolution(&self) -> u64 {
        0
    }

    fn now(&self) -> u64 {
        0
    }
}

struct ZeroRng;

impl rand_core::RngCore for ZeroRng {
    fn next_u32(&mut self) -> u32 {
        0
    }

    fn next_u64(&mut self) -> u64 {
        0
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        dest.fill(0);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        dest.fill(0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasi_ctx_builder_default() {
        let builder = WasiCtxBuilder::new();
        assert!(builder.args.is_empty());
        assert!(!builder.inherit_stdout);
    }

    #[test]
    fn test_wasi_ctx_from_capabilities() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::Stdout);
        caps.grant(Capability::Clock);

        let builder = WasiCtxBuilder::from_capabilities(&caps);
        assert!(builder.inherit_stdout);
        assert!(builder.allow_clock);
    }
}
