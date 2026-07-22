# Issue tracker

Tracky currently tracks agent-ready work as local Markdown issues in `docs/issues/`.

Issue files use an ordered numeric prefix (`0001-...md`) so implementation can proceed in dependency order. Each issue is considered ready for an agent when its acceptance criteria are concrete and its blockers are listed.

Maps, specifications, and durable issue records stay in `docs/issues/`. For a
local tracker, `/to-tickets` publishes the approved implementation queue as the
single ordered file `tickets.md` at the repository root.

External pull requests are not a request surface for this local tracker.

For the current PDF inspect/import/review CLI workflow, read `docs/agents/pdf-import-workflow.md` before running commands. Issue completion state remains in each `docs/issues/000N-...md` file's acceptance criteria.

## Wayfinding operations

Wayfinder maps and tickets also live in `docs/issues/` and use the same ordered
numeric filenames. Because the local Markdown tracker has no native child,
assignee, dependency, comment, or closed-state fields, use these conventions:

- A map carries the `wayfinder:map` label and uses the Destination, Notes,
  Decisions so far, Not yet specified, and Out of scope sections.
- Each child ticket links its map under `## Parent map`, carries exactly one
  `wayfinder:<type>` label, and declares `Status: open` plus
  `Assignee: unassigned` when created.
- Claim a ticket before working by replacing `unassigned` with the driver's
  GitHub login. An open, unassigned ticket is unclaimed.
- Express dependencies under `## Blocked by`. A child is on the frontier when
  it is open, unassigned, and every listed blocker is closed.
- Resolve a ticket by adding `## Resolution`, changing its status to `closed`,
  and appending a linked one-line gist to the map's Decisions so far section.
  The Resolution section is the local equivalent of a resolution comment.
- Refer to maps and tickets by their linked titles in human-facing text; the
  numeric filename is only their tracker identity.
