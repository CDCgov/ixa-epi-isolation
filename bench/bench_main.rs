use criterion::{criterion_group, criterion_main};

mod benchmarks;
use benchmarks::rate_functions::constant_rate_benchmarks;

criterion_group!(rate_benches, constant_rate_benchmarks,);

criterion_main!(rate_benches);
