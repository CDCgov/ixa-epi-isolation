This document provides a guide to set up quarto to run in this repo. This readme assumes you are working on a linux or WSL system and are writing quarto markdown files (`.qmd`) which utilize blocks of python code for automatic figure generation in PDFs.

## Installation 
To install quarto run the follow commands.

### Step 1: Download the tarball
```
wget https://github.com/quarto-dev/quarto-cli/releases/download/v1.7.34/quarto-1.7.34-linux-amd64.tar.gz
```
### Step 2: Extract the files

Extract the contents of the tarball to the location where you typically install software (e.g. `~/opt`). For example:
```
mkdir ~/opt
tar -C ~/opt -xvzf quarto-1.7.34-linux-amd64.tar.gz
```

### Step 3: Create a symlink

Create a symlink to `bin/quarto` in a folder that is in your path. If there is no such folder, you can create a folder such as `~/.local/bin` and place the symlink there. For example:

For example:

```
mkdir ~/.local/bin
ln -s ~/opt/quarto-1.7.34/bin/quarto ~/.local/bin/quarto
```

### Step 4: Check quarto on path
If you can run `quarto -v` at this point, jump ahead to the next step.
Otherwise, ensure that the folder where you created a symlink is in the path. For example:
```
( echo ""; echo 'export PATH=$PATH:~/.local/bin\n' ; echo "" ) >> ~/.profile
source ~/.profile
```

### Step 5: Check installation
Use `quarto check` to confirm that the installation is successful.

## Additional Setup Notes
- If using VSCode install the quarto extension.
- Ensure juypter is added to `pyproject.toml` file by running `poetry add juypter`.
- Make sure the poetry environment is activated using one of the following commands, depedning on the poetry version: 
`$(poetry env activate)` or `source $(poetry env info --path)/bin/activate`
- Ensure tinytex is installed by running `quarto install tinytex`
- Python code blocks' working directory is set to the location of the `.qmd` file.

## Rendering the Document
To render the document run `quarto render path/to/file/output.qmd`
