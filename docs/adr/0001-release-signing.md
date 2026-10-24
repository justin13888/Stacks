# ADR 0001: Release signing and attestation

- Status: accepted
- Date: 2026-10-22
- Issue: #2

## Context

Releases and tags must be trustworthy, and the maintainer ruled out the usual
way of getting there: "we should be having github do the attestation and not
adding arbitrary signing keys to a specific github account". So there are no
personal GPG or SSH keys, no PATs, no signing-key secrets and no GitHub App.
The only identity the release workflow has is `GITHUB_TOKEN`
(`github-actions[bot]`).

What GitHub signs on its own:

- **Squash merges.** A pull request merged in the web UI produces a commit
  that GitHub signs with its web-flow key, so every commit on `master` is
  verified without anyone holding a key.
- **Commits created through the REST API** with `GITHUB_TOKEN`.

The open question was whether an **annotated tag** that `github-actions[bot]`
creates through the REST API (`POST /git/tags`, then `POST /git/refs`) is
also signed and shown as verified. If it were, the release job could create a
signed `vX.Y.Z` tag before release-please publishes the release.

## Spike

A throwaway workflow, run on the issue #2 branch and never merged, created a
temporary annotated tag `spike-signing-<run_id>` on the branch head with
`GITHUB_TOKEN`, read it back, and deleted it in an `EXIT` trap. It also created
a dangling commit through `POST /git/commits` (no ref) for comparison.

Run `39870503368` (2026-10-22), `GET /repos/justin13888/Stacks/git/tags/768313df7c447f8cd1668937c93ca483476f8bbf`:

```json
{
  "tag": "spike-signing-39870503368",
  "tagger": {
    "name": "github-actions[bot]",
    "email": "41898282+github-actions[bot]@users.noreply.github.com",
    "date": "2026-10-22T23:14:51Z"
  },
  "object": { "sha": "a4d20c4c6661317b7c72e43538d2ccaaa39e6b2f", "type": "commit" },
  "message": "signing spike",
  "verification": {
    "verified": false,
    "reason": "unsigned",
    "signature": null,
    "payload": null,
    "verified_at": null
  }
}
```

The commit created through `POST /git/commits` in the same run came back
`"verified": true, "reason": "valid"`, with a PGP signature and committer
`GitHub <noreply@github.com>`. An earlier run (`39870263378`) returned the
same `unsigned` result for its tag. Both temporary tags were deleted
(`deleted refs/tags/spike-signing-…` in each log), and
`git ls-remote --tags origin` lists no `spike-*` tag.

## Decision

GitHub does **not** sign an annotated tag created through the API, so a
GitHub-verified tag is not available without a key we refuse to hold.
Therefore:

1. The release tag `vX.Y.Z` is the **lightweight** tag release-please creates
   when it publishes the GitHub Release. It points at the release PR's squash
   commit on `master`, which GitHub signed (web-flow) and which shows as
   verified.
2. Trust in what was built comes from **artifact attestations**, which GitHub
   signs through Sigstore with the workflow's OIDC identity:
   - each GHCR image (`ghcr.io/justin13888/stacks-api`, `stacks-web`) gets an
     `actions/attest-build-provenance` attestation on its digest, pushed to
     the registry, plus a buildx SBOM and `mode=max` provenance;
   - each release asset (`openapi.json`, `stacks-compose-vX.Y.Z.tar.gz`,
     `SHA256SUMS`) gets an `actions/attest-build-provenance` attestation.

   The attestations name the repository, the workflow and the commit SHA, so
   they bind each artifact to the signed commit the tag points at.
3. No signed annotated tag is created. Creating an unsigned annotated tag
   would add nothing over the lightweight one.

## Consequences

- `git tag -v v0.1.0` has nothing to verify; the tag itself is unsigned. Verify
  the commit (`git verify-commit v0.1.0^{commit}` with GitHub's web-flow key,
  or the "Verified" badge) and the artifacts:

  ```bash
  gh attestation verify oci://ghcr.io/justin13888/stacks-api:v0.1.0 -R justin13888/Stacks
  gh attestation verify stacks-compose-v0.1.0.tar.gz -R justin13888/Stacks
  ```

- A tag can be moved by anyone with write access, and nothing in the tag
  detects that. The attestation's commit SHA does: an artifact attested for
  one commit fails verification against another. A tag ruleset (no update or
  deletion of `v*`) closes the gap; it is a repository setting, applied by the
  maintainer.
- Artifact attestations on a **private** repository need GitHub Enterprise
  Cloud. The repository is public, so the attestation steps run on the free
  plan; making it private again would make them fail.
- Revisit if GitHub starts signing API-created tag objects: rerun the spike
  (create, read `verification`, delete) and, if verified, have the release job
  create the annotated tag before release-please publishes.
