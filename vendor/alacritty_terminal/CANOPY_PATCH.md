# Canopy patch for Alacritty Terminal 0.26.0

This directory contains the crates.io source for `alacritty_terminal` 0.26.0,
without its large reference-test fixtures. `Cargo.toml` patches exactly the
locked package to this local source.

Canopy changes two Windows process-creation details:

- quote the executable with the same Windows argument escaping routine used for
  argv, because `CreateProcessW` receives a null application name;
- accept an opaque Job Object handle and add it to the process attribute list
  with `PROC_THREAD_ATTRIBUTE_JOB_LIST`, assigning the ConPTY child atomically.
- sort the Windows environment block and deterministically replace inherited
  case variants with custom values without logging environment values.

The public application API continues to use `alacritty_terminal`. Keep this
patch narrow and re-evaluate it before changing the resolved dependency version.
Validate both the library and its Windows unit tests with the MSVC target.
