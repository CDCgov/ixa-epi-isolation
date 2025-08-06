#!/bin/bash
input_base=$1

pop_sizes=("10000" "50000")
cargo build --profile profiling

for p in ${pop_sizes[@]}; do
    echo $p
    sed -r "s/^(.*synth_population_file.*):(.*)/\1:\"input\/synth_pop_people_WY_${p}.csv\",/g" ${input_base} > input/input_benchmark_state_$p.json
    mkdir -p output/$p
    samply record --save-only --output profile_$p.json.gz -- ./target/profiling/epi-isolation -c input/input_benchmark_state_$p.json -o output/$p -f
done

rm input/input_benchmark_state_*.json
