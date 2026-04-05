# Open-Source Project Documentation Best Practices

Research findings compiled from analysis of well-known open-source projects
(Tauri, Electron, Capacitor, MCP Servers) and established community standards
(Keep a Changelog, SemVer, GitHub Open Source Guides).

---

## 1. Essential Documentation Files for an Open-Source Project

Every serious open-source project should carry a baseline set of files at the
repository root. The table below lists them in order of priority.

| File | Purpose | Who reads it | Examples observed |
|---|---|---|---|
| `README.md` | Front door. Project pitch, install, quick-start, links. | Everyone | All projects |
| `LICENSE` (or `LICENSE_MIT` / `LICENSE_APACHE-2.0`) | Legal terms under which the code is shared. | Users, lawyers, CI | Tauri ships dual MIT/Apache-2.0 |
| `CHANGELOG.md` | Human-curated version history. | Users upgrading, maintainers | Tauri (per-crate), Capacitor, Keep a Changelog |
| `CONTRIBUTING.md` | How to contribute: setup, style, PR process. | Contributors | Electron, Capacitor |
| `CODE_OF_CONDUCT.md` | Community behavior standards. | Everyone | Capacitor (custom), Electron (Contributor Covenant) |
| `SECURITY.md` | How to responsibly disclose vulnerabilities. | Security researchers | Tauri, Electron, MCP Servers |
| `ARCHITECTURE.md` | High-level system design and component map. | Contributors, integrators | Tauri |
| `CLAUDE.md` / `AGENTS.md` | AI-assistant context file (see section 6). | AI coding tools | Electron (root + docs/), MCP Servers |

### Recommended GitHub template files

```
.github/
  ISSUE_TEMPLATE/
    bug_report.yml
    feature_request.yml
  PULL_REQUEST_TEMPLATE.md
  dependabot.yml
```

### What the best README files include

Based on analysis of Tauri, Electron, and Capacitor READMEs:

1. **Project name + one-line tagline** -- what it is, in plain language.
2. **Badges** -- CI status, npm/crates.io version, license, chat/docs links.
3. **One-paragraph description** -- expand on the tagline with concrete
   capabilities.
4. **Platform support table** -- a matrix of OS vs. capability (especially
   important for cross-platform projects).
5. **Installation** -- copy-paste-ready commands for all package managers.
6. **Quick-start / minimal example** -- the smallest working code that proves
   the project does what it claims.
7. **Links** -- full docs site, contributing guide, changelog, license.
8. **License line** -- SPDX identifier at the bottom.

### What to leave out of README

- Exhaustive API reference (link to it instead).
- Multi-page tutorials (link to a `/docs` folder or docs site).
- Internal architecture details (belongs in `ARCHITECTURE.md`).
- Detailed platform-specific setup (belongs in platform-specific docs).

---

## 2. What Makes a Great "Getting Started" Guide

A getting-started guide is distinct from a README. It is the first deep page a
new user lands on after deciding to try the project.

### Structure (observed in Electron's tutorial/ directory)

Electron organizes its tutorial as a numbered sequence:

```
tutorial-1-prerequisites.md
tutorial-2-first-app.md
tutorial-3-preload.md
tutorial-4-adding-features.md
tutorial-5-packaging.md
tutorial-6-publishing-updating.md
```

### Principles

1. **Assume zero context.** The reader may be an expert on one platform but new
   to yours. Define terms or link to a glossary (Electron has `glossary.md`).
2. **One concept per page.** Each page should end with a working checkpoint the
   user can verify before moving on.
3. **Copy-pasteable commands.** Every command should be complete and runnable
   as-is. Avoid partial snippets with "..." that require the user to fill in
   blanks.
4. **Show the reward early.** Within the first page, the user should see
   something working (a window, a notification, a running service).
5. **Explain what each step does.** Don't just list commands. A one-sentence
   "why" after each step prevents cargo-culting.
6. **Link forward and backward.** "Previous: Prerequisites / Next: Preload
   Scripts" navigation aids discovery.
7. **Call out platform differences inline.** Use badges or callout blocks:
   `[Android]`, `[iOS]`, `[Desktop]` when steps diverge.
8. **Prerequisites checklist.** List exact versions of tools required (Rust
   1.77+, Node 20+, Android SDK 34, Xcode 15).

### Anti-patterns to avoid

- Walls of prose with no code.
- Code that references files not yet created.
- Assuming the reader has read other docs first.
- Out-of-date screenshots without dates or version labels.

---

## 3. How API Reference Documentation Should Be Structured

### Per-symbol page pattern (Electron model)

Electron organizes API docs as one Markdown file per module in `docs/api/`:

```
docs/api/
  app.md
  browser-window.md
  clipboard.md
  dialog.md
  ipc-main.md
  ...
  structures/           # Shared data types
    browser-window-options.md
    cookie.md
    ...
```

Each file follows this internal structure:

```
# Module Name

> Stability: [Stable | Experimental | Deprecated]

## Class: ModuleName (if applicable)

### `module.method(arg1, arg2)`

<!-- YAML history block (Electron-specific) -->

- `arg1` String - Description of arg1
- `arg2` Number (optional) - Description of arg2

Returns `Promise<void>` - Description of return value.

Detailed description of what the method does, including:
- Side effects
- Platform-specific behavior (call out with badges)
- Events triggered

### Events

#### Event: 'event-name'
- `parameter` Type - Description

### Static Methods
### Instance Methods
### Properties
```

### Best practices from real projects

1. **One concept per heading.** Methods, events, properties each get their own
   `###` heading. This makes deep-linking reliable.
2. **Type signatures in backticks.** `method(param: Type): ReturnType` in the
   heading line for scanability.
3. **Every parameter gets a description.** Include type, whether optional, and
   default value. Use a consistent bullet format.
4. **Include a realistic example** per method or per page. Not toy examples --
   real-world usage patterns.
5. **Document platform differences inline.** If `startService()` behaves
   differently on Android vs iOS, state it right in the method description,
   not in a separate section.
6. **Stability indicators.** Mark APIs as Stable / Experimental / Deprecated.
7. **Cross-references.** Link related methods, events, and types with
   relative Markdown links.
8. **Machine-readable metadata.** Electron uses YAML blocks for API history
   (added PR, deprecated PR). OpenAPI / JSON Schema serves a similar purpose
   for REST APIs.

### For a Tauri plugin specifically

A Tauri plugin API reference should include:

- **Rust trait documentation** -- the `BackgroundService` trait with `init()`,
  `run()`, type parameters, and lifecycle semantics.
- **Rust struct documentation** -- `ServiceContext<R>`, `Notifier`, `StartConfig`,
  `ServiceError` variants.
- **TypeScript API documentation** -- `startService()`, `stopService()`,
  `isServiceRunning()`, `onPluginEvent()` with parameter and return types.
- **Permissions reference** -- every permission identifier, what it allows,
  and which commands require it.

---

## 4. Platform-Specific Documentation

### The challenge

Cross-platform projects must document behavior that differs across Android,
iOS, Windows, macOS, and Linux -- without drowning users in details irrelevant
to their platform.

### Recommended structure: shared core + platform supplements

```
docs/
  getting-started.md          # Shared for all platforms
  android.md                  # Android-specific setup and quirks
  ios.md                      # iOS-specific setup and quirks
  desktop.md                  # Windows + macOS + Linux
  permissions.md              # Permission model (shared + per-platform)
  troubleshooting.md          # Platform-tagged FAQ
```

### What each platform page should cover

| Section | Android | iOS | Desktop |
|---|---|---|---|
| Prerequisites | SDK version, NDK, JDK, emulator setup | Xcode version, provisioning | System libs, build tools |
| Permissions / Manifest | AndroidManifest.xml entries | Info.plist entries, entitlements | None (or OS-specific) |
| Configuration | Gradle settings | Podfile / SPM settings | Cargo.toml / tauri.conf.json |
| Behavior differences | Foreground Service, START_STICKY | BGTaskScheduler, ~30s limit | Standard Tokio task |
| Known limitations | Background restrictions by OEM | System kills after ~30s bg | N/A |
| Debugging | `adb logcat`, Android Studio | Xcode console, device logs | `cargo log`, devtools |
| Distribution | Google Play policies | App Store review guidelines | OS packaging (MSIX, dmg, AppImage) |

### Presentation patterns from real projects

**Capacitor** keeps platform code in separate top-level directories (`android/`,
`ios/`) with per-platform README files and CHANGELOGs.

**Electron** uses platform-specific tutorial pages:
- `native-code-and-electron-cpp-linux.md`
- `native-code-and-electron-cpp-win32.md`
- `native-code-and-electron-objc-macos.md`
- `native-code-and-electron-swift-macos.md`
- `mac-app-store-submission-guide.md`
- `windows-store-guide.md`
- `snapcraft.md` (Linux packaging)

**Tauri** documents platform differences inline within its main docs site,
using callout blocks and platform badges.

### Key principles

1. **Never say "see the OS docs."** Reproduce the essential steps inline with
   your project's context.
2. **Platform compatibility matrix in README.** A quick-glance table that
   shows feature support across all platforms.
3. **Tag troubleshooting entries by platform.** `[Android] App crashes on
   Samsung devices when...`
4. **Version-pin your platform requirements.** "Android SDK 34+", "Xcode 15+",
   not "recent version of Xcode."

---

## 5. Migration Guides and Changelogs

### Changelog best practices (Keep a Changelog standard)

Source: https://keepachangelog.com

**Format:**

```markdown
# Changelog

All notable changes to this project will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.2.0] - 2025-03-15

### Added
- New `startService({ serviceLabel })` option for Android notification text.

### Changed
- `stopService()` now waits for graceful shutdown (up to 5s) before returning.

### Deprecated
- `isRunning()` -- use `isServiceRunning()` instead. Will be removed in 2.0.

### Removed
- Removed `restartService()` (use stop + start).

### Fixed
- Fixed race condition when calling `startService()` twice rapidly on Android.

### Security
- Updated `tokio` to patch CVE-2025-XXXXX.
```

**Rules observed across projects:**

1. **Changelogs are for humans**, not machines. Never dump git logs.
2. **Each entry should be understandable** without reading the commit or PR.
3. **Include PR/commit links** for traceability (Tauri includes full URLs).
4. **Date format: YYYY-MM-DD** (ISO 8601).
5. **Reverse chronological order** -- newest version first.
6. **Include version comparison links** at the bottom:
   `[1.2.0]: https://github.com/org/repo/compare/v1.1.0...v1.2.0`
7. **Per-crate / per-package changelogs** in monorepos (Tauri does this for
   each crate in `crates/*/CHANGELOG.md`).

### Migration guide best practices (from Electron's breaking-changes.md)

Electron maintains `docs/breaking-changes.md` -- a single file that catalogs
every breaking change with:

- **Change type classification:**
  - `API Changed` -- code will throw
  - `Behavior Changed` -- silent behavioral difference
  - `Default Changed` -- old default no longer applies
  - `Deprecated` -- still works, emits warning, will be removed
  - `Removed` -- gone entirely

- **Structure per entry:**
  ```markdown
  ## Planned Breaking API Changes (43.0)

  ### Behavior Changed: Dialog methods default to Downloads directory

  The `defaultPath` option for the following methods now defaults to the
  user's Downloads folder when not explicitly provided:

  - `dialog.showOpenDialog`
  - `dialog.showSaveDialog`

  Previously, the OS file dialog determined the initial directory.
  Now, Electron explicitly sets it to Downloads.

  ### Migration

  If your app relied on the OS remembering the last directory, explicitly
  pass `defaultPath: ''` to restore the old behavior.
  ```

**Principles:**

1. **One migration guide per major version.** Don't make users piece together
   changes from scattered release notes.
2. **Show before/after code.** Concrete snippets beat abstract descriptions.
3. **Provide automated migration tools** when possible (codemods, CLI commands).
4. **Severity classification** lets users assess impact at a glance.
5. **Include the "why."** Explain the motivation, not just the change.
6. **Link to the PR/issue** for full context.
7. **Keep a "Planned Breaking Changes" section** for upcoming versions so users
   can prepare.

---

## 6. AI-Friendly Documentation (CLAUDE.md / AGENTS.md)

### The landscape in 2025-2026

AI coding assistants now read project-level instruction files automatically:

| File | Tool | Convention |
|---|---|---|
| `CLAUDE.md` | Claude Code (Anthropic) | Root + subdirectory files |
| `AGENTS.md` | OpenAI Codex / MCP tools | Per-module files |
| `.cursorrules` | Cursor | Single root file |
| `.github/copilot-instructions.md` | GitHub Copilot | Repository settings |
| `.windsurfrules` | Windsurf / Codeium | Single root file |

### What a great CLAUDE.md / AGENTS.md contains

Based on analysis of Electron's `CLAUDE.md` (root) and `docs/CLAUDE.md`, plus
MCP Servers' `src/everything/AGENTS.md`:

1. **Project overview** -- 2-3 sentences: what the project is, what language(s),
   what it does.
2. **Directory structure** -- ASCII tree with one-line descriptions of each
   top-level directory and key files.
3. **Build / test / lint commands** -- exact commands, copy-pasteable. The MCP
   Servers AGENTS.md lists: `npm run build`, `npm run watch`, `npm run start:stdio`.
4. **Code style rules** -- naming conventions, import order, formatting rules,
   what linter to run.
5. **Architecture patterns** -- how components connect, factory patterns,
   trait contracts, extension points.
6. **Constraints and "do not" rules** -- negative constraints that prevent the
   AI from making common mistakes (e.g., "DO NOT add business logic to the
   plugin", "Lock the Mutex briefly -- create token, store, drop lock, THEN
   spawn task").
7. **Common workflows** -- step-by-step for the most frequent tasks.
8. **Key files table** -- a quick-reference mapping of file paths to purposes.
9. **Environment variables** -- names, purposes, default values.
10. **Common issues / troubleshooting** -- patterns the AI will encounter.

### Pattern observed from Electron's CLAUDE.md

Electron's root `CLAUDE.md` is notably comprehensive (~200 lines) and includes:
- Directory structure with annotation
- Build tools setup and essential commands (in a table)
- Typical development workflow (step-by-step)
- Patches system explanation
- PR labeling conventions (semver labels, backport targets)
- Code style references
- CI/CD information
- Common issues with fixes

The `docs/CLAUDE.md` is domain-specific -- it focuses on API history migration,
a narrow task that contributors frequently need help with.

### Pattern observed from MCP Servers' AGENTS.md

More concise (~80 lines) and focused on:
- Build/test/run commands
- Code style guidelines (naming conventions, patterns)
- How to extend the server (where to put new tools, resources, prompts)
- Rules for adding new features

### Recommendations for writing one

1. **Be specific, not generic.** "Use camelCase for variables" is better than
   "follow good naming conventions."
2. **Include concrete file paths.** "Tools live in `src/everything/tools/`" is
   better than "see the tools directory."
3. **State what NOT to do.** Negative constraints are the most valuable part
   of these files.
4. **Keep commands current.** Stale commands erode trust with the AI and human
   readers alike.
5. **Version it with the code.** These files are living documentation; update
   them when architecture changes.

---

## 7. License and Governance Documents

### Essential files

| File | Purpose | Standard |
|---|---|---|
| `LICENSE` | Grant of rights to use, modify, distribute. | MIT, Apache-2.0, GPL-3.0, etc. |
| `CODE_OF_CONDUCT.md` | Community behavior norms. | Contributor Covenant v2.x is most common |
| `SECURITY.md` | Vulnerability reporting process. | GitHub Security Advisory, HackerOne |
| `CONTRIBUTING.md` | Contribution process and legal agreements. | DCO (Developer Certificate of Origin) or CLA |
| `GOVERNANCE.md` (optional) | Decision-making process, roles. | Used by larger projects |
| `MAINTAINERS.md` (optional) | Who maintains what. | List of maintainers with areas of responsibility |

### License selection

For a Tauri plugin (Rust + TypeScript):

- **Dual license MIT OR Apache-2.0** -- Tauri itself uses this. Maximum
  compatibility, minimal friction. The Apache-2.0 grant includes patent
  rights; MIT is simplest permissive license.
- Include an SPDX identifier in README: `SPDX-License-Identifier: MIT OR
  Apache-2.0`
- If the project is purely a library, avoid GPL/AGPL (copyleft creates
  adoption friction for downstream users).

### Code of Conduct

The **Contributor Covenant** (https://www.contributor-covenant.org/) is the
most widely adopted standard. Capacitor uses a custom CoC; Electron uses the
Contributor Covenant. Either approach works.

Key elements:
- Scope (what spaces the CoC covers)
- Standards (expected behavior)
- Enforcement (who enforces, how to report)
- Consequences (what happens for violations)
- Contact (email or form for reports)

### SECURITY.md

Both Tauri and Electron provide clear security policies:

- **Supported versions table** (which versions get security fixes)
- **Reporting process** (private disclosure via GitHub Security Advisories,
  not public issues)
- **Response timeline** (Tauri: 90-day coordinated disclosure)
- **Attribution policy** (credit for reporters)
- **Bounty program** (if applicable; both Tauri and Electron note limited
  financial resources)

### CONTRIBUTING.md

The best contributing guides (Electron, Capacitor) include:

1. **Welcome statement** -- friendly, sets the tone.
2. **Code of Conduct reference** -- "This project adheres to..."
3. **Issue vs. Discussion guidance** -- when to use each.
4. **Development setup** -- step-by-step local environment instructions.
5. **Code reproduction expectations** -- what a good bug report looks like.
6. **Branch strategy** -- which branches exist, which to target.
7. **PR process** -- labeling, review workflow, semver labels.
8. **Commit message format** -- conventions (Conventional Commits, etc.).
9. **Testing requirements** -- what must pass before merge.
10. **Style guide references** -- where to find coding standards.
11. **Dependency policy** -- who can update deps, process for requesting.
12. **Language policy** -- (Electron: "We accept issues in any language.")

---

## Summary: Recommended File Checklist for This Project

For the `tauri-plugin-background-service` project specifically:

### Must have (already present)
- [x] `README.md` -- comprehensive, includes platform table, installation, usage
- [x] `PROMPT.md` -- AI assistant instructions for implementation

### Must have (missing)
- [ ] `LICENSE` or `LICENSE_MIT` + `LICENSE_APACHE-2.0`
- [ ] `CHANGELOG.md`
- [ ] `CONTRIBUTING.md`
- [ ] `CODE_OF_CONDUCT.md`
- [ ] `SECURITY.md`

### Should have
- [ ] `CLAUDE.md` or `AGENTS.md` -- AI-friendly context file for contributors
- [ ] `.github/ISSUE_TEMPLATE/` -- bug report and feature request templates
- [ ] `.github/PULL_REQUEST_TEMPLATE.md`
- [ ] `docs/getting-started.md` -- expanded getting-started guide
- [ ] `docs/api-reference.md` -- detailed API docs for Rust trait + TypeScript
- [ ] `docs/android.md` -- Android-specific setup and behavior
- [ ] `docs/ios.md` -- iOS-specific setup and behavior
- [ ] `docs/migration-guide.md` -- for future major versions

### Nice to have
- [ ] `ARCHITECTURE.md`
- [ ] `docs/troubleshooting.md`
- [ ] `docs/examples/` -- expanded examples directory
- [ ] `GOVERNANCE.md` (if project grows a community)
- [ ] `MAINTAINERS.md`

---

## Sources

- [Tauri Repository](https://github.com/tauri-apps/tauri) -- ARCHITECTURE.md, SECURITY.md, per-crate CHANGELOGs
- [Electron Repository](https://github.com/electron/electron) -- CLAUDE.md, CONTRIBUTING.md, breaking-changes.md, tutorial structure, glossary
- [Capacitor Repository](https://github.com/ionic-team/capacitor) -- README.md, CONTRIBUTING.md, CODE_OF_CONDUCT.md
- [MCP Servers Repository](https://github.com/modelcontextprotocol/servers) -- AGENTS.md, SECURITY.md
- [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) -- Changelog format standard
- [Semantic Versioning](https://semver.org/) -- Version numbering convention
- [GitHub Open Source Guides](https://opensource.guide/) -- General best practices
- [Contributor Covenant](https://www.contributor-covenant.org/) -- Code of Conduct template
