# Security policy

## Supported versions

replysieve is pre-1.0. Only the latest release gets security fixes.

## Reporting a vulnerability

Please do not open a public issue for a security problem.

Report it privately through GitHub, from this repository's Security tab and then
Report a vulnerability, or at
<https://github.com/zeroonething/replysieve/security/advisories/new>.
The report stays private between you and the maintainer until an advisory is
published.

Include what the issue is, how to reproduce it (a command and an input is ideal),
and the output of `replysieve --version` and your OS.

You can expect an acknowledgement within 72 hours and an assessment within 7
days. You will be credited in the advisory and the release notes unless you
would rather not be named. This is a small project, so please allow reasonable
time for a fix before any public disclosure.

## What counts

- A comment that gets replysieve to approve it by instructing the moderator
  (prompt injection that beats both the pre-check and the model check).
- Comment text written to disk or logs at any verbosity.
- The API key leaking into output, logs or error messages.
- Anything that makes replysieve send data anywhere other than the configured
  OpenRouter endpoint.

A comment that is simply judged wrongly (junk approved, good rejected) is an
accuracy bug. Please open a normal issue with the text and the verdict.
