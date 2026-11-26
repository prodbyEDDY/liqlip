# Security policy

A clipboard manager sees passwords, keys and personal data, so security reports are taken
seriously.

## Reporting a vulnerability

**Please don't open a public issue.** Report the problem privately through GitHub: go to the
repository's **Security** tab and choose **Report a vulnerability**.

Include:
- the Liqlip version (Settings → About Liqlip) and your Windows version;
- what an attacker could do, and the steps to reproduce it;
- a proof of concept, if you have one.

You will get a reply within a week. Once a fix is released, you will be credited unless you
prefer not to be.

## Scope

Examples of issues that matter:
- clipboard content that should have been excluded by a privacy marker, the ignore list or the
  secret detector, but was saved;
- clipboard content written to logs, or left on disk after a deletion;
- input sent to a window other than the confirmed paste target;
- a way for web content in Liqlip's windows to reach the IPC layer or the file system.

## Supported versions

Only the latest release receives fixes.
