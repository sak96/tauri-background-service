# Migration Guide

This guide covers breaking changes and migration steps between major versions of `tauri-plugin-background-service`.

> **Note:** This guide will be populated when breaking changes are introduced in future versions. The plugin is currently at **0.1.0** and follows [Semantic Versioning](https://semver.org/). Breaking changes will be documented here with before/after code examples.

## Change Type Classification

| Type | Meaning | Migration Required |
|------|---------|--------------------|
| **API Changed** | Function signature, parameter, or return type changed | Yes — update call sites |
| **Behavior Changed** | Runtime behavior changed without signature change | Possibly — verify assumptions |
| **Default Changed** | Default value for a configuration option changed | Possibly — check if relying on old default |
| **Deprecated** | Feature still works but will be removed in a future version | Recommended — plan migration |
| **Removed** | Feature no longer exists | Yes — replace with alternative |

## Migration Template

When a breaking change is documented, it follows this format:

```markdown
### [VERSION] Change Title (Change Type)

**Affected:** Who is affected (e.g., "All users", "Android only")

**Before:**

```rust
// Old API or configuration
```

**After:**

```rust
// New API or configuration
```

**Steps:**
1. Concrete action to migrate
2. Another concrete action
```

## Version History

_No versions with breaking changes yet._

## Planned Breaking Changes

_No planned breaking changes at this time._

When planning a breaking change, document it here before release so users can prepare. Include the target version, the planned change, and the recommended migration path.
