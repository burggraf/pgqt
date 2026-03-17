# Phase 1 Deep Analysis: What's Blocking 80%?

## Executive Summary

| File | Current | Target | Gap | Status |
|------|---------|--------|-----|--------|
| strings.sql | 73.1% | 75% | 1.9% | ✅ **ACHIEVABLE** |
| join.sql | 70.8% | 80% | 9.2% | ⚠️ **ACHIEVABLE WITH LATERAL** |
| update.sql | 65.3% | 80% | 14.7% | ⚠️ **NEEDS WORK** |
| insert.sql | 57.8% | 80% | 22.2% | ❌ **NOT ACHIEVABLE** |

## Detailed Analysis

### 1. strings.sql (73.1% → 75%)

**Gap: 1.9% (10 statements)**

**Status: EASILY ACHIEVABLE**

The gap is minimal. Likely fixes:
- NULLS FIRST/LAST handling (~25 errors in total log)
- String function improvements
- Minor syntax fixes

**Recommendation:** Minor effort should close this gap.

---

### 2. join.sql (70.8% → 80%)

**Gap: 9.2% (83 statements)**

**Status: ACHIEVABLE WITH LATERAL SUPPORT**

**Key Blocker: LATERAL Subqueries**
- 65 statements fail due to `LATERAL` not supported in SQLite
- This is ~7% of the file
- Can be polyfilled with CTEs (complex but feasible)

**Other Issues:**
- Missing tables (cascade from partition/type failures)
- Syntax errors

**Calculation:**
- Current: 70.8%
- + LATERAL polyfill: +7% → 77.8%
- + Other fixes: +2-3% → ~80%

**Recommendation:** Implement LATERAL → CTE transformation to reach target.

---

### 3. update.sql (65.3% → 80%)

**Gap: 14.7% (44 statements)**

**Status: NEEDS INVESTIGATION**

**Error Breakdown:**
- Syntax error: 61 failures
- No such column: 16 failures
- No such table: 14 failures
- ON CONFLICT: 7 failures
- Policy/syntax: 5 failures
- Views: 4 failures

**Key Issues:**
1. **UPDATE ... FROM v.* syntax** - Not properly handled
2. **Partition-related updates** - Tables don't exist
3. **Policy syntax** - CREATE/DROP POLICY not supported
4. **ON CONFLICT** - Constraint matching issues

**Fixable:**
- UPDATE FROM improvements
- Better table alias handling
- Policy could be partially emulated with RLS

**Recommendation:** Focus on UPDATE FROM patterns and table alias handling.

---

### 4. insert.sql (57.8% → 80%)

**Gap: 22.2% (87 statements)**

**Status: NOT ACHIEVABLE - FUNDAMENTAL LIMITATION**

**Unfixable SQLite Limitations:**

| Category | Count | Reason |
|----------|-------|--------|
| PARTITION tables | 62 | SQLite has no partitioning |
| CREATE RULE | 6 | PostgreSQL rules system |
| CREATE TYPE | 1 | Composite types |
| Data-modifying CTEs | 1 | WITH ... INSERT syntax |
| Row value misused | 3 | Complex row operations |
| **Subtotal** | **73** | |

**Cascade Errors (from above):**

| Category | Count | Reason |
|----------|-------|--------|
| No such table | 51 | Partition tables not created |
| No such column | 12 | Columns from missing tables |
| Table already exists | 2 | Cleanup issues |
| **Subtotal** | **65** | |

**Total Unfixable: ~138 statements (35% of file)**

**Fixable Issues:**

| Category | Count | Fix |
|----------|-------|-----|
| Column count mismatch | 20 | DEFAULT handling |
| Syntax errors | 70 | Investigation needed |
| RETURNING | 1 | Handler improvement |
| **Total** | **91** | |

**Achievable Calculations:**
- **Best case:** Fix all fixable → 77.6%
- **Realistic:** Fix 50% of syntax → 68.5%
- **With test adjustments:** 70.3%

**Recommendation:** 
- **Adjust target to 65-70%** to account for PARTITION limitation
- Focus on DEFAULT handling to gain ~5%

---

## Summary Recommendations

### Immediate Actions

1. **strings.sql**: Minor fixes, should hit 75% easily
2. **join.sql**: Implement LATERAL → CTE polyfill
3. **update.sql**: Fix UPDATE FROM patterns and alias handling
4. **insert.sql**: **Adjust target to 65-70%**

### Revised Targets

| File | Original | Revised | Rationale |
|------|----------|---------|-----------|
| strings.sql | 75% | 75% | Achievable |
| join.sql | 80% | 80% | Achievable with LATERAL |
| update.sql | 80% | 75% | Requires more investigation |
| insert.sql | 80% | 65% | PARTITION = 35% unfixable |

### Investment Priority

1. **High ROI**: LATERAL polyfill (unlocks join.sql)
2. **Medium ROI**: DEFAULT handling (gains ~5% in insert.sql)
3. **Medium ROI**: UPDATE FROM improvements
4. **Low ROI**: Trying to fix partition-related failures