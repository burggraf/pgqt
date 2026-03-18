# PGQT Compatibility Improvement Plans

This directory contains detailed implementation plans for improving PGQT's PostgreSQL compatibility from **66.68%** to **85%+**.

## Quick Start

1. **Read the main plan:** [COMPATIBILITY_IMPROVEMENT_PLAN.md](../COMPATIBILITY_IMPROVEMENT_PLAN.md)
2. **Pick a phase:** Start with Phase 1 (JSON/JSONB) for highest impact
3. **Follow the plan:** Each phase has detailed sub-phases with verification checklists

## Plan Structure

```
plans/
├── README.md                          # This file
├── PHASE_1_JSON_JSONB.md              # Phase 1: JSON/JSONB Functions (Highest Impact)
├── PHASE_2_INTERVAL.md                # Phase 2: Interval Type & Functions
├── PHASE_3_AGGREGATES.md              # Phase 3: Boolean & Bitwise Aggregates
├── PHASE_4_INSERT.md                  # Phase 4: INSERT Improvements
├── PHASE_5_CTE.md                     # Phase 5: CTE (WITH Clause) Enhancements
├── PHASE_6_FLOAT.md                   # Phase 6: Float/Real Edge Cases
└── PHASE_7_ERROR_HANDLING.md          # Phase 7: Error Handling Alignment
```

## Phase Overview

| Phase | Focus | Est. Score Gain | Priority | Duration |
|-------|-------|-----------------|----------|----------|
| 1 | JSON/JSONB Functions | +7-10% | **Critical** | 2-3 weeks |
| 2 | Interval Type | +3-4% | **High** | 1-2 weeks |
| 3 | Boolean/Bitwise Aggregates | +4-5% | **High** | 1 week |
| 4 | INSERT Improvements | +1-2% | Medium | 3-4 days |
| 5 | CTE Enhancements | +1-2% | Medium | 3-4 days |
| 6 | Float Edge Cases | +2-3% | Low | 2-3 days |
| 7 | Error Handling | +3-5% | Low | 3-4 days |
| **Total** | | **+21-31%** | | **7-9 weeks** |

## Implementation Rules

Every sub-phase **MUST** complete these 5 items before being considered done:

1. ✅ **Build Succeeds**
   ```bash
   cargo build --release
   ```

2. ✅ **No Build Warnings**
   ```bash
   cargo clippy --release
   ```

3. ✅ **All Tests Pass**
   ```bash
   ./run_tests.sh
   ```

4. ✅ **Documentation Updated**
   - Code comments (rustdoc)
   - User documentation in `docs/`
   - README updates if needed

5. ✅ **CHANGELOG.md Updated**
   - Follow existing format
   - Include all new features and fixes

## Suggested Implementation Order

### For Maximum Impact (Recommended)
1. **Phase 1** - JSON/JSONB (brings most user value)
2. **Phase 2** - Interval (time/date handling is critical)
3. **Phase 3** - Aggregates (analytics support)
4. **Phase 4** - INSERT (CRUD completeness)
5. **Phase 5** - CTE (query organization)
6. **Phase 6** - Float (edge cases)
7. **Phase 7** - Error Handling (strictness)

### For Quick Wins
If you want to see faster progress:
1. **Phase 3** - Aggregates (fastest to implement)
2. **Phase 4** - INSERT (focused scope)
3. **Phase 5** - CTE (leverages existing SQLite support)
4. **Phase 1** - JSON/JSONB (highest impact but most work)

## Using These Plans with Subagents

Each phase file is designed to be handed to a subagent for implementation:

1. **Create a worktree** for isolation:
   ```bash
   git worktree add .worktrees/phase1-json -b feature/phase1-json
   cd .worktrees/phase1-json
   ```

2. **Copy the plan** to the working directory:
   ```bash
   cp plans/PHASE_1_JSON_JSONB.md .
   ```

3. **Assign to subagent** with the plan file as reference

4. **Review checkpoints** at each sub-phase completion

5. **Merge when complete** and move to next phase

## Tracking Progress

Update this section as phases are completed:

| Phase | Status | Started | Completed | Final Score |
|-------|--------|---------|-----------|-------------|
| 1 | 🔲 Not Started | - | - | - |
| 2 | 🔲 Not Started | - | - | - |
| 3 | 🔲 Not Started | - | - | - |
| 4 | 🔲 Not Started | - | - | - |
| 5 | 🔲 Not Started | - | - | - |
| 6 | 🔲 Not Started | - | - | - |
| 7 | 🔲 Not Started | - | - | - |

**Current Overall Score:** 66.68%

**Target Overall Score:** 85%+

## Key Files Reference

### Source Files to Modify
- `src/handler/mod.rs` - Register new functions
- `src/transpiler/func.rs` - Function call handling
- `src/transpiler/expr.rs` - Expression/operator handling
- `src/transpiler/dml.rs` - DML statement handling

### New Files to Create
- `src/json.rs` or extend `src/jsonb.rs`
- `src/interval.rs`
- `src/aggregates.rs` (or add to existing)
- `tests/*_tests.rs` - Integration tests
- `docs/*.md` - Documentation

### Test Commands
```bash
# Build
cargo build --release

# Check warnings
cargo clippy --release

# Run all tests
./run_tests.sh

# Run specific test
cargo test --test json_function_tests

# Run compatibility suite
cd postgres-compatibility-suite
source venv/bin/activate
python3 runner_with_stats.py
```

## Questions?

Refer to the main [COMPATIBILITY_IMPROVEMENT_PLAN.md](../COMPATIBILITY_IMPROVEMENT_PLAN.md) for:
- Detailed rationale for each phase
- Risk assessment
- Timeline estimates
- Success metrics

Or consult the [AGENTS.md](../AGENTS.md) for:
- Project structure
- Testing infrastructure
- Development workflow
