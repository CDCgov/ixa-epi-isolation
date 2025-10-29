use criterion::Criterion;
use epi_isolation::rate_fns::{ConstantRate, InfectiousnessRateFn};
use std::hint::black_box;

pub fn constant_rate_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("rate_fns::constant_rate");
    let rate = ConstantRate::new(0.5, 14.0).expect("Valid constant rate parameters");

    // Test instantaneous rate evaluation
    group.bench_function("rate_early", |b| {
        b.iter(|| {
            black_box(rate.rate(black_box(1.0))); // Early in infection
        });
    });

    group.bench_function("rate_mid", |b| {
        b.iter(|| {
            black_box(rate.rate(black_box(7.0))); // Middle of infection period
        });
    });

    group.bench_function("rate_boundary", |b| {
        b.iter(|| {
            black_box(rate.rate(black_box(14.0))); // End of infection period
        });
    });

    group.bench_function("rate_post", |b| {
        b.iter(|| {
            black_box(rate.rate(black_box(15.0))); // After infection period
        });
    });

    // Test cumulative rate evaluation
    group.bench_function("cum_rate_early", |b| {
        b.iter(|| {
            black_box(rate.cum_rate(black_box(1.0))); // Early in infection
        });
    });

    group.bench_function("cum_rate_mid", |b| {
        b.iter(|| {
            black_box(rate.cum_rate(black_box(7.0))); // Middle of infection period
        });
    });

    group.bench_function("cum_rate_boundary", |b| {
        b.iter(|| {
            black_box(rate.cum_rate(black_box(14.0))); // End of infection period
        });
    });

    group.bench_function("cum_rate_post", |b| {
        b.iter(|| {
            black_box(rate.cum_rate(black_box(15.0))); // After infection period
        });
    });

    // Test inverse cumulative rate
    group.bench_function("inverse_cum_rate_low", |b| {
        b.iter(|| {
            black_box(rate.inverse_cum_rate(black_box(0.5))); // Should be possible (1 day at rate 0.5)
        });
    });

    group.bench_function("inverse_cum_rate_mid", |b| {
        b.iter(|| {
            black_box(rate.inverse_cum_rate(black_box(3.5))); // Should be possible (7 days at rate 0.5)
        });
    });

    group.bench_function("inverse_cum_rate_max", |b| {
        b.iter(|| {
            black_box(rate.inverse_cum_rate(black_box(7.0))); // Maximum possible value (14 days at rate 0.5)
        });
    });

    group.bench_function("inverse_cum_rate_impossible", |b| {
        b.iter(|| {
            black_box(rate.inverse_cum_rate(black_box(8.0))); // Should be impossible (requires 16 days at rate 0.5)
        });
    });

    group.finish();
}
