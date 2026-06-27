# Nidus Logo Assets

`nidus-logo-with-bg.png` is the source image. Regenerate derived assets with:

```bash
node logos/generate.mjs
```

The generator removes the green source background with a deterministic chroma key and writes transparent logo, mark-only, favicon, branded favicon, Open Graph, and website-ready variants.
