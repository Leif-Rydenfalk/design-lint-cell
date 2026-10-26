# design-lint-cell

A ce-net cell: one JSON line in on stdin, one line out. Checks an HTML page against the
`landing-page-design` rules from [elayadesign/ai-design-skills](https://github.com/elayadesign/ai-design-skills)
(MIT): type scale, spacing table, flat backgrounds, all-round borders, one typeface, no italics,
alt text, focus ring, reduced motion, ship requirements.

```
echo '{"html":"<h1>Hi</h1>","css":"h1{font-size:17px}"}' | cargo run -q
{"score":94,"violations":[...],"counts":{...},"not_checked":[...]}
```

Published as `design.lint` on ce-net; call it from any signed-in account:

```
code_call {"cell": "design.lint", "input": {"html": "...", "css": "..."}}
```

Input: `{"html": string, "css": string}` — `css` may be empty when the styles are inline.
Output: `{"score": 0-100, "violations": [{"rule","severity","evidence","fix"}], "counts": {...}, "not_checked": [...]}`.

The score counts distinct rules broken (error 12, warning 6, info 2), not instances.
Rules that need data the page does not contain (e.g. no CSS at all) are listed in `not_checked`
rather than counted.

MIT.
