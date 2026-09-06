# docs/plan — per-phase implementation plans

`ROADMAP.md` §4 is the **index** (objective + depends-on + status + gate summary
per phase). This directory holds the **detail**: one file per phase, with the
granular, individually-verifiable steps.

## How to use

1. Read `ROADMAP.md` §1 for the current phase.
2. Open this directory's file for that phase (linked from §4).
3. Every 06–40 file carries a **"Architecture frozen at Phase 5"** banner naming
   its **governing ADRs**. The *design* is settled and will not move. At phase
   entry, expand the step outline into concrete steps against those ADRs +
   `ARCHITECTURE.md` + `AI_PIPELINES.md` (exact modules, crate APIs, filenames),
   then do the work.
4. Work the steps in order. Each step has a `Verify:` line — run it, observe the
   result, record it.
5. When every step passes, run the **Verification gate**. Record evidence for each
   check (`CLAUDE.md` Article IV). Then update `ROADMAP.md` §1 + §5 and the plan
   file itself (mark stages done).

## File list

| Phase | File | Detail level |
| ----- | ---- | ------------ |
| — | `pd_product-definition.md` | full |
| 3 | `03_architecture-research.md` | full |
| 4 | `04_adversarial-review.md` | full |
| 5 | `05_architecture-freeze.md` | full |
| 6 | `06_bootstrap.md` | full |
| 7 | `07_application-contracts.md` | outline |
| 8 | `08_configuration.md` | outline |
| 9 | `09_sqlite-persistence.md` | outline |
| 10 | `10_observability.md` | outline |
| 11 | `11_model-registry.md` | outline |
| 12 | `12_model-acquisition.md` | full |
| 13 | `13_resource-manager.md` | full (finalized at phase entry) |
| 14 | `14_model-lifecycle.md` | full (finalized at phase entry) |
| 15 | `15_llama-cpp-adapter.md` | full (finalized at phase entry) |
| 16 | `16_vertical-slice-text-chat.md` | full (finalized at phase entry) |
| 17 | `17_conversation-engine.md` | full (finalized at phase entry) |
| 18 | `18_voice-in.md` | full (finalized at phase entry; split 18.A / 18.B) |
| 18.5 | `18.5_deploy-diagnostics.md` | full (owner-requested insert; deploy + local probes, pulled forward from P37) |
| 19 | `19_voice-out.md` | full (finalized at phase entry; split 19.A / 19.B) |
| 20 | `20_personas.md` | full (finalized at phase entry) |
| 21 | `21_memory.md` | full (finalized at phase entry) |
| 22 | `22_image-generation.md` | full (finalized at phase entry) |
| 23 | `23_model-hot-swapping.md` | outline |
| 24 | `24_scheduler.md` | outline |
| 25 | `25_character-data-model.md` | outline |
| 26 | `26_character-conversations.md` | outline |
| 27 | `27_character-identity.md` | outline |
| 28 | `28_typed-image-actions.md` | outline |
| 29 | `29_character-discovery.md` | outline |
| 30 | `30_ui-ux.md` | outline |
| 31 | `31_performance-audit.md` | outline |
| 32 | `32_offline-audit.md` | outline |
| 33 | `33_fault-injection.md` | outline |
| 34 | `34_security-audit.md` | outline |
| 35 | `35_dependency-audit.md` | outline |
| 36 | `36_maintainability-audit.md` | outline |
| 37 | `37_packaging.md` | outline |
| 38 | `38_final-architecture-audit.md` | outline |
| 39 | `39_permanent-workflow.md` | outline |
| 40 | `40_closing.md` | outline |

"outline" = objective, exclusions, architecture + performance notes, a 5–12 step
outline, the gate, and (since Phase 5) the frozen-architecture banner + governing
ADRs. The concrete per-step detail is filled in at phase entry
(Phase 5.9 re-derives 6–40 against the frozen architecture first).

## Phases 0–2

Complete. No plan file — see `ROADMAP.md` §4 and `docs/verification/01_env_audit.md`.
