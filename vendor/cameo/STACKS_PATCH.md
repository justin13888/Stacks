# Stacks vendor patch

This is a vendored copy of `github.com/justin13888/cameo` at commit
`4f35f75` with one local fix applied:

```
src/unified/conversions/tmdb/{movie,tv,person}.rs
  - popularity:   <var>.popularity     →  Some(<var>.popularity)
  - vote_average: <var>.vote_average  →  Some(<var>.vote_average)
```

Reason: at `master` the progenitor-generated TMDB types changed those
fields to `f64`, but the unified-model fields stayed `Option<f64>`, so
the conversions stopped compiling. The patch above re-establishes the
type match and is safe to upstream as-is.

Once upstream merges an equivalent fix, the workspace `[workspace.dependencies]`
entry in `../../Cargo.toml` can revert to the `git = "..."` form and
this directory can be deleted.
