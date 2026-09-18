# Local personal Elo

The corrected scope is an honor-based personal estimate in Kai. The historical
plan directory name does not imply verified results. There is no server or service.

Implementation:

- Add a small panel to Settings → You for manual opponent Elo and win/loss/draw.
- Start at 1200, use fixed K=32 ordinary Elo, document rounding and validation.
- Save locally using existing platform paths and browser localStorage patterns.
- Keep 50 explanatory results with undo and re-entry for corrections.
- Preserve independent P2P gameplay and leave Agni unchanged.
- Verify calculation, bad inputs, persistence/reload, save failures, history
  retention, and correction consistency; check native/web builds and formatting.
- Verify the panel at narrow and wide sizes with pointer/touch input, and record
  any runtime/device limitations in the implementation review.

No task-owned server/service or verification artifacts existed in the clean
starting Kai checkout. The earlier Agni task worktree had no changes or commits.
Existing unrelated networking and project work are preserved.

See [design](../../wiki/design/personal-elo.md) and
[review](review/implementation.md). Commit to the task branch, create a
`review/trusted-p2p-elo` handoff branch, and release the implementation worktree
after review. Do not merge to main.
