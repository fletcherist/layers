# Testing Rule

Before considering any task done, run `cargo test` and make sure all tests pass. Do not skip this step.

# New Feature Testing Rule

When implementing a new feature that mutates `App` state, write at least one test using `App::new_headless()`. This applies to any method that adds, removes, or modifies App struct fields (objects, waveforms, audio_clips, regions, components, etc.). Does NOT apply to pure UI/rendering features. Tests go in `src/tests/` as new files or added to existing ones. At minimum: one happy-path test proving the feature works.

# Changelog Rule

After completing each task, prepend a short entry to the top of `CHANGELOG.md` in the project root describing what was done. Each entry should include today's date and a brief description. Format: `- YYYY-MM-DD: description`. Keep it really simple, one line max. Also add codebase length to the end, example (14.3k loc). Do not type date every time — only when it changed and it's another day.

To count lines of code, run: `find src -name '*.rs' | xargs wc -l | tail -1`

# Version Bump Rule

After completing each task, increment the patch version in `Cargo.toml`. Only touch the third number (e.g. 0.2.1 → 0.2.2). The first two numbers are managed by the user.
Also change a version at README.md version badge.

