use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use run::v2::orchestrator::{OrchestratorConfig, create_orchestrator};
use run::v2::runtime::{CapabilitySet, RuntimeConfig, RuntimeEngine};
use std::sync::{Arc, Mutex};

fn minimal_wasm_bytes() -> Vec<u8> {
    // Minimal valid WASM module header. Enough for load/instantiation plumbing.
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

fn bench_cold_start(c: &mut Criterion) {
    c.bench_function("v2_cold_start_load", |b| {
        b.iter_batched(
            || minimal_wasm_bytes(),
            |bytes| {
                let mut engine = RuntimeEngine::new(RuntimeConfig::production()).unwrap();
                let _ = engine
                    .load_component_bytes("cold_component", bytes)
                    .unwrap();
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_warm_start_instantiate(c: &mut Criterion) {
    c.bench_function("v2_warm_start_instantiate", |b| {
        let mut engine = RuntimeEngine::new(RuntimeConfig::production()).unwrap();
        let component_id = engine
            .load_component_bytes("warm_component", minimal_wasm_bytes())
            .unwrap();
        let caps = CapabilitySet::service_default();

        b.iter(|| {
            let _ = engine.instantiate(&component_id, caps.clone()).unwrap();
        });
    });
}

fn bench_orchestrator_start_n(c: &mut Criterion) {
    c.bench_function("v2_orchestrator_start_50", |b| {
        b.iter_batched(
            || {
                let mut engine = RuntimeEngine::new(RuntimeConfig::production()).unwrap();
                for i in 0..50 {
                    let id = format!("svc_{i}");
                    let _ = engine
                        .load_component_bytes(&id, minimal_wasm_bytes())
                        .unwrap();
                }
                let runtime = Arc::new(Mutex::new(engine));
                let orchestrator =
                    create_orchestrator(Arc::clone(&runtime), OrchestratorConfig::default());
                (orchestrator, CapabilitySet::service_default())
            },
            |(orchestrator, caps)| {
                for i in 0..50 {
                    let id = format!("svc_{i}");
                    orchestrator.register(&id, vec![]).unwrap();
                }
                orchestrator.start_all(caps).unwrap();
                orchestrator.stop_all().unwrap();
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    benches,
    bench_cold_start,
    bench_warm_start_instantiate,
    bench_orchestrator_start_n
);
criterion_main!(benches);
