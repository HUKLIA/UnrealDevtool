# Free Windows trust options

First record the exact warning. A Defender threat detection, a potentially unwanted application detection, a SmartScreen unknown-download warning and a Smart App Control block require different investigation. Do not assume every warning is a false positive.

## Detection review

1. Record the threat name and affected path from Windows Security protection history.
2. Compare the file SHA-256 with the final release checksum. Matching proves identity, not safety.
3. Review the corresponding source commit and release artifact.
4. For a suspected false detection, submit the exact file through [Microsoft Security Intelligence](https://www.microsoft.com/en-us/wdsi/filesubmission) as a software developer. The app's Self-Check opens this portal. Submission is a separate manual action; the app does not upload files.
5. Retain the submission result and recheck the same artifact after updated security intelligence.

Microsoft's [developer FAQ](https://learn.microsoft.com/en-us/defender-xdr/developer-faq) explains this process. A submission is not a guaranteed approval or a SmartScreen reputation reset.

## Free signing candidate

[SignPath Foundation](https://signpath.org/) offers free signing for qualifying open-source projects. Acceptance requires an application and compliance with its [terms](https://signpath.org/terms.html), including open-source components, team roles, MFA and a signing policy. This repository's MIT license is only one part of eligibility.

The bundled music and Miku artwork are credited as third-party works with rights reserved in README.md. Their current licensing evidence may prevent meeting the all-open-source-component requirements. Resolve distribution rights and eligibility before applying; replacing them with original or appropriately licensed assets is a possible prerequisite. No eligibility or acceptance is claimed, and no account or application has been created.

A self-signed certificate does not establish public publisher trust. Even valid signing does not guarantee that security products will accept a release. See Microsoft's [SmartScreen reputation documentation](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).

## Changes included here

- Manual browser-download option avoids executable self-replacement for users choosing that route.
- Automatic updates refuse releases without a checksum.
- Release notes now use the hash of the final artifact after optional signing.
- App Self-Check links to the free Microsoft submission portal.
- No Defender settings or exclusions are changed.

These are integrity, transparency and support improvements. They do not establish that the original detection is resolved. Verification still needs the original threat name and the final release binary.
