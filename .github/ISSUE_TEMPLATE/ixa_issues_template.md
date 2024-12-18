---
name: ixa issue template
about: A template for writing specs for ixa modeling work
title: ''
labels: ''
assignees: ''

---

This template should be used as an outline. It may not be necessary to fill out every section. Delete this block of text and fill in anything in brackets.

## Goal
[1-3 sentence summary of the issue or feature request. E.g. "We want to improve automatic generation of reports..."]

## Context
[Short paragraph describing how the issue arose and constraints imposed by the existing code architecture]

## Required features

- [Describe each thing you need the code to do to achieve the goal]
- [Example 1: Use a config to set input and output paths]
- [Example 2: Read in some-dataset and output some-transformed-dataset]
- etc...

## Specifications
[A checklist to keep track of details for each feature. At least one specification per feature is recommended. Edit the example below:]

- [ ] EX2: A function that reads data from the `some-api` API and returns the dataset
- [ ] EX2: Another function that inputs the dataset, performs $x$ transform, and outputs $y$
- [ ] EX1: A script that runs the workflow from a config
- [ ] The workflow should run in the VAP from `directory`
- [ ] All functions should have associated unit tests
- [ ] etc. etc.

## Out of scope

- [Things out of scope from this issue/PR]

## Related documents

- [Link to related scripts, functions, issues, PRs, conversations, datasets, etc.]
