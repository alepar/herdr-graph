# hg-66u.84 pre-run evidence: e2e_summary_lands_in_archived_seat_folder

Verdict: pre-run. `content write` was removed on main by 9d5c988 ("feat: remove content write; summarizer uses
request complete --summary-from", 2026-10-31), an ancestor of main 0a84c67; the test on main (0a84c67) still calls it
and was never updated. This run touched neither src/cli/content.rs nor src/cli/request.rs. The test is left
unmodified, per the bead's pre-run branch.

## Step 1: static proof (base c34e4c5)

```
$ git merge-base --is-ancestor 9d5c988 0a84c67 && echo ancestor
ancestor

$ git show 0a84c67:src/cli/content.rs | grep -n 'content write was removed'
31:pub const STUB_MESSAGE: &str = "content write was removed: summaries use 'herdr-graph request complete <rq> --summary-from <file> --covered <a-b>'; instructions use 'herdr-graph plan instructions set …' then 'herdr-graph apply'";
72:            "content write was removed: summaries use 'herdr-graph request complete <rq> --summary-from <file> --covered <a-b>'; instructions use 'herdr-graph plan instructions set …' then 'herdr-graph apply'"

$ git show 0a84c67:tests/e2e_private_herdr.rs | grep -n -A3 '"content",'
1838:        "content",
1839-        "write",
1840-        "--object",
1841-        &seat.id,

$ git log --oneline 0a84c67..c34e4c5 -- src/cli/content.rs src/cli/request.rs
(exit 0)
```

## Step 2: run on the run tree (c34e4c5)

Command (inside toolbox ht-build): `nice cargo test --features private-herdr --test e2e_private_herdr e2e_summary_lands_in_archived_seat_folder`

```
     Running tests/e2e_private_herdr.rs (target/debug/deps/e2e_private_herdr-6626a68d56dee30f)

running 1 test
test e2e_summary_lands_in_archived_seat_folder ... FAILED

failures:

---- e2e_summary_lands_in_archived_seat_folder stdout ----

thread 'e2e_summary_lands_in_archived_seat_folder' (3617816) panicked at tests/e2e_private_herdr.rs:148:9:
["content", "write", "--object", "st_01M6S8SY0P5FRCT21WC6W11F1W", "--rel", "summaries/sess-1.md", "--from", "/tmp/hg-01m6s8sxp4z6a64br94vqsnhtb/summary.md"] failed: herdr-graph: content write was removed: summaries use 'herdr-graph request complete <rq> --summary-from <file> --covered <a-b>'; instructions use 'herdr-graph plan instructions set …' then 'herdr-graph apply'

note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    e2e_summary_lands_in_archived_seat_folder

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 32 filtered out; finished in 2.32s

error: test failed, to rerun pass `--test e2e_private_herdr`
exit 101
```

The panic is at tests/e2e_private_herdr.rs:148 (`cli_ok`) for the `content write` call, with the stub message. It is the
first failure, so every assertion before it passed on the real rig: the summarizer request routed to the scribe, the seat's
tab closed and the seat retired, and the seat folder moved under `/archive/` (this run's retirement and archival paths).

Rig hygiene: the test stopped its own private herdr server; `stop-run-processes --check` reported no `match:` lines.

## Follow-up (not done here)

Test-only repair: replace the `content write` + `request complete --output summaries/sess-1.md` pair with
`request complete <rq> --summary-from <summary.md> --covered 0-<len>`, keeping the `landed` assertion. It needs a
decision that the test is the defect, which this bead does not name.
