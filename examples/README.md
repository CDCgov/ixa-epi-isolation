The `examples` directory contains some simple exercises and best practices for using the
`abmwrappers` python package with `ixa-epi-isolation`.

## Getting started
### Poetry
In a newly cloned repo, be sure to initiate the poetry environment by running `poetry install`.
If a `poetry.lock` file already exists, it is recommended to remove the old file or to instead run `poetry lock`.
It is still possible that you will need to install modules manually if `poetry.lock` exists.
If packages are still missing (shown as inaccessible by code check or throwing an error on `poetry run ...`),
use `$(poetry env activate)` to activate the virtual environment.

### Rust and `cargo`
Build the current release binaries by executing `cargo build --release`

Ensure that you can run the release from the command line by running
```
target/release/epi-isolation -c input/input.json
```
Note that if the files already exist in the root directory, you will only be able to run the above
command by including the force overwrite argument `-f`.

### Rendering `quarto` docs
Install (quarto)[https://docs.posit.co/resources/install-quarto.html] and ensure that your poetry
environment is activated with `$(poetry env activate)`. Verify you can run quarto with `quarto check install`,
being sure to (add a symbolic link for quarto)[https://docs.posit.co/resources/install-quarto.html#add-symlink-quarto] if issues occur. 
In order to render documents, also be sure to install `tinytex` using `quarto install tinytex`.

To render a doc, use 

```
quarto render path/to/file/output.qmd
```

## Replicate core example
Here, we set up a replicate of the core example and then we plot the results in a quarto document.
An analogous python script is included to demonstrate use of the scripts file hierarchy.

This example serves to run 30 replicates of the `input/input.json` file and plot the counts by
infection status over time.

To run the example using only python, use the command

```
poetry run python examples/replicate-core-example/scripts/replicate.py -v
```

To create a quarto md doc, use the command

```
quarto render examples/replicate-core-example/docs/output.qmd
```
