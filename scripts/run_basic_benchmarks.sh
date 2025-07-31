#!/bin/bash
input_base=$1

pop_sizes=("250000" "500000" "1000000")

for p in ${pop_sizes[@]}; do
    echo $p
    sed -r "s/^(.*synth_population_file.*):(.*)/\1:\"input\/synth_pop_people_WY_${p}.csv\",/g" ${input_base} > input/input_state_$p.json
    mkdir -p output/$p
    samply record --save-only --output profile_$p.json.gz -- ./target/profiling/epi-isolation -c input/input_state_$p.json -o output/$p -f 
done

