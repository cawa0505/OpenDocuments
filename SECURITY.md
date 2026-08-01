# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 1.x     | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly:

1. **Do NOT open a public GitHub issue**
2. Email: security@opendocuments.dev (or create a private security advisory on GitHub)
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

We will acknowledge receipt within 48 hours and aim to release a fix within 7 days for critical issues.

## Security Considerations

- **Secrets Isolation**: OpenDocuments does not persist cloud API keys (e.g. OpenAI, Anthropic) in standard environment variables at the OS level. All BYOK keys are safely stored in your local workspace SQLite table and loaded dynamically into memory only during runtime.
- **Local Sandbox Execution**: All document parsing (`opendoc-parser-*`) is modularized. To prevent container-level escapes, it is highly recommended to run the unified binary on a secure, restricted user account.
- **Workspace Access**: OpenDocuments workspace databases (`opendocuments.db`) and LanceDB vector spaces are created locally on disk inside the user's home folder (`~/.opendocuments/`). Ensure proper file permissions on this folder so that other system users cannot read raw database contents.
