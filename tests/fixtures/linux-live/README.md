The full live Wikipedia "Linux" page with the two stylesheets it links, captured 2026-08-28.

This is the input behind the M7 perf gate. `../linux-revision-1371530035.html` is the Parsoid
article body only and carries no external CSS, so it exercises a much smaller cascade; the external
stylesheets are what make the custom cascade expensive, and therefore what M7 exists to attack.

Captured with `curl -L --fail`, from a browser-equivalent request for the desktop Vector 2022 skin.
Nothing here is fetched at test time — `AGENTS.md` forbids a test touching the network, so the
harness feeds these bodies to `PageLoad::deliver` against the resource ids discovery reports.

## page.html

Source: https://en.wikipedia.org/wiki/Linux

SHA-256: bec6db1db90db3460b9750d15f0fee9b50a3f27118d32caee43a45e4ae616f23

## modules.css

The ResourceLoader module bundle the page links first.

Source: https://en.wikipedia.org/w/load.php?lang=en&modules=ext.cite.parsoid.styles%7Cext.cite.styles%7Cext.flaggedRevs.basic%7Cext.phonos.icons%2Cstyles%7Cext.uls.interlanguage%7Cext.visualEditor.desktopArticleTarget.noscript%7Cext.wikimediaBadges%7Cext.wikimediaBadges.ulsV2%7Cext.wikimediamessages.styles%7Cjquery.makeCollapsible.styles%7Cmediawiki.codex.messagebox.styles%7Cmediawiki.skinning.content.parsoid%7Cmediawiki.skins.legacy%7Cskins.vector.icons%2Cstyles%7Cskins.vector.search.codex.styles%7Cwikibase.client.init&only=styles&skin=vector-2022

SHA-256: 1e0c28fd478a83294fbf5347253769cd44bf4bc8a00d2c04bc743dd6ff37a69d

## site.css

Source: https://en.wikipedia.org/w/load.php?lang=en&modules=site.styles&only=styles&skin=vector-2022

SHA-256: 2f9d2333eaed611c06a73d76507da7c451cc6654fe1cee3146bde7b8b0bf41d1
