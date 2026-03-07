---
name: code-dev
description: >
  Guide the agent through code development workflow: analyze project,
  collect context, plan changes, execute modifications, and verify results.
  Use when users request code changes, new features, refactoring, or bug fixes.
---

# Code Development Skill

This skill defines the complete code development workflow for the agent. Follow these steps in order when handling any code development task.

## Workflow Overview

1. **Analyze Task** — Understand the request and scan the project
2. **Collect Context** — Gather relevant code and dependencies
3. **Plan Changes** — Create a modification plan and present it to the user
4. **Execute Modifications** — Apply code changes in dependency order
5. **Verify Results** — Build, test, and auto-fix if needed

---

## Step 1: Analyze Task

Before writing any code, understand the project and the user's intent.

1. Parse the user's request to identify what needs to change (new feature, bug fix, refactor, etc.)
2. Call `analyze_code` with `action: scan_project` to get the project structure, type, and top-level symbols
3. Read project coding standards files if they exist:
   - `.editorconfig` — indentation, charset, line endings
   - `rustfmt.toml` / `.rustfmt.toml` — Rust formatting rules
   - `.prettierrc` / `.prettierrc.json` — JavaScript/TypeScript formatting
   - `.clang-format` — C/C++ formatting
   - `pyproject.toml` `[tool.black]` / `[tool.ruff]` section — Python formatting
   - `.eslintrc` / `eslint.config.js` — JavaScript/TypeScript linting rules
4. Note the project type (Rust/Cargo, Node/npm, Python/pip, etc.) and its build system

All generated code must conform to the discovered coding standards.

## Step 2: Collect Context

Gather the code context needed to make accurate modifications.

1. Call `analyze_code` with `action: search_context` using keywords from the user's request
2. Review the returned code snippets and referenced symbols
3. Identify:
   - Files that need to be created or modified
   - Import/dependency relationships between those files
   - Module registration files that may need updating (e.g., `mod.rs`, `index.ts`, `__init__.py`)
4. If the context is insufficient, call `search_context` again with refined queries
5. Use `read_file` to load full contents of files you plan to modify

## Step 3: Plan Changes

Present a clear plan before making any modifications.

**Before writing any code, show the user:**

1. A list of files to be created or modified, for example:
   ```
   Files to modify:
   - src/tools/mod.rs        (add module declaration)
   - src/tools/new_tool.rs   (create: new tool implementation)
   - src/cli/helpers.rs      (register new tool)
   ```
2. A brief summary of what changes each file will receive
3. The dependency order in which files will be modified

Wait for the user to confirm or adjust the plan before proceeding. If the user indicates urgency or says to go ahead, proceed without waiting.

## Step 4: Execute Modifications

Apply changes following these rules:

### Dependency Order

Execute multi-file modifications in dependency order to minimize intermediate compilation errors:

1. **Data models and types** — structs, enums, interfaces, type definitions
2. **Core implementations** — functions, methods, trait implementations
3. **Module registration** — `mod.rs`, `index.ts`, `__init__.py`, `lib.rs`
4. **Integration points** — wiring, configuration, initialization code
5. **Tests** — test files and test utilities

### Import/Use Declarations

When generating or modifying code, always include the necessary import/use declarations:

- For Rust: add `use` statements at the top of the file, grouped by crate (std, external, local)
- For TypeScript/JavaScript: add `import` statements
- For Python: add `import` / `from ... import` statements
- Check existing imports in the file to avoid duplicates

### Module Registration

When adding new modules, update the relevant registration files:

- **Rust**: Add `pub mod new_module;` to `mod.rs` or `lib.rs`
- **TypeScript**: Add `export` to `index.ts` barrel files
- **Python**: Update `__init__.py` with imports

### Applying Changes

- Use `write_file` for new files
- Use `edit_file` for modifying existing files (precise text replacement)
- **Batch edits**: When a single file needs multiple edits, use the `edits` array parameter to apply them all in one call:
  ```json
  {
    "path": "src/main.rs",
    "edits": [
      { "old_text": "old_code_1", "new_text": "new_code_1" },
      { "old_text": "old_code_2", "new_text": "new_code_2" }
    ]
  }
  ```
  This is preferred over multiple separate `edit_file` calls to the same file — it reduces tool call count and is atomic (all-or-nothing).
- After each file modification, call `show_diff` with the original content to display the unified diff to the user

### Sensitive File Protection

Before modifying any of these file types, **always request explicit user confirmation**:

- `.env`, `.env.*` — environment variables, may contain secrets
- `*.key`, `*.pem`, `*.cert` — cryptographic key files
- `*credentials*`, `*secret*` — files with sensitive naming
- Config files that commonly hold secrets: `config.toml`, `settings.json`, `application.yml` (when they contain password/token/key fields)

Prompt the user with something like:
> "This file may contain sensitive data. Confirm modification of `.env.production`? (y/n)"

## Step 5: Verify Results

After all modifications are complete, verify correctness.

### Build Verification

1. Detect the appropriate build command from the project type:
   - Rust: `cargo check` (fast) or `cargo build`
   - Node: `npm run build` or `npx tsc --noEmit`
   - Python: `python -m py_compile <file>` or project-specific lint
   - Go: `go build ./...`
2. Execute the build command using `exec`
3. Report the result to the user

### Auto-Fix on Build Failure

If the build fails, attempt automatic repair:

1. **Attempt 1**: Analyze the error output, identify the failing file and line, apply a fix, rebuild
2. **Attempt 2**: If still failing, re-read the file context, try an alternative fix approach, rebuild
3. **Attempt 3**: Final attempt with a different strategy, rebuild

**After 3 failed attempts**, stop and report to the user:
- The exact error messages
- What fixes were attempted
- Suggested next steps for manual resolution

Do not retry beyond 3 attempts to avoid infinite loops.

### Test Verification

If the project has a test suite, run relevant tests after a successful build:

- Rust: `cargo test`
- Node: `npm test` or `npx jest`
- Python: `pytest`
- Go: `go test ./...`

Report test results. If tests fail, apply the same auto-fix loop (max 3 retries) for test failures.

---

## Quick Reference: Tool Usage

| Step | Tool | Action |
|------|------|--------|
| Analyze | `analyze_code` | `scan_project` — project structure and symbols |
| Context | `analyze_code` | `search_context` — find relevant code |
| Read | `read_file` | Load full file contents |
| Create | `write_file` | Create new files |
| Modify | `edit_file` | Precise text replacement in existing files |
| Diff | `show_diff` | Display unified diff after modifications |
| Build/Test | `exec` | Run build and test commands |
