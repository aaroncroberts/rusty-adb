# Session Handoff Generator

This prompt generates a clean handoff summary for continuing work in a new session.

---

## SESSION HANDOFF: Teamatorium Business Plan 2026

### ✅ Completed This Session

**Section 02: Company Description**
- Created comprehensive 14-page [company-description.md](docs/business-plan-2026/02-company-description/company-description.md)
- Documented 4-year history (2021-2025) with $386k revenue, 55 staff, 40% margins
- Closed task: bizzy-awj ✅

**Financial Modeling: Faire Revenue Model**
- Created detailed [faire-revenue-model.md](docs/business-plan-2026/08-financial-models/faire-revenue-model.md)
- Documented 4-year performance: $89k (2022) → $386k (2025) = 334% growth
- Per-event economics: $67.5k-$77k revenue, 21-40% margins
- Model B projection: Events baseline $100k-$150k (Years 1-3)

---

### 🎯 Immediate Next Steps (Strategic Sequence)

**Priority: Financial Modeling (6 models remaining)**

Use this exact prompt to continue:

```
Continue building the financial models for Teamatorium's business plan.

CONTEXT:
- We're in the financial modeling phase (strategically optimal sequence: financials → narratives)
- Faire Revenue Model is complete (✅ documents 4-year actual performance)
- 6 financial models remain before we can finalize narrative sections

NEXT TASK: Build Wholesale Revenue Model
- Location: docs/business-plan-2026/08-financial-models/wholesale-revenue-model.md
- Foundation: Research R-003 (PCC, Metropolitan Market requirements)
- Model structure: accounts, pints per account per week, wholesale pricing, distribution costs
- Projection: $120k Year 1 → $400k Year 3 for Model B
- Use Faire model format as template

REMAINING TASKS (in order):
1. Wholesale Revenue Model ← START HERE
2. Mobile Vending Model (farmers markets, breweries)
3. 5-Year Pro Forma P&L (integrate all revenue models)
4. Cash Flow Statement (monthly Year 1, quarterly Years 2-5)
5. Break-Even Analysis (by channel and model)
6. Unit Economics (per pint, per customer, per event)

After financial modeling complete → Finalize Sections 03-06, 09 with actual numbers
```

---

### 📊 Work in Progress

**Beads Tasks:**
- bizzy-w0k [P1] [task] in_progress - Extract and map content from vision doc
- bizzy-3af [P1] [task] in_progress - Create business plan skeleton

**10 ready tasks available** (run `bd ready` to see full list)

---

### 📁 Key Files to Reference

**Completed:**
- [01-executive-summary/executive-summary.md](docs/business-plan-2026/01-executive-summary/executive-summary.md)
- [02-company-description/company-description.md](docs/business-plan-2026/02-company-description/company-description.md)
- [07-funding-request/funding-request-model-b.md](docs/business-plan-2026/07-funding-request/funding-request-model-b.md)
- [07-funding-request/funding-request-model-c.md](docs/business-plan-2026/07-funding-request/funding-request-model-c.md)
- [07-funding-request/funding-request-model-d.md](docs/business-plan-2026/07-funding-request/funding-request-model-d.md)
- [08-financial-models/faire-revenue-model.md](docs/business-plan-2026/08-financial-models/faire-revenue-model.md) ← JUST COMPLETED

**Research Available:**
- [03-market-analysis/research/R-001-market-sizing.md](docs/business-plan-2026/03-market-analysis/research/R-001-market-sizing.md)
- [03-market-analysis/research/R-003-wholesale-accounts.md](docs/business-plan-2026/03-market-analysis/research/R-003-wholesale-accounts.md) ← Use for wholesale model
- [03-market-analysis/research/R-005-mobile-vending.md](docs/business-plan-2026/03-market-analysis/research/R-005-mobile-vending.md) ← Use for mobile model

---

### 💡 Strategic Guidance

**User's explicit instruction:** "ALWAYS proceed in the strategically optimal sequence"

This means:
1. ✅ Complete ALL financial models FIRST (foundation)
2. ❌ Do NOT write narrative sections with placeholder numbers
3. ✅ THEN finalize Sections 03-06, 09 with complete financial data
4. Quality over speed

**Revenue Attribution:**
- Total 2025 revenue: $386k (all brands)
- Creamatorium (ice cream): ~$270k-$309k (70-80%) ← Business plan focus
- Other brands: ~$77k-$116k (20-30%) ← Continue independently

**Model B Core Assumptions:**
- Events: $100k-$150k baseline (proven channel, maintains presence)
- Wholesale: $120k Y1 → $400k Y3 (primary growth driver)
- Mobile: $60k Y1 → $150k Y3 (secondary growth driver)
- Total Creamatorium: $280k Y1 → $700k Y3

---

### 🔧 Session Close Checklist

Before ending next session, run:
```bash
git status              # Check changes
git add <files>         # Stage completed work
bd sync                 # Sync beads
git commit -m "..."     # Commit with message
bd sync                 # Final beads sync
git push                # Push to remote
```

---

**Session closed cleanly. Ready to continue with Wholesale Revenue Model.**
