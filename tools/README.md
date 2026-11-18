# Verification tools

These scripts require the packaged app (or `npm run tauri dev`) to be running
with CDP enabled:

```bash
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" \
  ./src-tauri/target/release/yebian.exe
```

Then:

```bash
npm run verify:resume
npm run verify:cleanup
npm run verify:anchor
```

- `verify:resume` uses a temporary vault, a local OpenAI-compatible mock and a
  saved DeepSeek key. It checks: failed usage is kept, resume only repeats the
  missing call, final usage is 2M/2M ¥80, full retry clears checkpoints, and the
  two-part MiMo ASR failure only retries part 2.
- `verify:cleanup` checks that “clear finished” keeps an error task with
  checkpoints, `clear_temp` reports kept jobs, and explicit cleanup removes it.
- ASR verification is skipped when no ASR key is saved. The mock is never a
  real provider and no request leaves the machine.
- Temporary vaults are named `yebian-verify-*`; the original vault/settings are
  restored in `finally`. If a run is interrupted, check Settings → 数据与存储.

- `verify:anchor` creates a temporary note and annotation, forces an orphan, clicks the relocation button, and asserts the annotation moved to a current block. It needs the app on CDP 9222.
- `probe-production-agent.mjs` starts a small text booklet job with `agent: true`, polls the job and the pending agent task, and prints step states plus the final note path/usage. It needs the dev app on CDP 9222 and a saved AI key; it spends a few cents of model calls.
- `probe-html-booklet.mjs` starts the experimental `html-booklet` chain on the Super Agent transcript; `probe-html-booklet.mjs <job-id>` resumes an existing job (cached chapters are reused). It prints chapter/render/pagecheck/judge steps and final artifacts.
- `node --experimental-strip-types tools/test-agent-gate.mjs` checks the deterministic submit gate: dirty metrics, a patch after the last inspect/review, review before a patch, and a missing review are all rejected.
- `test-scrollbar-flash.mjs` checks that the tree keeps `yb-scroll` while collapsed and after expand-all (no native bar flash), programmatic scroll / route reset does not reveal the overlay, while a real wheel does; a scroll-revealed bar must survive pointer movement for its 900ms idle window.
- `test-graph-scrollbar.mjs` opens 关联 → 全屏关系图, forces the fixed `.yb-scrollbar` visible, and asserts it stays hidden over the graph, cannot be re-shown by pointer movement near the old sidebar edge, and is cleared after close.
- `audit-wizard.mjs` opens 制作 → 新建制作, walks 来源/模板/选项/确认, then switches to B站收藏 and audits at 1280×607, 1040×680 and 820×680. It checks fixed text boxes are optically centered (0.75px tolerance), compact badges do not inherit the 1.95 body line-height, text is not clipped, and the footer stays in the viewport. Writes `docs/wizard-audit.json`; exits non-zero on findings.
- `capture-history-shots.mjs` captures the pending queue / source library / 59-fact curation overview; `capture-split-shots.mjs` captures split dialog/result/undo on a real composite fact; `shot-page.mjs <path>` captures the current viewport.
- `node --experimental-strip-types tools/test-annotation-anchor.mjs` runs the
  pure matcher cases without opening the app.
