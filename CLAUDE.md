# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

MewsFeed is a Twitter-like decentralized social media application built on Holochain. "Mews" are the equivalent of tweets, with features like replies, quotes, retweets ("mewmews"), likes ("licks"), mentions, hashtags, and cashtags.

## Human Interaction

Teach me one thing every return from a prompt that will incrementally help me work on this project without AI.

## Safety

Always avoid putting the home directory, current user, or other references to non-reproducible or personal environment in source files, documentation, and commit messages.
Avoid absolute paths, or any path that includes any directories above the source repository, preferring relative paths, or path fragments relative to the source root. Common locations of executable system binaries could be an exception to the rule, for example in the shebang of a script.

When making a file executable, never `chmod +x`, instead `chmod ug+x`.

## Project Management

The project-management directory contains descriptions of work to be done and that has been done;
work that is out of scope and high-level 

### the specdoc

For any code generation there should be a Markdown or (preferably) AsciiDoc file in the project-management directory detailing tasks to be done, and testing to verify correctness, *before* generating code. This is the **specdoc**.
This specdoc may be a ticket in project-management/tickets, or another file indicated by the human driver.

Specdocs should document not just what the software should do, but also why design decisions were made.

#### specdoc file name convention
The specdoc should follow a naming convention where the first five characters of the file name are most significant hexidecimal digits of unix time starting with 0, eg `0697C`, on 2026 Jan 30.
Incrementing these five digits to future dates or backdating is fine.
The middle of the specdoc file name should be a very short descriptive name.
The suffix of the specdoc file name is TODO for future work,
PWIP for present work in progress (sometimes PWOR for present work),
or DONE for completed work, or AXED for work that has been decided against,
then the file type suffix. Case doesn't matter.

#### the final Up Next section
A generated specdoc should end with a brief section describing future work, called "Up Next".
It should read like the abstract of a paper, being brief, high level, covering important topics,
and should not go into detail. Up Next may reference a few tickets.
Up Next may simply say that this particular work is complete,
 or that there are a number of choices of what to do next,
 which may or may not be enumerated.

#### generating the specdoc
When generating these specdocs,
 first write the plan from plan mode into the project-management/plans directory,
 and link to it in the beginning of the generated specdoc.
While the plan is for the agent and the human may read it,
 the specdoc is for the human, and the agent should make sure it is consistent with the work, or ask to update it.

#### pseudocode and single-line steps
Generated specdoc content should not include code beyond single-line method signatures,
 and in-line monospace names of entities in the code,
 or expressions that support a point,
 but it should contain short pseudocode snippets, and ordered lists of steps.
Copy items in ordered lists of steps into the code as single-line comments.

The specdoc is for a human to understand why the work is being done with regards to previous work,
 supporting future work, and how the task supports the overall project goals.

#### digestible length
A generated specdoc should be under 256 lines,
 though they may grow incrementally as directed by, or edited by, a human.
To keep the specdoc short enough, the scope of present work may be limited,
 utilizing the Up Next section, or creating a ticket in project-management/tickets.
A specdoc should name types and other entities in code in the same repository,
 and it should link to the source file in which they can be found.
Before the code is written, these links will be broken, which is fine.

#### human verification
A generated specdoc should enable a human to evaluate the completion and correctness of the code generation.
Human-written tickets (or other documents) may serve as a specdoc,
 in which case some of these constraints (like file name) may not be met, which is fine,
 but ask the human if you may update it to enable the human to verify the work.

#### consistency
When the code is generated (work is done),
 evaluate and report any inconsistency with the specdoc
 and other obviously relevant documentation in the project management directory.

## tickets
Open tickets by creating a file in project-management/tickets.

## bash scripts

Create shell scripts with the `.bash` file extension and reference bash, not sh in the shebang.

## Development Environment

Requires Nix with Holochain development environment:
```bash
nix develop   # Enter the development shell
npm install   # Install JS dependencies
```

## Common Commands

### Build
```bash
npm run build:happ          # Build the Holochain app (DNA + zomes)
npm run build:zomes         # Build only the Rust zomes to WASM
```

### Test
```bash
npm run test                # Build happ and run all tryorama tests
npm t -w tests              # Run tests only (assumes happ already built)
```

To run a single test file:
```bash
cd tests && npx vitest run src/mewsfeed/mews/agent-mews.test.ts
```

### Development
```bash
npm start                   # Run 2 agents with UI (cleans sandbox, builds happ)
AGENTS=3 npm start          # Run with custom number of agents
```

### Linting & Formatting
```bash
npm run lint                # Lint TypeScript and Vue files
cargo fmt                   # Format Rust code
cargo clippy                # Rust linting
```

### Package for Distribution
```bash
npm run package             # Creates mewsfeed.webhapp in workdir/
```

## Architecture

### Holochain DNA Structure

Single DNA (`mewsfeed`) with paired integrity/coordinator zomes:

| Integrity Zome | Coordinator Zome | Purpose |
|---------------|------------------|---------|
| `mews_integrity` | `mews` | Core mew content, hashtags, cashtags, mentions |
| `profiles_integrity` | `profiles` | User profiles |
| `follows_integrity` | `follows` | Follow relationships |
| `likes_integrity` | `likes` | Licks (likes) on mews |
| `agent_pins_integrity` | `agent_pins` | Pinned mews |
| - | `ping` | Network connectivity check |

### Shared Crates (`crates/`)

- `mews_types` - Core types: `Mew`, `MewType`, `FeedMew`, `Notification`
- `follows_types` - Follow relationship types
- `hc_link_pagination` - Pagination utilities for link queries
- `hc_call_utils` - Cross-zome call helpers
- `hc_zome_input` - Input validation utilities

### Key Domain Types

`MewType` enum defines mew variants:
- `Original` - New mew
- `Reply(ActionHash)` - Reply to another mew
- `Quote(ActionHash)` - Quote with comment
- `Mewmew(ActionHash)` - Repost (retweet)

### UI (`ui/`)

Vue 3 application with:
- Vite build tooling
- Pinia for state management (with persistence)
- TanStack Query for data fetching/caching
- Tailwind CSS + DaisyUI for styling
- `@holochain/client` for Holochain communication
- `@holochain-open-dev/profiles` for profile components

### Tests (`tests/`)

Testing includes Tryorama-based integration tests using Vitest. These tests run with 10-minute timeout (`vitest.config.ts`) and are not parallelized due to Holochain sandbox requirements.

Networking tests include bash scripts over localhost for flexibility, inspection, and composability.

Testing of the ActivityPub integration will include verification of interop with common Fediverse software including Mostodon, Pixelfed, Loops, and recent releases of popular options.

## Configuration

- DNA properties in `dnas/mewsfeed/workdir/dna.yaml`: `mew_characters_min: 10`
- Rust line width: 100 chars (`rustfmt.toml`)
- WASM target: `wasm32-unknown-unknown`
