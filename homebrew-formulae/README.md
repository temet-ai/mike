# homebrew-formulae/ — staging copy, not the tap

`mike.rb` and `mike-mcp.rb` here are generated from the real checksums of the
packaged release archives (see [`docs/RELEASING.md`](../docs/RELEASING.md) and
[`docs/RELEASE_PUBLICATION.md`](../docs/RELEASE_PUBLICATION.md)). They are **not**
hand-edited; the header of each file gives the regenerate command.

The tap is the separate repository `temet-ai/homebrew-mike`. Its `Formula/` folder
carries these two files, updated by `scripts/release/homebrew.py` after the archives
of a `v0.1.0-alpha.N` release are published. Users install with:

```sh
brew tap temet-ai/mike
brew install mike mike-mcp
```
