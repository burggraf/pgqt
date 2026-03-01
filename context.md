## Summary

I've read the `pi-planning-with-files` skill and written detailed findings to `/Users/markb/dev/postgresqlite/context.md`.

### Key Takeaways:

**What this skill does:** Provides Manus-style file-based planning using persistent markdown files (`task_plan.md`, `findings.md`, `progress.md`) as "working memory on disk" to extend your limited context window.

**When to use it:** Complex tasks with 3+ steps, research projects, or anything requiring >5 tool calls.

**Core principle:** Treat filesystem as permanent storage (disk) to persist information that won't fit in context (RAM).

**Critical rules:**
1. Create `task_plan.md` FIRST, before any execution
2. The 2-Action Rule: Save findings after every 2 operations to prevent data loss
3. Read before deciding: Refresh goals from plan file before major decisions
4. Log ALL errors and never repeat failed actions
5. Use the 3-Strike Error Protocol: Try 3 different approaches, then escalate

**Practical workflow:** Initialize files → work in phases → update after each phase → read plan before decisions → handle errors systematically → use session-catchup script for multi-session continuity.

The skill includes templates and helper scripts in the skill directory that you can copy to your project.