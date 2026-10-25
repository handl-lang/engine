# Explanation

## Why this crate exists

Engines are *data-driven*: a language definition supplies schemes and the
engine interprets them (ADR-0044, «language as data»). That needs one shared term
algebra — otherwise every engine invents its own node enum and kits can no longer
swap engines. `hnl-engine` is that algebra, kept dependency-free so both the
kernel (`handl-language`, `handl-value`) and every engine can sit above it
without cycles.

## Where it sits

This crate belongs to the `engine-*` family. Plugins, kits and hosts reach it
through the kit's operation slot, so the engine can be swapped without touching a
language definition.

See the [reference](/engine/reference/) for the API and the
[how-to guides](/engine/how-to/) for tasks.
