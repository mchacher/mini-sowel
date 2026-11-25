# release/

What a release is signed and built with, apart from the secret.

- `update-key.pub`: the Ed25519 **public** key of the update file (64 hex characters and a newline,
  as `--update-keygen` prints it after `public key: `). It is committed by the maintainer through a
  pull request, once, when the key is made; `scripts/release.sh` compiles it into the radio image
  (`SOLITE_UPDATE_PUBLIC_KEY`) and checks the signature against it, and the release workflow
  refuses to run without it. It is not a secret. The private key is never in this repository.

How to make the key and where it lives: [docs/release.md](../docs/release.md).
