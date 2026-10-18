# Exact threads 2.0.3 qualification evidence — 2026-10-17

Final runtime source: graph `d9a50590c6ca3dccc558664d4ed608eb2153605d`, initialized threads submodule `ded34fa971c18cef81ea1b597097a8ff7cd91eb3`, protocol 9, macOS aarch64. Herdr 0.9.1–0.9.5 each passed37 cases: 185 total; all 15 cleanup gates clean, 190 owned roots removed, no emergency reaping/survivors. Task 4/whole-branch review and integration remain controller gates.

- [Verified evidence](verified-evidence.json): actual binary versions/SHA256, all 15 suite counts/times/cleanups, prebuilt graph/test/threads hashes, config statuses, and final source/artifact preservation assertions.
- [Commands](command-results.json): exact flags, execution heads, exits and durations. Default694/0/1 and no-default 691/0/1 ran at `33be69e`; the final test-only `test-support` fix is covered by ordinary parallel test-support 710/0/1 and all consumers at `d9a5059`. Native config 16 harness passes include 2 explicit early returns; driver 24 passed.
- [Full matrix JSON](matrix.json) and [driver Markdown](matrix.md): every exact case/result and complete owned PID/start/cleanup data. Original absolute log paths are preserved; their contents are mirrored in the archive under the same relative matrix.logs paths.
- [Final official provenance](final-official-provenance.json), [initial official provenance](official-provenance.json), [official asset metadata](herdr-official-assets-preflight.json), [tested pins](herdr-releases-tested.tsv), [remote annotated/peeled tag](threads-official-tag-remote.txt).
- [Frozen source hashes](pinned-source-equivalence.json):427 tracked entries compared at final freeze, concurrent Markdown differences only. Verified evidence additionally proves all 175 code/test/script/manifest/lock/gitlink entries equal after final docs changes. [Original local artifact](artifact-before.json) and final proof preserve its 13624672-byte size, mtime and SHA256; external checkout stays clean/unchanged.
- [Config JSON](config-smoke.json): shell VERIFIED, Claude/Codex SKIPPED because HG_REAL_AGENTS=0. These are not native launch/resume passes.
- [Review copy hashes](review-copies.json) and [verbatim controller rulings](controller-rulings.txt).
- [Preserved trial notes](trial-notes.md): failed default/test-support, deterministic isolation red, intermediate exact-filter assertion failure, interrupted pre-pin check and offline/metadata setup attempts remain their actual outcomes.
- [Raw evidence archive](raw-evidence.tar.gz), [member hashes](raw-evidence-manifest.json):127 byte-identical members, including every Task 4 final/failed/build log, per-suite roots/result/cleanup JSON, prior source/command snapshots, raw official metadata, and exact local workflow/verification scripts. No binaries, credentials, source Git databases or unrelated files are archived.

The full retained clone/cache/log root is `/private/tmp/hg-task4-qualification-534cd4f_`. The archive was reopened and each member's SHA256/length checked against its original bytes. Verify the checked-in bundle with:

```sh
shasum -a 256 -c SHA256SUMS
```

Raw archive member names are relative. Inspect or extract it only into an empty private directory. Historical Task 3 trial logs remain at their original documented locations; October 5/7 evidence and prior proper reports are unchanged. [The integration handoff](../../releases/2026-10-17-threads2-herdr-qualification.md) records exact commands, recovery/rebuild/restart requirements, limits and all rulings. Shell/synthetic fixtures do not qualify native inference/auth/bootstrap/root-agent/summarizer flows, remote continuity, actual restore holds, Linux/Intel runtime, or a live user-session handoff. No live services were restarted, plugin binary replaced, version bumped, branch merged or pushed, or release published.
