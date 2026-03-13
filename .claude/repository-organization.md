# Repository Organization Rules

## 📁 Directory Structure

The repository MUST maintain this exact structure:

```
lib-todd-data/
├── .claude/                    # Agent rules and guidelines
│   ├── rules.md               # Development workflow rules
│   └── repository-organization.md  # This file
├── .github/                    # GitHub configuration
│   └── workflows/             # CI/CD pipelines
├── docs/                       # Documentation (user-facing)
│   ├── README.md              # Documentation index
│   ├── getting-started.md     # Quick start guide
│   ├── CONTRIBUTING.md        # Contribution guidelines
│   ├── guides/                # User guides
│   ├── api/                   # API reference
│   ├── architecture/          # Architecture docs
│   └── examples/              # Code examples
├── packages/                   # Monorepo packages
│   ├── todd-data/             # Main library
│   └── example-app/           # Example usage
├── scripts/                    # Build and utility scripts
├── README.md                   # Project overview
├── CHANGELOG.md               # Version history
├── LICENSE                    # License file
└── WBS.md                     # Work Breakdown Structure
```

## 🎯 File Organization Principles

### 1. Documentation Files

**Location Rules:**
- User-facing docs → `docs/`
- Development docs → `docs/CONTRIBUTING.md`, `.claude/`
- Package docs → `packages/*/README.md`
- Root README → Project overview only

**Naming Convention:**
- Use kebab-case: `getting-started.md`, `api-reference.md`
- Use descriptive names: NOT `doc1.md`, YES `mongodb-guide.md`

**Required Files:**
- `README.md` - Must exist in root
- `docs/README.md` - Documentation index
- `docs/getting-started.md` - Quick start
- `docs/CONTRIBUTING.md` - How to contribute
- `CHANGELOG.md` - Version history

### 2. Source Code Files

**Location Rules:**
- Library code → `packages/todd-data/src/`
- Tests → `packages/todd-data/src/**/__tests__/` or `*.test.ts`
- Type definitions → Colocated with implementation

**Naming Convention:**
- Use kebab-case for files: `storage-adapter.ts`
- Use PascalCase for classes: `MongoDBAdapter`
- Test files: `*.test.ts` or `__tests__/*.ts`

**Structure:**
```
packages/todd-data/src/
├── core/                  # Core abstractions
│   ├── __tests__/        # Core tests
│   ├── storage-adapter.ts
│   ├── errors.ts
│   └── index.ts
├── types/                # Shared types
├── adapters/             # Storage implementations
│   ├── mongodb/
│   └── d1/
├── cli/                  # CLI tool
└── testing/              # Test utilities
```

### 3. Configuration Files

**Location: Root only**
- Package management: `package.json`, `pnpm-workspace.yaml`
- TypeScript: `tsconfig*.json`
- Linting: `.eslintrc.js`, `.markdownlint.json`
- Git: `.gitignore`, `.gitattributes`
- Node: `.nvmrc`, `.npmrc`

**Never nest config files** except in packages with their own package.json

### 4. Build Artifacts

**MUST be gitignored:**
- `dist/` - Build output
- `coverage/` - Test coverage
- `node_modules/` - Dependencies
- `*.log` - Log files
- `.DS_Store` - Mac files

**Location:**
- Build output → `packages/*/dist/`
- Coverage → `packages/*/coverage/`

## 📝 Documentation Organization

### Documentation Categories

1. **Getting Started** (`docs/getting-started.md`)
   - Installation
   - Quick start
   - Basic examples

2. **Guides** (`docs/guides/`)
   - Feature-specific tutorials
   - Best practices
   - Common patterns

3. **API Reference** (`docs/api/`)
   - Auto-generated from TSDoc
   - One file per module
   - Include examples

4. **Architecture** (`docs/architecture/`)
   - Design decisions
   - Patterns used
   - Trade-offs

5. **Examples** (`docs/examples/`)
   - Complete, runnable examples
   - Real-world use cases
   - Copy-paste ready

### Documentation Maintenance Rules

**ALWAYS update docs when:**
- Adding new features
- Changing APIs
- Fixing bugs that affect usage
- Adding examples

**Documentation must be:**
- Up-to-date with code
- Tested (examples must run)
- Cross-referenced
- Searchable

## 🔧 Maintenance Automation

### Pre-Commit Checks

Before EVERY commit, verify:

```bash
# 1. Documentation links are valid
find docs -name "*.md" -exec markdown-link-check {} \;

# 2. Examples compile
pnpm build

# 3. Tests pass
pnpm test

# 4. Lint passes
pnpm lint
```

### Monthly Cleanup

On the first of each month:

1. Review WBS.md - Update progress
2. Review CHANGELOG.md - Ensure complete
3. Review docs/ - Remove outdated content
4. Review package.json - Update dependencies
5. Review .github/workflows - Update actions

## 🚫 Anti-Patterns to Avoid

### DO NOT:

1. **Create orphan documentation**
   - Every doc must be linked from docs/README.md
   - No standalone docs without clear purpose

2. **Mix concerns**
   - Don't put build scripts in src/
   - Don't put docs in packages/
   - Don't put source in scripts/

3. **Create deep nesting**
   - Maximum 3 levels deep in docs/
   - Maximum 4 levels deep in src/

4. **Use unclear names**
   - ❌ `utils.ts`, `helpers.ts`, `stuff.md`
   - ✅ `query-builder.ts`, `mongodb-guide.md`

5. **Duplicate content**
   - One source of truth for each concept
   - Use links to reference, don't copy

6. **Leave stale files**
   - Delete unused files
   - Update or remove outdated docs
   - Clean up commented code

## ✅ Quality Checklist

### New Feature Checklist

When adding a feature:

- [ ] Code implemented in correct package
- [ ] Tests added with >80% coverage
- [ ] TSDoc comments on public APIs
- [ ] README updated if API changed
- [ ] Getting started guide updated if needed
- [ ] Guide added to `docs/guides/` if complex
- [ ] Example added to `docs/examples/`
- [ ] CHANGELOG.md updated
- [ ] WBS.md updated if applicable

### New Documentation Checklist

When adding documentation:

- [ ] Placed in correct category
- [ ] Linked from docs/README.md
- [ ] Cross-referenced from related docs
- [ ] Examples are tested and runnable
- [ ] Markdown linting passes
- [ ] Links are valid
- [ ] No spelling errors
- [ ] Clear headings and structure

## 🤖 Agent Rules

### Documentation Management

**ALWAYS:**
1. Update docs/README.md when adding new documentation
2. Keep table of contents updated
3. Add examples for new features
4. Link related documentation

**NEVER:**
1. Create documentation outside docs/
2. Leave broken links
3. Create docs without examples
4. Skip updating the index

### File Organization

**ALWAYS:**
1. Follow the directory structure exactly
2. Use prescribed naming conventions
3. Keep related files together
4. Maintain separation of concerns

**NEVER:**
1. Create new top-level directories without approval
2. Mix documentation with source code
3. Nest configuration files unnecessarily
4. Create files in wrong locations

### Cleanup Protocol

**Weekly:**
- Check for orphan files
- Verify documentation links
- Review WBS.md progress

**Before Session End:**
- All files in correct locations
- No temporary files committed
- Documentation index updated
- Links validated

## 📋 Repository Health Metrics

### Green Signals ✅

- All docs linked from README
- No broken links in documentation
- All examples compile and run
- Test coverage >80%
- No linting errors
- Clean directory structure
- Up-to-date CHANGELOG

### Red Signals ❌

- Orphan documentation files
- Broken documentation links
- Examples that don't run
- Outdated API documentation
- Files in wrong locations
- Missing or stale CHANGELOG
- Unclear file organization

## 🔄 Continuous Improvement

This document should be:
- Reviewed quarterly
- Updated when patterns emerge
- Simplified when possible
- Enforced consistently

**Last Updated:** 2026-02-12
**Review Due:** 2026-05-12
