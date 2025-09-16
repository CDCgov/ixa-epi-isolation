This document provides a guide to set up quarto to run in this repo. This readme assumes you are working on a linux or WSL system and are writing quarto markdown files (`.qmd`) which utilize blocks of python code for automatic figure generation in PDFs.

## Installation 
Install (quarto)[https://docs.posit.co/resources/install-quarto.html]. Use `quarto check` to confirm that the installation is successful. 

## Additional Setup Notes
- If using VSCode install the quarto extension and read the (quarto documentation)[https://quarto.org/docs/get-started/hello/vscode.html].
- If necessary install jupyter with the command `python3 pip -m install jupyter`. You can determine if it is necessary by running `quarto check jupyter`
- Ensure jupyter is added to `pyproject.toml` file by running `poetry add jupyter`.
- Make sure the poetry environment is activated using one of the following commands, depending on the poetry version: 
`$(poetry env activate)` or `source $(poetry env info --path)/bin/activate`. The poetry environment needs to be activated at the beginning of a session before rendering a quarto document.
- Ensure tinytex is installed by running `quarto install tinytex`
- Python code blocks' working directory is set to the location of the `.qmd` file.

## Rendering the Document
To render the document run `quarto render path/to/file/output.qmd`
