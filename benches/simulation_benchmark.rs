use criterion::{criterion_group, criterion_main, Criterion};
use std::time::Duration;
use rts_assignment::run_simulation; // Import from your library

fn benchmark_system_integration(c: &mut Criterion) {
    // Define a group to configure sample size if needed
    let mut group = c.benchmark_group("simulation_integration");

    // Reduce sample count because even 10ms takes time to run 100 times
    group.sample_size(50);

    group.bench_function("run_10ms_cycle", |b| {
        b.iter(|| {
            // Run a tiny simulation (10ms)
            // This tests thread spawn + a few sensor cycles + shutdown
            run_simulation(Duration::from_millis(10));
        })
    });

    group.finish();
}

criterion_group!(benches, benchmark_system_integration);
criterion_main!(benches);