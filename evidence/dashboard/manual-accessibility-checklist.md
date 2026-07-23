# Dashboard manual accessibility release checklist

Status: not run

This is a release-candidate input, not evidence that a manual check passed. Run
each row against the real packaged artifact, retain findings, and identify the
tester before changing its result.

Use this Markdown file while executing the checks, then record both required
environments in `manual-accessibility.template.json`. The JSON submission is
the signed interchange form: it binds every row to the full commit,
`Cargo.lock` hash, candidate workflow run, target archive name and SHA-256.
Safari/VoiceOver uses the Apple Silicon candidate; Firefox/Orca uses the Linux
candidate. Do not reuse one environment's result for the other.

## Environment

- Commit:
- Target:
- Operating system:
- Browser and version:
- Tester:
- Date:

## Checks

Use `not run`, `pass`, or `fail`. Attach the retained evidence URI and findings
for every row; do not infer manual results from axe or other automation.

| Check | Result | Evidence / findings |
| --- | --- | --- |
| Keyboard-only operation | not run | |
| Visible and restored focus | not run | |
| VoiceOver with Safari | not run | |
| Orca with Firefox | not run | |
| 200 percent zoom | not run | |
| 320 CSS pixel reflow | not run | |
| WCAG 2.2 AA contrast | not run | |
| Pointer target size | not run | |
| Reduced motion | not run | |
| Refresh and error announcements | not run | |
| No color-only meaning | not run | |

## Sign-off

- Overall result: not run
- Responsible maintainer:
- Approval date:
- Notes:
