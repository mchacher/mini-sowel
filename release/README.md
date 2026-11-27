# release/

What a release is signed and built with, apart from the secret.

- `update-key.pub`: the Ed25519 **public** key of the update file (64 hex characters and a newline,
  as `--update-keygen` prints it after `public key: `). It is committed by the maintainer through a
  pull request, once, when the key is made; `scripts/release.sh` compiles it into the radio image
  (`SOLITE_UPDATE_PUBLIC_KEY`) and checks the signature against it, and the release workflow
  refuses to run without it. It is not a secret. The private key is never in this repository.

- `recipe-key.pub`: the Ed25519 **public** key that signs the script recipes (spec 042), a second
  key, never the update key's: its public half is committed here the same way, compiled into the
  radio image (`SOLITE_RECIPE_PUBLIC_KEY`), and `scripts/release.sh` and the release workflow refuse
  to build without it. The private half is on the maintainer's computer and in the secret of the
  recipes' repository, never in this repository.

How to make the keys and where they live: [docs/release.md](../docs/release.md).
